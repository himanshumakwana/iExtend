#![cfg(target_os = "linux")]

//! Tests for the NVIDIA proprietary-driver detection.
//!
//! These tests do not require GPU hardware.  They verify that the probe
//! function runs without panicking and returns a plausible value.

#[cfg(unix)]
use ix_display_linux::nvidia_cuda::proprietary_driver_active;

#[test]
#[cfg(unix)]
fn probe_runs_without_panic() {
    // The result depends on the host; we just verify no panic.
    let result = proprietary_driver_active();
    // On a CI host without NVIDIA hardware this should be false.
    let _ = result;
}

#[test]
#[cfg(unix)]
fn probe_false_when_no_nvidia_module() {
    // On any non-NVIDIA host /sys/module/nvidia/version won't exist.
    // This verifies the probe gracefully returns false (no panic, no error).
    if !std::path::Path::new("/sys/module/nvidia/version").exists() {
        assert!(!proprietary_driver_active());
    }
}
