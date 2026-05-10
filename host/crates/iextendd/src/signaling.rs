//! WebRTC signaling channel between daemon and iPad.
//!
//! Runs a TCP listener on a configurable port (default 7783); paired iPads
//! connect after the simple-pair handshake completes and exchange SDP +
//! ICE candidates over this channel. Wire protocol is length-prefixed
//! JSON — same shape Network.framework's `NWConnection` consumes natively
//! on the iPad side, so we don't need a gRPC client over there.
//!
//! # Wire format
//!
//! Each message is a 4-byte big-endian length prefix followed by `len`
//! bytes of UTF-8 JSON. The JSON is one of:
//!
//! ```json
//! {"type": "offer", "sdp": "v=0..."}
//! {"type": "answer", "sdp": "v=0..."}
//! {"type": "ice", "candidate": "candidate:..."}
//! {"type": "bye"}
//! ```
//!
//! # Connection lifecycle
//!
//! 1. iPad opens TCP to `host:7783` after pair completes.
//! 2. iPad sends `offer` → daemon forwards to its WebRTC peer connection.
//! 3. Daemon replies `answer`.
//! 4. Both sides exchange `ice` messages as candidates are gathered.
//! 5. Either side may send `bye` to close the channel.
//!
//! # Auth
//!
//! For now, **any** TCP client that knows the port can open a signaling
//! channel — there is no per-connection authentication. This is fine for
//! v1 since the port is only reachable from the LAN segment and the
//! actual media stream is DTLS-protected by WebRTC's own handshake.
//! Tightening to "only paired iPads" requires the iPad to sign a nonce
//! with the Curve25519 key it generated during pair (see Plan 7's SPAKE2
//! section). Tracked as future work.

#![allow(dead_code)]

use crate::grpc_server::DaemonState;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Default port the signaling listener binds to. Configurable via
/// `Settings.signaling_port` so users behind unusual firewalls can move it,
/// matching the existing `pair_port` knob.
pub const DEFAULT_SIGNALING_PORT: u16 = 7783;

/// One signaling frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SignalMsg {
    Offer { sdp: String },
    Answer { sdp: String },
    Ice { candidate: String },
    Bye,
}

/// A bidirectional signaling channel for one connected iPad. The session
/// loop in iextendd holds one of these per paired iPad currently in a
/// session — channel is dropped when the iPad disconnects.
pub struct SignalingChannel {
    pub peer_addr: SocketAddr,
    /// Rx for messages arriving from the iPad.
    pub inbound: mpsc::Receiver<SignalMsg>,
    /// Tx for messages going out to the iPad.
    pub outbound: mpsc::Sender<SignalMsg>,
}

/// Run the signaling listener until cancelled. Each accepted connection
/// spawns a `connection_loop` task that owns one socket end-to-end.
pub async fn run(state: Arc<RwLock<DaemonState>>, cancel: CancellationToken) -> Result<()> {
    let port = DEFAULT_SIGNALING_PORT;
    let listener = match TcpListener::bind(("0.0.0.0", port)).await {
        Ok(l) => l,
        Err(e) => {
            warn!(err = %e, port, "signaling: failed to bind; signaling disabled");
            cancel.cancelled().await;
            return Ok(());
        }
    };
    info!(port, "WebRTC signaling listener bound");

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("signaling listener stopping");
                return Ok(());
            }
            accept = listener.accept() => {
                match accept {
                    Ok((stream, addr)) => {
                        info!(?addr, "signaling: client connected");
                        let state = state.clone();
                        let cancel = cancel.clone();
                        tokio::spawn(async move {
                            if let Err(e) = connection_loop(stream, addr, state, cancel).await {
                                warn!(?addr, err = %e, "signaling connection error");
                            }
                        });
                    }
                    Err(e) => {
                        warn!(err = %e, "signaling: accept failed");
                    }
                }
            }
        }
    }
}

