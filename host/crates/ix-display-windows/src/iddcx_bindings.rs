//! Hand-written FFI mirror of `Public.h` for use on the user-mode side.
//!
//! On a real Windows build, `build.rs` would invoke bindgen against the actual
//! `Public.h` from the WDK include path to produce these exact types. We
//! hand-write them here so the crate compiles on Linux (where bindgen can't
//! include WDK headers) and to serve as the canonical authoritative source for
//! code review.
//!
//! # Layout contract
//! All types use `#[repr(C)]` with explicit field widths matching the C
//! counterparts in `Public.h`. Any change here **must** bump
//! `IEXDD_PROTOCOL_VERSION` in both this file and `Public.h`.

#![allow(non_camel_case_types, dead_code)]

use std::ffi::c_void;

// ---------------------------------------------------------------------------
// Protocol constants
// ---------------------------------------------------------------------------

pub const IEXDD_PROTOCOL_VERSION: u32 = 1;
pub const IEXDD_DEVICE_TYPE: u32 = 0x9D00;
pub const IEXDD_MAX_DIRTY_RECTS: usize = 16;
pub const IEXDD_MAX_INFLIGHT_FRAMES: usize = 4;

// Device path opened by user-mode via CreateFile.
pub const IEXDD_DEVICE_PATH: &str = r"\\.\IExtendDisplay";

// ---------------------------------------------------------------------------
// IOCTL codes (METHOD_BUFFERED, FILE_ANY_ACCESS)
// Computed via CTL_CODE(type, func, method, access):
//   (type << 16) | (access << 14) | (func << 2) | method
// type=0x9D00, access=0x0000 (FILE_ANY_ACCESS), method=0 (METHOD_BUFFERED)
// ---------------------------------------------------------------------------

const fn ctl_code(device_type: u32, function: u32) -> u32 {
    // METHOD_BUFFERED = 0, FILE_ANY_ACCESS = 0
    (device_type << 16) | (function << 2)
}

pub const IOCTL_IEXDD_HELLO: u32 = ctl_code(IEXDD_DEVICE_TYPE, 0x800);
pub const IOCTL_IEXDD_PULL_FRAME: u32 = ctl_code(IEXDD_DEVICE_TYPE, 0x801);
pub const IOCTL_IEXDD_RELEASE_FRAME: u32 = ctl_code(IEXDD_DEVICE_TYPE, 0x802);
pub const IOCTL_IEXDD_QUERY_STATS: u32 = ctl_code(IEXDD_DEVICE_TYPE, 0x803);

// ---------------------------------------------------------------------------
// Structures — must match Public.h exactly
// ---------------------------------------------------------------------------

/// GUID type (same layout as Windows GUID).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub struct GUID {
    pub Data1: u32,
    pub Data2: u16,
    pub Data3: u16,
    pub Data4: [u8; 8],
}

/// Input/output for IOCTL_IEXDD_HELLO.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct IEXDD_HELLO {
    pub ProtocolVersion: u32,
    pub ClientPid: u32,
    pub ClientNonce: GUID,
}

/// Fixed-size rect matching the Windows RECT layout.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct RECT {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// Output header for IOCTL_IEXDD_PULL_FRAME.
/// Followed in the I/O buffer by `DirtyRectCount * RECT`.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct IEXDD_FRAME_HEADER {
    /// QueryPerformanceCounter value at vsync acquire.
    pub PresentTimeQpc: u64,
    /// Monotonic per-monitor acquire counter; gaps indicate dropped frames.
    pub AcquireSeq: u64,
    /// NT shared handle duplicated into this process. Import via
    /// `ID3D11Device::OpenSharedResource1`.
    pub SharedTextureHandle: *mut c_void,
    pub Width: u32,
    pub Height: u32,
    /// 0 means full frame (> IEXDD_MAX_DIRTY_RECTS or genuinely full refresh).
    pub DirtyRectCount: u32,
    /// DXGI_COLOR_SPACE_TYPE; 0 = RGB_FULL_G22_NONE_P709 (sRGB).
    pub ColorSpaceId: u32,
}

// SAFETY: IEXDD_FRAME_HEADER contains a raw pointer used only for handle
// value transport (not dereferenced by Rust). The kernel fills it; user-mode
// passes it to OpenSharedResource1 which takes ownership.
unsafe impl Send for IEXDD_FRAME_HEADER {}

/// Input for IOCTL_IEXDD_RELEASE_FRAME.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct IEXDD_FRAME_RELEASE {
    pub AcquireSeq: u64,
}

/// Output for IOCTL_IEXDD_QUERY_STATS.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct IEXDD_STATS {
    pub FramesAcquired: u64,
    pub FramesDelivered: u64,
    pub FramesDropped: u64,
    pub PullRequestsTotal: u64,
    pub ReleaseRequestsTotal: u64,
    pub InFlightCount: u32,
    pub ProtocolVersion: u32,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

impl IEXDD_FRAME_HEADER {
    /// Returns true if the frame represents a full-screen refresh.
    #[inline]
    pub fn is_full_frame(&self) -> bool {
        self.DirtyRectCount == 0
    }

    /// Returns the expected total byte size of the IOCTL output buffer,
    /// including the trailing RECT array.
    #[inline]
    pub fn output_size(&self) -> usize {
        std::mem::size_of::<IEXDD_FRAME_HEADER>()
            + self.DirtyRectCount as usize * std::mem::size_of::<RECT>()
    }
}

// ---------------------------------------------------------------------------
// Size assertions — catch layout drift between Rust and C at compile time.
// ---------------------------------------------------------------------------

const _: () = {
    assert!(std::mem::size_of::<GUID>() == 16);
    assert!(std::mem::size_of::<IEXDD_HELLO>() == 24); // 4+4+16
    assert!(std::mem::size_of::<RECT>() == 16); // 4×i32
    assert!(std::mem::size_of::<IEXDD_FRAME_RELEASE>() == 8);
    assert!(std::mem::size_of::<IEXDD_STATS>() == 48); // 5×u64 + 2×u32
                                                       // IEXDD_FRAME_HEADER size is pointer-size-dependent; skip static assert.
};
