//! Windows IddCx virtual display + DXGI capture backend.
//!
//! On non-Windows targets this entire crate compiles to an empty module;
//! consumers gate all use behind `#[cfg(windows)]`. The cargo workspace still
//! builds on Linux via `cargo check -p ix-display-windows`.
//!
//! # Architecture
//!
//! ```text
//! iexdd.sys (kernel)
//!   └── IOCTL_IEXDD_PULL_FRAME (inverted call — blocks until frame ready)
//!         │
//!   ix-display-windows (user mode)
//!   ├── WindowsDisplaySource  (impl DisplaySource)
//!   ├── inverted_call.rs      (DeviceIoControl wrapper, HELLO handshake)
//!   ├── frame_pump.rs         (tokio::sync::mpsc frame producer)
//!   └── iddcx_bindings.rs     (hand-written FFI mirror of Public.h)
//! ```
//!
//! Zero-copy frame path: the kernel duplicates a D3D11 shared texture handle
//! directly into this process; `OpenSharedResource1` imports it without any
//! pixel-data copy.

#![cfg(windows)]
// Scaffolding-stage allows. Promote to deny when the IddCx pipeline ships
// end-to-end and every public surface is wired up.
#![allow(
    dead_code,
    unused_imports,
    clippy::arc_with_non_send_sync,
    clippy::type_complexity
)]

mod dxgi_capture;
mod error;
mod frame_pump;
mod iddcx_bindings;
mod inverted_call;

pub use dxgi_capture::{spawn_capture_thread, CaptureError, CapturedFrame};
pub use error::{Error, Result};
pub use frame_pump::GpuFrame;

use ix_display::{
    DisplayError, DisplayMode, DisplaySource, GpuFrame as SharedGpuFrame, GpuFrameKind,
    MonitorHandle,
};
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// WindowsDisplaySource — public entry point
// ---------------------------------------------------------------------------

/// IddCx virtual display backend. Implements [`DisplaySource`] from
/// `ix-display` so the daemon holds a platform-erased `Box<dyn DisplaySource>`.
///
/// # Example
/// ```no_run
/// # #[cfg(windows)]
/// # {
/// use ix_display_windows::WindowsDisplaySource;
/// use ix_display::{DisplaySource, DisplayMode};
///
/// let mut src = WindowsDisplaySource::new().expect("driver installed?");
/// let _handle = src.create_virtual_monitor(DisplayMode::default()).unwrap();
/// loop {
///     if let Some(frame) = src.capture_frame() {
///         // hand frame to encoder
///         drop(frame); // releases back to kernel
///     }
/// }
/// # }
/// ```
pub struct WindowsDisplaySource {
    conn: inverted_call::Connection,
    monitor: Option<MonitorHandle>,
    pump_rx: Option<tokio::sync::mpsc::Receiver<GpuFrame>>,
}

impl WindowsDisplaySource {
    /// Open the `\\.\IExtendDisplay` device and complete the HELLO handshake.
    ///
    /// Fails with [`Error::DriverNotInstalled`] if `iexdd.sys` is not loaded.
    pub fn new() -> Result<Self> {
        let conn = inverted_call::Connection::open()?;
        Ok(Self {
            conn,
            monitor: None,
            pump_rx: None,
        })
    }
}

impl DisplaySource for WindowsDisplaySource {
    fn create_virtual_monitor(
        &mut self,
        mode: DisplayMode,
    ) -> std::result::Result<MonitorHandle, DisplayError> {
        // The kernel driver advertises its fixed mode list; `mode` is advisory.
        // Log it so we can surface a warning if the requested mode is absent.
        info!(
            "create_virtual_monitor: {}x{}@{}Hz hdr={}",
            mode.width, mode.height, mode.refresh_hz, mode.hdr
        );

        // Start the frame pump on a dedicated background thread.
        let (tx, rx) =
            tokio::sync::mpsc::channel::<GpuFrame>(iddcx_bindings::IEXDD_MAX_INFLIGHT_FRAMES);

        let conn_clone = self
            .conn
            .try_clone()
            .map_err(|e| DisplayError::Backend(format!("failed to clone device handle: {e}")))?;

        std::thread::Builder::new()
            .name("iextend-pull".into())
            .spawn(move || frame_pump::run(conn_clone, tx))
            .map_err(|e| DisplayError::Backend(format!("spawn pull thread: {e}")))?;

        self.pump_rx = Some(rx);
        self.monitor = Some(MonitorHandle(0)); // v1: single virtual monitor, id=0

        Ok(MonitorHandle(0))
    }

    fn capture_frame(&mut self) -> Option<SharedGpuFrame> {
        let rx = self.pump_rx.as_mut()?;

        // Non-blocking try_recv: return None if no frame is ready yet.
        match rx.try_recv() {
            Ok(win_frame) => {
                // Convert our Windows-specific GpuFrame to the shared GpuFrame type
                // from ix-display. The shared handle (NT handle) is the zero-copy path.
                let kind = GpuFrameKind::D3D11Shared {
                    nt_handle: win_frame.shared_handle,
                };

                let damage = win_frame
                    .dirty_rects
                    .iter()
                    .map(|r| ix_display::DamageRect {
                        x: r.left as u32,
                        y: r.top as u32,
                        w: (r.right - r.left) as u32,
                        h: (r.bottom - r.top) as u32,
                    })
                    .collect();

                Some(SharedGpuFrame {
                    kind,
                    width: win_frame.width,
                    height: win_frame.height,
                    damage,
                    timestamp_us: win_frame.present_time_us,
                })
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => None,
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                warn!("capture_frame: pull thread disconnected");
                None
            }
        }
    }

    fn destroy(&mut self) {
        // Drop the receiver — this signals the pull thread to stop on next send.
        self.pump_rx = None;
        self.monitor = None;
        info!("WindowsDisplaySource destroyed");
    }
}

impl Drop for WindowsDisplaySource {
    fn drop(&mut self) {
        self.destroy();
    }
}

// ---------------------------------------------------------------------------
// Re-exports for downstream crates that need the raw frame type
// ---------------------------------------------------------------------------

/// Re-export the Windows-specific GPU frame (wraps a D3D11 shared handle).
/// Downstream `ix-codec` should use the platform-erased `ix_display::GpuFrame`
/// instead; this is exposed for testing and diagnostics only.
pub use frame_pump::GpuFrame as WindowsGpuFrame;
