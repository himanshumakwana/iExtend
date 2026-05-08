//! Linux vsync via DRM vblank interrupt (`DRM_IOCTL_WAIT_VBLANK`).
//!
//! # Implementation
//!
//! 1. Open `/dev/dri/card0` (or the path set in `$DRM_DEVICE`).
//! 2. Spawn a background thread that calls `drmWaitVBlank` with
//!    `DRM_VBLANK_RELATIVE | sequence=1` in a loop.
//! 3. On each return, set the shared `AtomicBool` to `true` so the `winit`
//!    main loop can call `window.request_redraw()`.
//!
//! On Wayland composited desktops the vblank is gatekept by the compositor;
//! `softbuffer` + `winit` will honour the compositor's `frame` callback
//! automatically, so the DRM thread is effectively a fallback for X11 / bare
//! KMS sessions.  It is safe to run both — the DRM thread will fire at the
//! hardware refresh rate, and the compositor's frame callback will suppress
//! over-painting.
//!
//! # Fallback
//!
//! If `/dev/dri/card0` can't be opened (permission denied, no DRM device),
//! this function returns `Ok(())` and the main loop runs at `ControlFlow::Poll`
//! speed (software-limited to ~thousands of Hz, well above 240 fps).

use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[cfg(target_os = "linux")]
use std::os::unix::io::AsRawFd;

/// Install the DRM vblank handler thread.
///
/// Returns immediately after spawning the thread.  If DRM is unavailable,
/// logs a warning and returns `Ok(())`.
pub fn install_drm_vblank_handler(flag: Arc<AtomicBool>) -> Result<()> {
    let drm_device = std::env::var("DRM_DEVICE").unwrap_or_else(|_| "/dev/dri/card0".into());

    let file = match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&drm_device)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "screen-timecode: could not open {drm_device}: {e}\n\
                 Falling back to Poll mode (no vsync synchronisation)."
            );
            return Ok(());
        }
    };

    std::thread::Builder::new()
        .name("vsync-vblank".into())
        .spawn(move || {
            let fd = file.as_raw_fd();
            loop {
                // drm_vblank_wait is the low-level ioctl wrapper.
                // We use a relative wait for sequence=1 (next vblank).
                if drm_wait_vblank(fd).is_err() {
                    // DRM error or device gone — sleep briefly and retry.
                    std::thread::sleep(std::time::Duration::from_millis(16));
                    continue;
                }
                flag.store(true, Ordering::Relaxed);
            }
        })
        .expect("failed to spawn vsync-vblank thread");

    Ok(())
}

/// Call `DRM_IOCTL_WAIT_VBLANK` with `DRM_VBLANK_RELATIVE | sequence=1`.
#[cfg(target_os = "linux")]
fn drm_wait_vblank(fd: std::os::raw::c_int) -> Result<()> {
    // The drm crate provides typed wrappers.  We call the ioctl directly here
    // to avoid pulling the full `drm` feature set into a bench-only binary.
    //
    // Kernel ABI: `DRM_IOCTL_WAIT_VBLANK` = _IOWR('d', 0x06, drmVBlank)
    // See: include/drm/drm.h in the kernel tree.
    use nix::libc;

    const DRM_IOCTL_WAIT_VBLANK: u64 = 0xC018_6406;

    #[repr(C)]
    #[allow(non_camel_case_types)]
    union drmVBlankData {
        request: drmVBlankReq,
        reply: drmVBlankReply,
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    #[allow(non_camel_case_types)]
    struct drmVBlankReq {
        type_: u32,    // DRM_VBLANK_RELATIVE = 0x1
        sequence: u32, // 1 = next vblank
        signal: u64,   // 0 = synchronous
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    #[allow(non_camel_case_types)]
    struct drmVBlankReply {
        type_: u32,
        sequence: u32,
        tval_sec: i64,
        tval_usec: i64,
    }

    let mut vblank = drmVBlankData {
        request: drmVBlankReq {
            type_: 0x1, // DRM_VBLANK_RELATIVE
            sequence: 1,
            signal: 0,
        },
    };

    let ret = unsafe { libc::ioctl(fd, DRM_IOCTL_WAIT_VBLANK as _, &mut vblank) };
    if ret < 0 {
        return Err(anyhow::anyhow!(
            "DRM_IOCTL_WAIT_VBLANK failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

/// Non-Linux stub — never called but must compile.
#[cfg(not(target_os = "linux"))]
fn drm_wait_vblank(_fd: i32) -> Result<()> {
    Ok(())
}
