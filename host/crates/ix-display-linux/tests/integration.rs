#![cfg(target_os = "linux")]

//! Integration tests that require evdi loaded and a graphical session.
//!
//! Gate: these tests only run when compiled with `--features integration`.
//!
//! ```bash
//! # On a machine with evdi DKMS installed and a running Wayland/X11 session:
//! sudo modprobe evdi initial_device_count=1
//! cargo test -p ix-display-linux --features integration
//! ```

#![cfg(feature = "integration")]

use crossbeam::queue::ArrayQueue;
use ix_display::{DisplayMode, DisplaySource};
use ix_display_linux::{evdi::EvdiMonitor, LinuxDisplaySource};
use std::sync::Arc;
use std::time::Duration;

/// Verify that we can open and close an evdi device handle without crashing.
#[test]
fn evdi_open_close_roundtrip() {
    let mon = EvdiMonitor::open()
        .expect("evdi device should be present — run: sudo modprobe evdi initial_device_count=1");
    drop(mon);
}

/// Verify that a virtual monitor can be connected with the default 1080p120 mode.
#[test]
fn evdi_connect_disconnect() {
    let mut mon = EvdiMonitor::open().expect("evdi present");
    mon.connect(1920, 1080, 120)
        .expect("connect should succeed");
    drop(mon); // Drop calls evdi_disconnect + evdi_close
}

/// Verify that LinuxDisplaySource initialises and returns a frame within 2 s.
///
/// This test requires:
/// - A running Wayland session (WAYLAND_DISPLAY set) or X11 (DISPLAY set).
/// - evdi DKMS installed and the module loaded.
/// - xdg-desktop-portal running (Wayland path) or X with XShm+XDamage (X11).
#[test]
fn end_to_end_first_frame() {
    let mut src = LinuxDisplaySource::new().expect("LinuxDisplaySource::new() should succeed");
    let mode = DisplayMode::default(); // 1920×1080 @ 120 Hz
    src.create_virtual_monitor(mode)
        .expect("create_virtual_monitor should succeed");

    // The first frame may take up to 2 s while the compositor reconfigures
    // its output list and starts rendering to the new EVDI connector.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        if let Some(frame) = src.capture_frame() {
            assert!(
                frame.width >= 1920,
                "frame width too small: {}",
                frame.width
            );
            assert!(
                frame.height >= 1080,
                "frame height too small: {}",
                frame.height
            );
            src.destroy();
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    src.destroy();
    panic!("no frame received within 2 s — check compositor output list and evdi status");
}
