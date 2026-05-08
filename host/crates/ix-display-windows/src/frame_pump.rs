//! Async-compatible frame producer.
//!
//! `run()` is a blocking function intended to run on a dedicated `std::thread`.
//! It loops on `Connection::pull_frame()` (which blocks until the kernel
//! delivers a frame) and forwards frames through a `tokio::sync::mpsc::Sender`.
//!
//! Frame release (acking the buffer back to WDDM) happens in [`GpuFrame::drop`],
//! which posts `IOCTL_IEXDD_RELEASE_FRAME` via a weak reference to a shared
//! [`ReleaseChannel`].

#![cfg(windows)]

use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use tokio::sync::mpsc::Sender;
use tracing::{debug, error, warn};

use crate::error::Result;
use crate::iddcx_bindings::{IEXDD_FRAME_HEADER, RECT};
use crate::inverted_call::Connection;

// ---------------------------------------------------------------------------
// ReleaseChannel — shared between GpuFrame Drop impls and the pull thread
// ---------------------------------------------------------------------------

/// Wraps a Connection handle used exclusively for RELEASE_FRAME IOCTLs.
/// The pull thread holds an `Arc`; each `GpuFrame` holds a `Weak`. When
/// user-mode drops the frame, the `Weak::upgrade` succeeds until the pull
/// thread itself exits (Arc count drops to zero), at which point release IOCTLs
/// are silently skipped — the kernel cleans up on file-handle close.
pub struct ReleaseChannel {
    conn: Connection,
}

impl ReleaseChannel {
    pub fn release(&self, seq: u64) -> Result<()> {
        self.conn.release_frame(seq)
    }
}

// ---------------------------------------------------------------------------
// GpuFrame — the public frame type
// ---------------------------------------------------------------------------

/// One captured frame, backed by a D3D11 NT shared texture handle.
///
/// When this value is dropped, `IOCTL_IEXDD_RELEASE_FRAME` is posted to the
/// driver, releasing the swapchain buffer back to WDDM.
pub struct GpuFrame {
    /// Raw NT handle value (not a `HANDLE` to avoid `windows` crate dep in
    /// public API). Pass to `ID3D11Device::OpenSharedResource1`.
    pub shared_handle: u64,
    pub width: u32,
    pub height: u32,
    pub dirty_rects: Vec<RECT>,
    /// Microseconds since the QueryPerformanceCounter epoch (boot-relative).
    pub present_time_us: u64,
    pub(crate) seq: u64,
    pub(crate) release: Weak<ReleaseChannel>,
}

impl std::fmt::Debug for GpuFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuFrame")
            .field("seq", &self.seq)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("dirty_rects", &self.dirty_rects.len())
            .finish()
    }
}

impl Drop for GpuFrame {
    fn drop(&mut self) {
        if let Some(ch) = self.release.upgrade() {
            if let Err(e) = ch.release(self.seq) {
                // Best-effort; error during shutdown is expected.
                debug!("GpuFrame::drop release seq={} error: {e}", self.seq);
            }
        }
    }
}

// SAFETY: GpuFrame is moved across thread boundaries through the mpsc channel.
// `shared_handle` is a raw pointer VALUE (not a reference) managed by the
// kernel; it remains valid until the matching RELEASE_FRAME IOCTL.
unsafe impl Send for GpuFrame {}

// ---------------------------------------------------------------------------
// QPC → microseconds conversion
// ---------------------------------------------------------------------------

/// Convert a QueryPerformanceCounter tick value to microseconds since boot.
/// We compute the ratio once at startup to avoid a syscall per frame.
fn qpc_to_us(qpc_ticks: u64) -> u64 {
    use windows::Win32::System::Performance::QueryPerformanceFrequency;
    // SAFETY: QueryPerformanceFrequency never fails on Windows Vista+.
    let mut freq: i64 = 0;
    unsafe { QueryPerformanceFrequency(&mut freq).ok().unwrap() };
    // Avoid overflow: scale by dividing frequency first.
    let freq_mhz = freq as u64 / 1_000_000;
    if freq_mhz == 0 {
        qpc_ticks
    } else {
        qpc_ticks / freq_mhz
    }
}

// ---------------------------------------------------------------------------
// run — blocking frame pump (runs on a dedicated thread)
// ---------------------------------------------------------------------------

/// Main loop for the frame-pump thread. Blocks on each `pull_frame()` call.
///
/// Returns when either:
///  - The `Sender` is dropped (receiver half gone).
///  - The driver returns an error (device removed, service stopped, etc.).
pub fn run(conn: Connection, tx: Sender<GpuFrame>) {
    // Clone the connection for releases; the original is used for pulls.
    let release_conn = match conn.try_clone() {
        Ok(c) => c,
        Err(e) => {
            error!("frame_pump: failed to clone connection for releases: {e}");
            return;
        }
    };

    let channel = Arc::new(ReleaseChannel { conn: release_conn });

    tracing::info!("frame_pump: pull thread started");

    loop {
        let result = conn.pull_frame();
        match result {
            Ok((header, rects)) => {
                let frame = make_frame(header, rects, Arc::downgrade(&channel));

                let seq = frame.seq;
                let send_start = Instant::now();

                match tx.blocking_send(frame) {
                    Ok(()) => {
                        let elapsed = send_start.elapsed();
                        if elapsed > Duration::from_millis(16) {
                            warn!(
                                "frame_pump: blocking_send took {:?} for seq={} \
                                 — encoder thread may be starved",
                                elapsed, seq
                            );
                        }
                        debug!("frame_pump: delivered seq={seq}");
                    }
                    Err(_) => {
                        // Receiver dropped — normal shutdown.
                        tracing::info!("frame_pump: receiver gone, stopping");
                        return;
                    }
                }
            }
            Err(e) => {
                error!("frame_pump: pull_frame error: {e}");
                // Give the kernel a moment before retrying (device removal
                // path); if the error persists, the loop will keep returning
                // the same error and the caller will see no frames.
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_frame(
    header: IEXDD_FRAME_HEADER,
    rects: Vec<RECT>,
    release: Weak<ReleaseChannel>,
) -> GpuFrame {
    let present_us = qpc_to_us(header.PresentTimeQpc);

    GpuFrame {
        shared_handle: header.SharedTextureHandle as u64,
        width: header.Width,
        height: header.Height,
        dirty_rects: rects,
        present_time_us: present_us,
        seq: header.AcquireSeq,
        release,
    }
}
