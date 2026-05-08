//! TCP listener for the pre-WebRTC pairing handshake.
//!
//! Binds an ephemeral high port (the port is published in the mDNS SRV
//! record); accepts a single iPad connection at a time during the active
//! PIN window. The handshake itself is delegated to
//! `ix_rtc::pairing::PairingServer`.
//!
//! Lifecycle:
//!
//! - Tray clicks "Pair" → daemon spawns this listener bound to `0.0.0.0:0`.
//! - mDNS advertise updates with the chosen port.
//! - 60 s PIN timer runs; on timeout or successful pair, the listener
//!   shuts down.
//! - On bad-PIN attempts: count toward [`ix_rtc::pairing::ATTEMPT_LIMIT`].
//!   Lockout means the listener stays alive but rejects further attempts
//!   until a fresh PIN is generated.

#![allow(dead_code)]

use anyhow::Result;
use ix_pair_wire::{PairKind, PairMsg, HEADER_LEN};
use ix_rtc::pairing::PairingServer;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{info, warn};

/// Spawn the pairing listener for the duration of one PIN window. Returns the
/// chosen local port + a JoinHandle that completes when the window ends.
pub async fn spawn(pin: String) -> Result<(u16, tokio::task::JoinHandle<()>)> {
    let listener = TcpListener::bind("0.0.0.0:0").await?;
    let port = listener.local_addr()?.port();
    info!(port, "pairing listener bound");

    let handle = tokio::spawn(async move {
        let deadline =
            tokio::time::Instant::now() + std::time::Duration::from_secs(60);
        let mut server = PairingServer::new(&pin);

        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                info!("pairing window expired");
                return;
            }
            let remaining = deadline - now;
            tokio::select! {
                accept = listener.accept() => {
                    match accept {
                        Ok((stream, addr)) => {
                            if let Err(e) = handle_one(stream, addr, &mut server).await {
                                warn!(?e, "pairing connection failed");
                            }
                        }
                        Err(e) => {
                            warn!(?e, "pair listener accept error");
                            return;
                        }
                    }
                }
                _ = tokio::time::sleep(remaining) => {
                    info!("pairing window expired");
                    return;
                }
            }
        }
    });

    Ok((port, handle))
}

async fn handle_one(
    mut stream: TcpStream,
    addr: SocketAddr,
    server: &mut PairingServer,
) -> Result<()> {
    info!(?addr, "pairing client connected");
    // Read PStart frame.
    let msg = read_msg(&mut stream).await?;
    if msg.kind != PairKind::PStart {
        return Err(anyhow::anyhow!("expected PStart, got {:?}", msg.kind));
    }
    let _session_key = server.complete(&msg.body)?;
    let response = server.make_response();
    write_msg(&mut stream, &response).await?;
    // The remaining flow (PCertReq → PCertOk) lives in
    // `iextendd::session_finalize` once Plan 5 wires the session lifecycle;
    // for Plan 7 the pairing-listener responsibility ends after PResponse.
    Ok(())
}

async fn read_msg(stream: &mut TcpStream) -> Result<PairMsg> {
    let mut header = [0u8; HEADER_LEN];
    stream.read_exact(&mut header).await?;
    let body_len = u16::from_be_bytes([header[6], header[7]]) as usize;
    let mut buf = Vec::with_capacity(HEADER_LEN + body_len);
    buf.extend_from_slice(&header);
    buf.resize(HEADER_LEN + body_len, 0);
    stream.read_exact(&mut buf[HEADER_LEN..]).await?;
    Ok(PairMsg::decode(&buf)?)
}

async fn write_msg(stream: &mut TcpStream, msg: &PairMsg) -> Result<()> {
    let bytes = msg.encode()?;
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    Ok(())
}
