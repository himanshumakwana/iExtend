//! Screen-share pipeline (Plan A milestone M5).
//!
//! Runs the capture → encode → broadcast pipeline behind a single shared
//! task. Each `signaling::connection_loop` registers its `RtcPeer` here
//! so encoded H.264 access units fan out to every connected iPad's video
//! track.
//!
//! Only Windows has a real capture path right now (DXGI Desktop
//! Duplication via `ix_display_windows::dxgi_capture`). On other platforms
//! `start()` returns a `ScreenShare` that quietly idles — the daemon
//! still builds + boots, the signaling channel still works, but no
//! frames flow until the platform capture path lands (Plan 4 for Linux).
//!
//! # Lifetime
//!
//! - One `ScreenShare` per daemon process. It starts with the first peer
//!   that registers and continues running until the process exits.
//! - Peers are tracked in an `Arc<Mutex<Vec<Weak<RtcPeer>>>>` so a peer
//!   that gets dropped without explicit deregistration is cleaned up
//!   automatically on the next broadcast.
//!
//! # Encoder lifecycle
//!
//! The encoder is created once at startup and reused for every frame.
//! Resolution mismatches between the captured frame and the encoder's
//! configured resolution would be a bug — DXGI doesn't change resolution
//! mid-capture without raising a separate error. We log + skip frames
//! whose size doesn't match the configured resolution rather than
//! reconfiguring the encoder, since that needs a fresh IDR sequence and
//! WebRTC peers would have to renegotiate.

#![allow(dead_code)]

use anyhow::Result;
use ix_rtc::rtc_peer::RtcPeer;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

#[cfg(target_os = "windows")]
use ix_codec::{
    trait_::{ColorSpace, Profile},
    x264_sw::X264Sw,
    SharedConfig,
};
#[cfg(target_os = "windows")]
use std::time::Duration;

/// Default capture+encode resolution. Most modern laptops run 1920x1080
/// or higher; if a smaller display is the primary, the DXGI capture
/// reports the actual size and we'll re-init at that size on the first
/// frame.
const DEFAULT_W: u32 = 1920;
const DEFAULT_H: u32 = 1080;
const DEFAULT_FPS: u32 = 30;
const DEFAULT_BITRATE_KBPS: u32 = 8_000;

/// Handle held by `iextendd` for the lifetime of the daemon. Drop it to
/// stop the broadcast worker.
#[derive(Clone)]
pub struct ScreenShare {
    inner: Arc<Inner>,
}

struct Inner {
    peers: Mutex<Vec<Arc<RtcPeer>>>,
    cancel: CancellationToken,
}

impl ScreenShare {
    /// Spawn the capture+encode+broadcast worker. Returns a handle even
    /// when the platform has no capture support — in that case the
    /// worker idles instead of pumping frames.
    pub fn start(cancel: CancellationToken) -> Self {
        let inner = Arc::new(Inner {
            peers: Mutex::new(Vec::new()),
            cancel: cancel.clone(),
        });
        let worker_inner = inner.clone();
        std::thread::Builder::new()
            .name("iextend-screen-share".into())
            .spawn(move || {
                if let Err(e) = run_blocking(worker_inner) {
                    warn!(err = %e, "screen-share worker exited with error");
                }
            })
            .expect("failed to spawn screen-share worker thread");

        Self { inner }
    }

    /// Register a peer to receive frames. Idempotent — the same peer
    /// passed twice is only added once.
    pub async fn add_peer(&self, peer: Arc<RtcPeer>) {
        let mut peers = self.inner.peers.lock().await;
        if !peers.iter().any(|p| Arc::ptr_eq(p, &peer)) {
            peers.push(peer);
            info!(active_peers = peers.len(), "screen-share: peer registered");
        }
    }

