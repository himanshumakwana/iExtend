#![cfg(target_os = "linux")]

//! Unit tests for SecureBoot detection.
//!
//! These tests use the `SecureBootProbe` trait to inject synthetic EFI-variable
//! byte payloads without touching `/sys/firmware/efi/efivars/`.

use ix_display_linux::secureboot::{is_secureboot_enabled, SecureBootProbe};

// ---------------------------------------------------------------------------
// Fake probe
// ---------------------------------------------------------------------------

struct FakeProbe {
    var_bytes: Option<Vec<u8>>,
}

impl SecureBootProbe for FakeProbe {
    fn read_secureboot_efivar(&self) -> Option<Vec<u8>> {
        self.var_bytes.clone()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn efi_var_present_and_enabled_returns_true() {
    // Standard UEFI layout: 4-byte attributes prefix + 1-byte value.
    // Value = 1 means SecureBoot enabled.
    let p = FakeProbe {
        var_bytes: Some(vec![
            0x07, 0x00, 0x00, 0x00, // EFI attribute word (NV | BS | RT)
            0x01, // SecureBoot value: 1 = enabled
        ]),
    };
    assert!(is_secureboot_enabled(&p));
}

#[test]
fn efi_var_present_but_zero_returns_false() {
    let p = FakeProbe {
        var_bytes: Some(vec![0x07, 0x00, 0x00, 0x00, 0x00]),
    };
    assert!(!is_secureboot_enabled(&p));
}

#[test]
fn no_efi_var_returns_false() {
    // Non-UEFI systems / VMs without EFI firmware.
    let p = FakeProbe { var_bytes: None };
    assert!(!is_secureboot_enabled(&p));
}

#[test]
fn single_byte_one_returns_true() {
    // Minimal payload — only the flag byte, no attribute prefix.
    // Some kernel versions or custom firmware may produce this.
    let p = FakeProbe {
        var_bytes: Some(vec![0x01]),
    };
    assert!(is_secureboot_enabled(&p));
}

#[test]
fn empty_bytes_returns_false() {
    // Pathological: zero-length file.
    let p = FakeProbe {
        var_bytes: Some(vec![]),
    };
    assert!(!is_secureboot_enabled(&p));
}
