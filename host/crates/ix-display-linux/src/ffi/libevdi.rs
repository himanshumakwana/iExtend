//! libevdi 1.14 C ABI.
//!
//! Source of truth: `evdi/library/evdi_lib.h` from the upstream evdi
//! repository (https://github.com/DisplayLink/evdi).  Only the symbols we
//! actually call are declared here; adding more is cheap.
//!
//! At build time this file has *zero* link-time impact — we load the library
//! at runtime via `dlopen2::wrapper::Container::load`.

#![allow(non_camel_case_types, non_snake_case, dead_code)]

use dlopen2::wrapper::WrapperApi;
use std::os::raw::{c_int, c_uint, c_void};

/// Opaque handle to an open evdi device node.
pub type evdi_handle = *mut c_void;

// ---------------------------------------------------------------------------
// Mode descriptor — returned by the compositor-side DRM master when the
// virtual connector's EDID is accepted.
// ---------------------------------------------------------------------------

/// Describes the current display mode of the virtual monitor.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct evdi_mode {
    /// Horizontal resolution in pixels.
    pub width: c_int,
    /// Vertical resolution in pixels.
    pub height: c_int,
    /// Refresh rate in Hz.
    pub refresh_rate: c_int,
    /// Bits per pixel (always 32 for the modes we request).
    pub bits_per_pixel: c_int,
    /// DRM pixel format fourcc (e.g. `DRM_FORMAT_XRGB8888` = 0x34325258).
    pub pixel_format: c_uint,
}

// ---------------------------------------------------------------------------
// Capture buffer — registered by the caller, filled by the kernel module.
// ---------------------------------------------------------------------------

/// A CPU-visible capture buffer registered with the evdi kernel module.
///
/// The `buffer` pointer must remain valid for the lifetime of the registration
/// (between `evdi_register_buffer` and `evdi_unregister_buffer`).
#[repr(C)]
pub struct evdi_buffer {
    /// Caller-chosen identifier; used to correlate `update_ready` callbacks.
    pub id: c_int,
    /// Pointer to the mapped pixel data.
    pub buffer: *mut c_void,
    /// Buffer width in pixels.
    pub width: c_int,
    /// Buffer height in pixels.
    pub height: c_int,
    /// Row stride in bytes.
    pub stride: c_int,
    /// Array of `evdi_rect` structs describing dirty regions (may be NULL).
    pub rects: *mut c_void,
    /// Number of valid entries in `rects`.
    pub rect_count: c_int,
}

// ---------------------------------------------------------------------------
// Event callbacks
// ---------------------------------------------------------------------------

/// Called when the virtual monitor's mode changes (resolution / refresh).
pub type evdi_mode_changed_cb =
    unsafe extern "C" fn(mode: evdi_mode, user_data: *mut c_void);

/// Called when a new frame is ready in the registered buffer.
pub type evdi_update_ready_cb =
    unsafe extern "C" fn(buffer_id: c_int, user_data: *mut c_void);

/// Called when the DPMS state changes (on/off/standby).
pub type evdi_dpms_cb =
    unsafe extern "C" fn(dpms_mode: c_int, user_data: *mut c_void);

/// Called when the CRTC state changes.
pub type evdi_crtc_state_cb =
    unsafe extern "C" fn(state: c_int, user_data: *mut c_void);

// ---------------------------------------------------------------------------
// Event-dispatch context
// ---------------------------------------------------------------------------

/// Passed to `evdi_handle_events`; the kernel module calls the appropriate
/// callback for each pending event.
///
/// Unused callback slots should be set to a no-op function (not NULL) to
/// avoid NULL pointer dereferences in older libevdi versions.
#[repr(C)]
pub struct evdi_event_context {
    pub dpms_handler: evdi_dpms_cb,
    pub mode_changed_handler: evdi_mode_changed_cb,
    pub update_ready_handler: evdi_update_ready_cb,
    pub crtc_state_handler: evdi_crtc_state_cb,
    /// Forwarded as the last argument to every callback.
    pub user_data: *mut c_void,
}

// ---------------------------------------------------------------------------
// WrapperApi — populated by Container::load("libevdi.so.0")
// ---------------------------------------------------------------------------

/// Dynamically-loaded libevdi API surface.
///
/// Instantiate with:
/// ```rust,ignore
/// let lib: Container<LibevdiApi> =
///     unsafe { Container::load("libevdi.so.0") }?;
/// ```
#[derive(WrapperApi)]
pub struct LibevdiApi {
    /// Open the evdi device at the given index (0 = `/dev/evdi0`).
    evdi_open: unsafe extern "C" fn(device_index: c_int) -> evdi_handle,

    /// Close a previously opened handle (does NOT disconnect first).
    evdi_close: unsafe extern "C" fn(handle: evdi_handle),

    /// Connect a virtual monitor with the given EDID blob.
    ///
    /// `sku_area_limit` caps the maximum pixel area (width * height) of modes
    /// that the virtual monitor advertises; 3840*2160 covers 4K.
    evdi_connect2: unsafe extern "C" fn(
        handle: evdi_handle,
        edid: *const u8,
        edid_length: c_uint,
        sku_area_limit: u32,
    ),

    /// Signal to the kernel module that the monitor has been unplugged.
    evdi_disconnect: unsafe extern "C" fn(handle: evdi_handle),

    /// Register a CPU-visible capture buffer with the module.
    evdi_register_buffer:
        unsafe extern "C" fn(handle: evdi_handle, buffer: evdi_buffer),

    /// Unregister and release a previously registered capture buffer.
    evdi_unregister_buffer:
        unsafe extern "C" fn(handle: evdi_handle, buffer_id: c_int),

    /// Ask the module to fill the given buffer on the next vsync.
    /// Returns `true` if an update was already pending.
    evdi_request_update:
        unsafe extern "C" fn(handle: evdi_handle, buffer_id: c_int) -> bool,

    /// Drain pending events from the kernel module into the event context.
    evdi_handle_events:
        unsafe extern "C" fn(handle: evdi_handle, ctx: *mut evdi_event_context),

    /// Return a pollable file descriptor that becomes readable when events
    /// are pending.  Callers can select/poll/epoll this fd.
    evdi_get_event_ready:
        unsafe extern "C" fn(handle: evdi_handle) -> c_int,
}