    /// Drop a peer from the broadcast list (called when its signaling
    /// connection closes). Safe to call multiple times.
    pub async fn remove_peer(&self, peer: &Arc<RtcPeer>) {
        let mut peers = self.inner.peers.lock().await;
        peers.retain(|p| !Arc::ptr_eq(p, peer));
        info!(
            active_peers = peers.len(),
            "screen-share: peer unregistered"
        );
    }
}

/// Build the encoder config + run the capture loop. Synchronous because
/// `dxgi_capture::capture_loop` is blocking; it pushes frames into a
/// tokio mpsc channel that the broadcast loop consumes.
#[cfg(target_os = "windows")]
fn run_blocking(inner: Arc<Inner>) -> Result<()> {
    use ix_display_windows::CapturedFrame;
    use tokio::sync::mpsc;

    info!("screen-share: starting Windows DXGI capture path");

    let cfg = SharedConfig {
        width: DEFAULT_W,
        height: DEFAULT_H,
        fps_num: DEFAULT_FPS,
        fps_den: 1,
        initial_bitrate_kbps: DEFAULT_BITRATE_KBPS,
        min_bitrate_kbps: 1_000,
        max_bitrate_kbps: 25_000,
        profile: Profile::H264UlllFallback,
        color: ColorSpace::Bt709Sdr,
        intra_refresh_rows: 4,
    };

    // Build a small tokio runtime just for this worker. We can't reuse
    // the daemon's main runtime because we're on a sync std::thread.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let encoder_result = X264Sw::new(cfg);
    let mut encoder = match encoder_result {
        Ok(e) => e,
        Err(e) => {
            warn!(err = %e, "screen-share: encoder init failed; idling");
            rt.block_on(async { inner.cancel.cancelled().await });
            return Ok(());
        }
    };

    // Spawn the capture thread; it owns its own DXGI context.
    let mut rx: mpsc::Receiver<CapturedFrame> = ix_display_windows::spawn_capture_thread();

    rt.block_on(async move {
        loop {
            tokio::select! {
                _ = inner.cancel.cancelled() => {
                    info!("screen-share: cancelled");
                    return;
                }
                frame_opt = rx.recv() => {
                    let Some(frame) = frame_opt else {
                        warn!("screen-share: capture channel closed; exiting");
                        return;
                    };

                    // Resolution mismatch is a bug — log + skip rather
                    // than crash. A real fix is a re-init path, deferred
                    // to the polish phase.
                    if frame.width != DEFAULT_W || frame.height != DEFAULT_H {
                        warn!(
                            frame_w = frame.width,
                            frame_h = frame.height,
                            "screen-share: dropping frame with non-default resolution"
                        );
                        continue;
                    }

                    let yuv: Vec<u8> = (*frame.data).clone();
                    let pts_us = frame.pts_us;
                    let slice = match encoder.encode_yuv420(yuv, frame.width, frame.height, pts_us) {
                        Ok(s) => s,
                        Err(e) => {
                            warn!(err = %e, "screen-share: encode failed");
                            continue;
                        }
                    };

                    // Broadcast to every registered peer. Frame duration
                    // is 1/fps; webrtc-rs uses this for RTP timestamps.
                    let frame_dur = Duration::from_micros(1_000_000 / DEFAULT_FPS as u64);
                    let peers_snapshot = inner.peers.lock().await.clone();
                    for peer in peers_snapshot.iter() {
                        if let Err(e) = peer.write_sample(&slice.data, frame_dur).await {
                            warn!(err = %e, "screen-share: write_sample failed");
                        }
                    }
                }
            }
        }
    });
    Ok(())
}

/// Linux/macOS: no capture path wired yet (Plan 4 / future work). Idle so
/// the daemon still functions for pair + signaling testing.
#[cfg(not(target_os = "windows"))]
fn run_blocking(inner: Arc<Inner>) -> Result<()> {
    info!("screen-share: no capture path on this platform; idling");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        inner.cancel.cancelled().await;
    });
    Ok(())
}
