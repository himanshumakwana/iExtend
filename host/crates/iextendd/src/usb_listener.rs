//! USB pair listener.
//!
//! Subscribes to libimobiledevice plug events; when an iPad connects, opens
//! a TCP-shaped socket via usbmuxd to its port 7780 and dispatches the
//! resulting stream to the existing simple-pair-v0 handler in
//! `pair_listener::handle_one_usb`.
//!
//! Unlike the Wi-Fi listener (`pair_listener.rs`), the daemon side is the
//! TCP *client* — usbmuxd's tunneling makes the iPad the listener.
//!
//! When libimobiledevice isn't installed (e.g. fresh Windows box without
//! Apple Mobile Device Service, Linux without `libimobiledevice6`), the
//! listener logs a warning and idles until cancelled — Wi-Fi pair keeps
//! working untouched.

use crate::grpc_server::DaemonState;
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Loopback port on the iPad where its NWListener is bound. Hard-coded so
/// no out-of-band negotiation is required — the daemon just connects to
/// (udid, 7780) and the iPad app's listener accepts.
const IPAD_PAIR_LISTEN_PORT: u16 = 7780;

/// How often the polling thread inside ix-usb fires. The daemon's run loop
/// drains events from its receiver at the same cadence so plug latency
/// is bounded by ~1 s.
const DRAIN_INTERVAL: Duration = Duration::from_millis(500);

pub async fn run(state: Arc<RwLock<DaemonState>>, cancel: CancellationToken) -> Result<()> {
    if ix_usb::availability() == ix_usb::LibAvailability::Missing {
        warn!("libimobiledevice unavailable; USB pair disabled (Wi-Fi pair still works)");
        cancel.cancelled().await;
        return Ok(());
    }

    let rx = match ix_usb::subscribe_events() {
        Ok(rx) => rx,
        Err(e) => {
            warn!(err = %e, "failed to subscribe to USB events; USB pair disabled");
            cancel.cancelled().await;
            return Ok(());
        }
    };

    info!("USB pair listener started");

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("USB pair listener stopping");
                return Ok(());
            }
            _ = tokio::time::sleep(DRAIN_INTERVAL) => {}
        }

        // Drain pending events without blocking. crossbeam's try_recv is
        // non-blocking; we keep pulling until the channel is empty.
        while let Ok((event, info)) = rx.try_recv() {
            match event {
                ix_usb::DeviceEvent::Plugged => {
                    on_device_plugged(state.clone(), info).await;
                }
                ix_usb::DeviceEvent::Unplugged => {
                    on_device_unplugged(state.clone(), info).await;
                }
            }
        }
    }
}

async fn on_device_plugged(state: Arc<RwLock<DaemonState>>, info: ix_usb::DeviceInfo) {
    info!(udid = %info.udid, name = ?info.name, "iPad plugged in");
    {
        let mut s = state.write().await;
        s.usb_devices.retain(|d| d.udid != info.udid);
        s.usb_devices.push(info.clone());
    }

    // Spawn a connect task — Apple's usbmuxd takes ~200ms after plug-in
    // before the device's services are reachable, and the iPad app's
    // listener might not be foregrounded yet, so retry with backoff.
    let state = state.clone();
    tokio::spawn(async move {
        for attempt in 0..5 {
            // ix_usb::connect_socket is blocking — run it on a blocking thread
            // so we don't stall the tokio runtime while usbmuxd does its work.
            let udid = info.udid.clone();
            let connect_result = tokio::task::spawn_blocking(move || {
                ix_usb::connect_socket(&udid, IPAD_PAIR_LISTEN_PORT)
            })
            .await;

            match connect_result {
                Ok(Ok(std_stream)) => {
                    if let Err(e) = handle_usb_stream(std_stream, state.clone()).await {
                        warn!(err = %e, "USB pair handler error");
                    }
                    return;
                }
                Ok(Err(e)) if attempt == 4 => {
                    warn!(
                        err = %e,
                        "USB connect failed after 5 attempts; iPad app likely not foregrounded or device not trusted"
                    );
                    return;
                }
                Ok(Err(_)) => {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
                Err(e) => {
                    warn!(err = %e, "spawn_blocking for usb_connect panicked");
                    return;
                }
            }
        }
    });
}

async fn on_device_unplugged(state: Arc<RwLock<DaemonState>>, info: ix_usb::DeviceInfo) {
    info!(udid = %info.udid, "iPad unplugged");
    let mut s = state.write().await;
    s.usb_devices.retain(|d| d.udid != info.udid);
}

async fn handle_usb_stream(
    std_stream: std::net::TcpStream,
    state: Arc<RwLock<DaemonState>>,
) -> Result<()> {
    std_stream.set_nonblocking(true)?;
    let stream = tokio::net::TcpStream::from_std(std_stream)?;

    // The USB-tunneled socket has no real peer address (it's not routed
    // over IP). Synthesize 127.0.0.1:0 for log lines; the existing handler
    // only uses `addr` for diagnostics.
    let addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();

    crate::pair_listener::handle_one_usb(stream, addr, state).await
}
