//! Linux display backend for iExtend.
//!
//! This crate creates a virtual monitor via the `evdi` kernel module (loaded at
//! runtime via dlopen so there is no link-time dependency on GPL code), then
//! captures frames from it using either:
//!
//! - **Wayland path** — `xdg-desktop-portal` ScreenCast portal + PipeWire
//!   DMA-BUF delivery (zero-copy into VAAPI / AMF encoders).
//! - **X11 path** — MIT-SHM shared-memory XImage + XDamage dirty rectangles
//!   (one CPU→GPU upload for NVENC; direct shm ingest for VAAPI).
//!
//! NVIDIA proprietary driver caveat: cannot expose DMA-BUF for NVENC ingest.
//! Detected at start-up; the crate switches to the CUDA interop fallback
//! (`cuMemcpy2D` from the screencast buffer into an NVENC input surface,
//! ~0.3 ms overhead). Stubs live in `nvidia_cuda.rs`; the real plumbing lands
//! in Plan 5 (codec selection).
//!
//! # `#[cfg(unix)]` gate
//!
//! The Plan 2 skeleton used `#![cfg(unix)]` as a crate-level inner attribute.
//! That form hides ALL items from non-Unix hosts, which means `ix-display-linux`
//! exports nothing on Windows and `cargo check` on a cross-compile target for
//! Windows would produce "no items exported" errors in dependents. We relax it
//! to **per-item `#[cfg(unix)]`** attributes so that:
//!
//! 1. The crate root is always visible (empty on non-Unix but still valid).
//! 2. Every public item is individually gated.
//! 3. Tests that only probe environment variables (`compositor_probe`,
//!    `secureboot_probe`) remain runnable on any host where the CI machine
//!    might cross-check.

#[cfg(unix)]
pub mod detect;
#[cfg(unix)]
pub mod evdi;
#[cfg(unix)]
mod ffi;
#[cfg(unix)]
pub mod nvidia_cuda;
#[cfg(unix)]
pub mod secureboot;
#[cfg(unix)]
pub mod wayland;
#[cfg(unix)]
pub mod x11;

// Re-export the primary types callers need.
#[cfg(unix)]
pub use detect::{detect_backend, Backend, EnvProbe, StdEnv};

#[cfg(unix)]
use crossbeam::queue::ArrayQueue;
#[cfg(unix)]
use ix_display::{DisplayError, DisplayMode, DisplaySource, GpuFrame, MonitorHandle};
#[cfg(unix)]
use std::sync::Arc;
#[cfg(unix)]
use tracing::info;

// ---------------------------------------------------------------------------
// LinuxDisplaySource — the concrete type that iextendd holds as
// `Box<dyn DisplaySource>`.
// ---------------------------------------------------------------------------

/// Concrete Linux display-capture backend.
///
/// On construction:
/// 1. Opens an evdi virtual-monitor device via `EvdiMonitor::open()`.
/// 2. Selects the Wayland or X11 capture path via `detect_backend`.
/// 3. Logs whether SecureBoot is active (just informational for v1).
///
/// Callers must call `create_virtual_monitor` before `capture_frame`.
#[cfg(unix)]
pub struct LinuxDisplaySource {
    monitor: Option<evdi::EvdiMonitor>,
    backend: LinuxBackend,
    out: Arc<ArrayQueue<GpuFrame>>,
}

#[cfg(unix)]
#[allow(dead_code)]
enum LinuxBackend {
    Wayland(wayland::WaylandCapture),
    X11(x11::X11Capture),
}

#[cfg(unix)]
impl LinuxDisplaySource {
    /// Build and return the backend appropriate for the running session.
    ///
    /// Returns an error if neither `WAYLAND_DISPLAY` nor `DISPLAY` is set,
    /// or if `libevdi.so.0` cannot be found.
    pub fn new() -> Result<Self, LinuxError> {
        let out: Arc<ArrayQueue<GpuFrame>> = Arc::new(ArrayQueue::new(16));

        let env_backend = detect_backend(&StdEnv);
        if env_backend == Backend::None {
            return Err(LinuxError::NoCompositor);
        }

        if secureboot::is_secureboot_enabled(&secureboot::StdSecureBootProbe) {
            info!(
                "SecureBoot is active — verify that the evdi MOK signing cert \
                 was enrolled before the first boot (see docs/install-linux.md)"
            );
        }

        if nvidia_cuda::proprietary_driver_active() {
            info!(
                "NVIDIA proprietary driver detected — DMA-BUF unavailable for \
                 NVENC; will use CUDA-interop fallback (Plan 5 wires this up)"
            );
        }

        let backend = match env_backend {
            Backend::Wayland => LinuxBackend::Wayland(wayland::WaylandCapture::new(out.clone())),
            Backend::X11 => LinuxBackend::X11(x11::X11Capture::new(out.clone())),
            Backend::None => unreachable!(),
        };

        Ok(Self {
            monitor: None,
            backend,
            out,
        })
    }
}

#[cfg(unix)]
impl DisplaySource for LinuxDisplaySource {
    fn create_virtual_monitor(&mut self, mode: DisplayMode) -> Result<MonitorHandle, DisplayError> {
        let mut mon =
            evdi::EvdiMonitor::open().map_err(|e| DisplayError::DriverMissing(e.to_string()))?;
        mon.connect(mode.width, mode.height, mode.refresh_hz)
            .map_err(|e| DisplayError::Backend(e.to_string()))?;
        let handle = MonitorHandle(0); // evdi handle is opaque; we own it via mon
        self.monitor = Some(mon);
        info!(
            width = mode.width,
            height = mode.height,
            hz = mode.refresh_hz,
            "virtual monitor created"
        );
        Ok(handle)
    }

    fn capture_frame(&mut self) -> Option<GpuFrame> {
        // Pump one frame from the X11 path synchronously.  The Wayland path
        // pushes frames asynchronously from a background task (started by
        // iextendd once Plan 5 is in place).
        if let LinuxBackend::X11(ref mut cap) = self.backend {
            let _ = cap.pump();
        }
        self.out.pop()
    }

    fn destroy(&mut self) {
        // Drop the monitor — EvdiMonitor::drop calls evdi_disconnect +
        // evdi_close.
        self.monitor = None;
    }
}

// ---------------------------------------------------------------------------
// LinuxError
// ---------------------------------------------------------------------------

/// Top-level error type for the Linux display backend.
#[cfg(unix)]
#[derive(Debug, thiserror::Error)]
pub enum LinuxError {
    #[error(transparent)]
    Evdi(#[from] evdi::EvdiError),
    #[error(transparent)]
    X11(#[from] x11::X11Error),
    #[error("no graphical session detected (neither WAYLAND_DISPLAY nor DISPLAY is set)")]
    NoCompositor,
}
