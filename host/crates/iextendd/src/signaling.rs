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
use ix_rtc::rtc_peer::RtcPeer;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;

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

/// Per-connection task. Owns one `RtcPeer` for the lifetime of the TCP
/// connection: applies the iPad's offer, generates an answer, exchanges
/// ICE candidates, and forwards the encoder's video sink (added in M4d) to
/// the WebRTC peer connection.
///
/// Three concurrent surfaces:
/// - `read_msg(reader)` — drain inbound SignalMsgs from the iPad
/// - `pc.on_ice_candidate` — forward locally-gathered ICE candidates back
///   to the iPad over the same socket
/// - `cancel.cancelled()` — daemon shutdown
pub async fn connection_loop(
    stream: TcpStream,
    addr: SocketAddr,
    _state: Arc<RwLock<DaemonState>>,
    cancel: CancellationToken,
) -> Result<()> {
    let (mut reader, mut writer) = stream.into_split();

    // Create the peer eagerly. ~5ms cost; failing here means the iPad
    // sees the TCP socket close immediately rather than getting a stub
    // answer that goes nowhere.
    let peer = match RtcPeer::new().await {
        Ok(p) => Arc::new(p),
        Err(e) => {
            warn!(?addr, err = %e, "signaling: RtcPeer::new failed");
            return Err(anyhow::anyhow!("RtcPeer::new: {e}"));
        }
    };

    // Outbound queue — webrtc-rs's ICE candidate callback runs on its own
    // task pool and can't write directly to the socket, so it pushes here
    // and the select! below drains.
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<SignalMsg>();

    // Wire ICE candidate gathering. `None` means gathering complete; we
    // forward only `Some(...)` since the iPad's WebRTC.framework treats
    // null candidates as end-of-candidates differently.
    let out_tx_for_ice = out_tx.clone();
    peer.pc
        .on_ice_candidate(Box::new(move |c: Option<RTCIceCandidate>| {
            let tx = out_tx_for_ice.clone();
            Box::pin(async move {
                if let Some(c) = c {
                    if let Ok(json) = c.to_json() {
                        // RTCIceCandidateInit::candidate is the SDP "candidate:..."
                        // string the peer needs to feed to its add_ice_candidate.
                        let _ = tx.send(SignalMsg::Ice {
                            candidate: json.candidate,
                        });
                    }
                }
            })
        }));

    info!(?addr, "signaling: RtcPeer initialised, awaiting offer");

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
                        match peer.apply_offer_create_answer(&sdp).await {
                            Ok(answer_sdp) => {
                                let answer = SignalMsg::Answer { sdp: answer_sdp };
                                if let Err(e) = write_msg(&mut writer, &answer).await {
                                    warn!(?addr, err = %e, "signaling: write answer failed");
                                    return Err(e);
                                }
                                info!(?addr, "signaling: sent answer");
                            }
                            Err(e) => {
                                warn!(?addr, err = %e, "signaling: apply_offer_create_answer failed");
                                return Err(e);
                            }
                        }
                    }
                    Ok(SignalMsg::Answer { sdp }) => {
                        // We're the answerer in the iPad-initiated flow; an
                        // unexpected Answer here means the peer mixed up roles.
                        // Log + ignore rather than crashing the channel.
                        info!(?addr, sdp_len = sdp.len(), "signaling: ignoring unexpected Answer");
                    }
                    Ok(SignalMsg::Ice { candidate }) => {
                        if let Err(e) = peer.add_ice_candidate(&candidate).await {
                            warn!(?addr, err = %e, "signaling: add_ice_candidate failed");
                        }
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
            Some(out) = out_rx.recv() => {
                if let Err(e) = write_msg(&mut writer, &out).await {
                    warn!(?addr, err = %e, "signaling: write outbound failed");
                    return Err(e);
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
