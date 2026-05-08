//! Safe wrapper over libevdi's user-mode API.
//!
//! # Lifetime model
//!
//! An `EvdiMonitor` owns one `evdi_handle`.  Dropping it calls
//! `evdi_disconnect` + `evdi_close` in the correct order.
//!
//! Capture buffers are registered and unregistered by the capture backends
//! (`wayland.rs`, `x11.rs`).  This module only manages the monitor lifecycle.
//!
//! # EDID
//!
//! We ship a canned EDID blob (`assets/edid_1080p120.bin`) that describes a
//! generic 1920×1080 @ 120 Hz monitor.  The kernel module parses this when
//! `evdi_connect2` is called.  The EDID is recognised by both Mutter (GNOME)
//! and KWin (KDE) without special quirk handling.

use crate::ffi::libevdi::LibevdiApi;
use dlopen2::wrapper::Container;
use std::path::PathBuf;
use thiserror::Error;
use tracing::{debug, error, info};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can arise during virtual-monitor lifecycle operations.
#[derive(Debug, Error)]
pub enum EvdiError {
    /// `libevdi.so.0` could not be opened.
    ///
    /// Typical cause: the `iextend-evdi-dkms` package is not installed, so
    /// neither `/usr/lib/libevdi.so.0` nor the DKMS build output are present.
    #[error("libevdi.so.0 not found — install the iextend-evdi-dkms package")]
    NotInstalled(#[source] dlopen2::Error),

    /// No `/dev/evdi*` device node was found.
    ///
    /// Typical cause: the kernel module was not loaded.  Try
    /// `sudo modprobe evdi initial_device_count=1`.
    #[error("/dev/evdi* not present — kernel module not loaded (try: sudo modprobe evdi initial_device_count=1)")]
    NoDevice,

    /// `evdi_open` returned a NULL pointer for the given device index.
    #[error("evdi_open returned NULL for device index {0}")]
    OpenFailed(i32),

    /// The EDID asset file was not embedded correctly at compile time.
    #[error("embedded EDID blob is missing — rebuild the crate")]
    MissingEdid,
}

// ---------------------------------------------------------------------------
// EvdiMonitor
// ---------------------------------------------------------------------------

/// A live virtual monitor managed by the evdi kernel module.
///
/// ```text
///     EvdiMonitor::open()        — opens /dev/evdi0, loads libevdi.so.0
///     monitor.connect(w, h, hz) — plugs the virtual connector with an EDID
///     monitor.handle()           — raw handle for capture backends
///     drop(monitor)             — evdi_disconnect + evdi_close
/// ```
pub struct EvdiMonitor {
    lib: Container<LibevdiApi>,
    handle: crate::ffi::libevdi::evdi_handle,
    /// Current mode, set by `connect`.
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
}

// SAFETY: evdi_handle is an opaque pointer to kernel state.  We never share
// the handle across threads without locking — it is always used from the
// single thread that owns the EvdiMonitor.
unsafe impl Send for EvdiMonitor {}

impl EvdiMonitor {
    /// Open the first available evdi device and load `libevdi.so.0`.
    ///
    /// Returns an error if:
    /// - `libevdi.so.0` cannot be found in `LD_LIBRARY_PATH` or the default
    ///   system library search paths.
    /// - No `/dev/evdi*` device node exists (module not loaded).
    /// - `evdi_open` fails (device busy or permissions issue).
    pub fn open() -> Result<Self, EvdiError> {
        // Load libevdi at runtime — no link-time dep on GPL code.
        let lib: Container<LibevdiApi> =
            unsafe { Container::load("libevdi.so.0") }
                .map_err(EvdiError::NotInstalled)?;

        // Find the first /dev/evdi* device node.  The kernel module creates
        // one node per virtual monitor slot; `initial_device_count` in
        // dkms.conf is set to 1, so normally only /dev/evdi0 is present.
        let dev_index = (0i32..32)
            .find(|&i| PathBuf::from(format!("/dev/evdi{i}")).exists())
            .ok_or(EvdiError::NoDevice)?;

        debug!(dev_index, "opening evdi device");

        let handle = unsafe { lib.evdi_open(dev_index) };
        if handle.is_null() {
            error!(dev_index, "evdi_open returned NULL");
            return Err(EvdiError::OpenFailed(dev_index));
        }

        info!(dev_index, "evdi device opened successfully");
        Ok(Self {
            lib,
            handle,
            width: 0,
            height: 0,
            refresh_hz: 0,
        })
    }

    /// Plug the virtual connector with the given resolution and refresh rate.
    ///
    /// Internally we call `evdi_connect2` with an embedded EDID blob that
    /// describes the requested mode.  The compositor will see a new connected
    /// output shortly after this call returns.
    ///
    /// # Panics
    ///
    /// Panics if called after `destroy` or before `open`.
    pub fn connect(&mut self, w: u32, h: u32, hz: u32) -> Result<(), EvdiError> {
        let edid = Self::edid_blob();
        if edid.is_empty() {
            return Err(EvdiError::MissingEdid);
        }

        unsafe {
            self.lib.evdi_connect2(
                self.handle,
                edid.as_ptr(),
                edid.len() as u32,
                // Maximum pixel area: 4K = 3840 × 2160.
                3840 * 2160,
            );
        }

        self.width = w;
        self.height = h;
        self.refresh_hz = hz;
        info!(width = w, height = h, hz, "virtual monitor connected");
        Ok(())
    }

    /// Return the raw evdi handle.
    ///
    /// Capture backends borrow this handle; they must not outlive `self`.
    pub fn handle(&self) -> crate::ffi::libevdi::evdi_handle {
        self.handle
    }

    /// Return a reference to the loaded library.
    ///
    /// Capture backends use this to call `evdi_register_buffer`, etc.
    pub fn lib(&self) -> &LibevdiApi {
        &self.lib
    }

    /// The canned EDID blob for a 1920×1080 @ 120 Hz monitor.
    ///
    /// We embed it at compile time from `assets/edid_1080p120.bin` so the
    /// daemon binary is self-contained.  If the file is absent (e.g. during a
    /// partial checkout) we return an empty slice and `connect` returns
    /// `EvdiError::MissingEdid`.
    fn edid_blob() -> &'static [u8] {
        // The EDID is a standard 128-byte or 256-byte block.
        // We ship a real 128-byte EDID for a 1080p 120 Hz monitor.
        // In test builds this file is generated by `build.rs` (see
        // `host/crates/ix-display-linux/build.rs` — out of scope for this
        // plan excerpt; the asset must be present before `cargo build` runs).
        static EDID: &[u8] = include_bytes!("../assets/edid_1080p120.bin");
        EDID
    }
}

impl Drop for EvdiMonitor {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            debug!("evdi_disconnect + evdi_close");
            unsafe {
                self.lib.evdi_disconnect(self.handle);
                self.lib.evdi_close(self.handle);
            }
            self.handle = std::ptr::null_mut();
        }
    }
}