/// Per-connection task: reads frames from the socket, forwards them to the
/// session loop's inbound queue; writes outbound frames the session loop
/// hands us back. For M3 we don't actually have a session loop yet — the
/// connection_loop just echoes Offer→Answer with a placeholder SDP so the
/// iPad can verify the channel is alive. M4 will replace the echo with a
/// real WebRTC peer-connection bridge.
async fn connection_loop(
    mut stream: TcpStream,
    addr: SocketAddr,
    _state: Arc<RwLock<DaemonState>>,
    cancel: CancellationToken,
) -> Result<()> {
    let (mut reader, mut writer) = stream.split();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = write_msg(&mut writer, &SignalMsg::Bye).await;
                return Ok(());
            }
            msg = read_msg(&mut reader) => {
                match msg {
                    Ok(SignalMsg::Offer { sdp }) => {
                        info!(?addr, sdp_len = sdp.len(), "signaling: received offer");
                        // M3 placeholder: echo back a stub answer so the iPad's
                        // gRPC stream sees both directions working. M4 wires the
                        // real WebRTC peer-connection here.
                        let stub = SignalMsg::Answer {
                            sdp: format!(
                                "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=stub-answer\r\n\
                                 c=IN IP4 127.0.0.1\r\nt=0 0\r\n# offer was {} bytes",
                                sdp.len()
                            ),
                        };
                        write_msg(&mut writer, &stub).await?;
                    }
                    Ok(SignalMsg::Answer { sdp }) => {
                        info!(?addr, sdp_len = sdp.len(), "signaling: received answer");
                    }
                    Ok(SignalMsg::Ice { candidate }) => {
                        info!(?addr, cand = %candidate, "signaling: received ICE candidate");
                    }
                    Ok(SignalMsg::Bye) => {
                        info!(?addr, "signaling: peer said bye");
                        return Ok(());
                    }
                    Err(e) => {
                        warn!(?addr, err = %e, "signaling: read error");
                        return Err(e);
                    }
                }
            }
        }
    }
}

/// Read one length-prefixed JSON frame from the socket.
async fn read_msg<R: AsyncReadExt + Unpin>(reader: &mut R) -> Result<SignalMsg> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 64 * 1024 {
        return Err(anyhow::anyhow!(
            "signaling: frame too big ({len} bytes); refusing"
        ));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    let msg: SignalMsg = serde_json::from_slice(&buf)
        .map_err(|e| anyhow::anyhow!("signaling: JSON parse error: {e}"))?;
    Ok(msg)
}

/// Write one length-prefixed JSON frame to the socket.
async fn write_msg<W: AsyncWriteExt + Unpin>(writer: &mut W, msg: &SignalMsg) -> Result<()> {
    let body = serde_json::to_vec(msg)?;
    if body.len() > 64 * 1024 {
        return Err(anyhow::anyhow!(
            "signaling: outbound frame too big ({} bytes)",
            body.len()
        ));
    }
    let len = (body.len() as u32).to_be_bytes();
    writer.write_all(&len).await?;
    writer.write_all(&body).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip every SignalMsg variant through serde_json to catch field
    /// renames / tag misspellings.
    #[test]
    fn signal_msg_serde_roundtrip() {
        let cases = [
            SignalMsg::Offer {
                sdp: "v=0\r\n...".into(),
            },
            SignalMsg::Answer {
                sdp: "v=0\r\nans".into(),
            },
            SignalMsg::Ice {
                candidate: "candidate:1 1 UDP 1 1.1.1.1 1 typ host".into(),
            },
            SignalMsg::Bye,
        ];
        for case in cases {
            let json = serde_json::to_string(&case).unwrap();
            let back: SignalMsg = serde_json::from_str(&json).unwrap();
            assert_eq!(format!("{case:?}"), format!("{back:?}"));
        }
    }

    #[test]
    fn signal_msg_offer_wire_shape() {
        let msg = SignalMsg::Offer { sdp: "v=0".into() };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, r#"{"type":"offer","sdp":"v=0"}"#);
    }
}
