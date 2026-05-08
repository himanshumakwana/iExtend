//! Cross-platform display capture types.
//!
//! Both `ix-display-windows` (IddCx + DXGI) and `ix-display-linux` (evdi +
//! PipeWire/X11) implement [`DisplaySource`] and emit [`GpuFrame`]s into a
//! shared queue. Codec impls in `ix-codec` (Plan 5) ingest those frames.
//!
//! This crate intentionally has zero OS-specific dependencies so it can be
//! pulled into either backend without licence or ABI surprises.

use thiserror::Error;

/// A rectangle of pixels that changed since the last frame.
///
/// Capture backends report damage so encoders can do partial-frame intra-refresh
/// (slice-level encode of only the dirty region) — see spec §3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DamageRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl DamageRect {
    pub fn full(w: u32, h: u32) -> Self {
        Self { x: 0, y: 0, w, h }
    }

    /// True if `self` and `other` overlap or touch (axis-aligned).
    pub fn overlaps(&self, other: &Self) -> bool {
        let (a_r, a_b) = (self.x + self.w, self.y + self.h);
        let (b_r, b_b) = (other.x + other.w, other.y + other.h);
        self.x < b_r && other.x < a_r && self.y < b_b && other.y < a_b
    }

    /// Smallest rect containing both.
    pub fn union(&self, other: &Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let r = (self.x + self.w).max(other.x + other.w);
        let b = (self.y + self.h).max(other.y + other.h);
        Self { x, y, w: r - x, h: b - y }
    }
}

/// Where a captured frame's bytes live.
#[derive(Debug, Clone, Copy)]
pub enum GpuFrameKind {
    /// Linux/Wayland: raw DMA-BUF fd. Owned by the capture-side; encoder
    /// borrows it for the lifetime of one encode call.
    DmaBuf {
        fd: i32,
        stride: u32,
        modifier: u64,
        format_fourcc: u32,
    },
    /// Linux/X11: shared-memory CPU buffer. `addr` is the mmap'd start.
    /// Encoder may need to upload to GPU for hw encode.
    ShmCpu {
        addr: *mut u8,
        stride: usize,
    },
    /// Windows IddCx: D3D11 texture handle (NT shared handle).
    /// Encoder imports via `OpenSharedResource1`. Truly zero-copy.
    D3D11Shared {
        nt_handle: u64,
    },
    /// Windows fallback / Linux NVIDIA proprietary: CUDA device pointer.
    /// `cuMemcpy2D` from this into the encoder's input surface.
    CudaDevicePtr {
        ptr: u64,
        pitch: usize,
    },
}

// SAFETY: GpuFrame is moved between threads via SPSC ring buffer. Callers must
// ensure pointers remain valid until the consumer is done with them. The
// lifetime contract is "valid until the next capture_frame() call on the same
// source," documented on the consumer-side trait.
unsafe impl Send for GpuFrameKind {}

/// One captured frame. Capture backends produce these; encoders consume them.
#[derive(Debug, Clone)]
pub struct GpuFrame {
    pub kind: GpuFrameKind,
    pub width: u32,
    pub height: u32,
    pub damage: Vec<DamageRect>,
    /// Microseconds since some monotonic origin. Used by the latency
    /// regression test in Plan 10.
    pub timestamp_us: u64,
}

/// Display mode requested when creating a virtual monitor.
#[derive(Debug, Clone, Copy)]
pub struct DisplayMode {
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
    /// HDR10 (BT.2020 PQ) when true; SDR sRGB otherwise.
    pub hdr: bool,
}

impl Default for DisplayMode {
    fn default() -> Self {
        Self { width: 1920, height: 1080, refresh_hz: 120, hdr: false }
    }
}

/// Opaque handle returned by `create_virtual_monitor`. Backends know how to
/// destroy it on drop.
#[derive(Debug, Clone, Copy)]
pub struct MonitorHandle(pub u64);

/// Errors that any backend can surface to the daemon.
#[derive(Debug, Error)]
pub enum DisplayError {
    #[error("backend not available on this platform: {0}")]
    NotAvailable(&'static str),
    #[error("driver not installed: {0}")]
    DriverMissing(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("backend-specific: {0}")]
    Backend(String),
}

/// Trait both `ix-display-windows` and `ix-display-linux` implement. The
/// daemon (`iextendd`) holds a `Box<dyn DisplaySource>` chosen at startup
/// based on the running OS.
pub trait DisplaySource: Send {
    /// Plug a virtual monitor of the given mode. Returns its handle.
    fn create_virtual_monitor(&mut self, mode: DisplayMode) -> Result<MonitorHandle, DisplayError>;

    /// Pop the next captured frame, if one is ready. Non-blocking.
    /// `None` means "no frame yet — try again next tick."
    fn capture_frame(&mut self) -> Option<GpuFrame>;

    /// Tear down the virtual monitor and release any kernel/userspace
    /// resources. Backends also do this in their Drop impl, but explicit
    /// destroy is preferred for clean shutdown.
    fn destroy(&mut self);
}
