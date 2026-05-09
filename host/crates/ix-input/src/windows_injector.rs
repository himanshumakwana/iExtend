// windows.rs — Windows user-mode partner for the vhf-stylus kernel driver.
//
// This module is compiled only on Windows (`#[cfg(windows)]`).
// It opens the `\\.\iExtendStylus` device node exposed by `vhf-stylus.sys`
// and forwards 18-byte HID reports via DeviceIoControl.
//
// The kernel driver (host/drivers/windows/vhf-stylus/) processes these
// reports and synthesises Windows Ink / Pointer Input events that all modern
// apps (Photoshop 2024+, Clip Studio, Krita 5.2+) consume natively.

use crate::wire::{self, KeyPayload, Kind, PencilPayload, TouchPayload};
use crate::Injector;
use std::io;

// On Linux the windows-rs types aren't available; use type aliases so the file
// compiles with `#[cfg(windows)]` gating but the dead-code analyser is happy.
#[cfg(windows)]
use windows::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::{CreateFileW, FILE_SHARE_NONE, OPEN_EXISTING},
    System::IO::DeviceIoControl,
};

/// IOCTL code for VHF report submission.
/// Matches the driver's IOCTL_IEXTEND_SUBMIT_REPORT definition in Public.h.
pub const IOCTL_IEXTEND_SUBMIT_REPORT: u32 = 0x88_001;

/// Windows user-mode injector.
///
/// Opens `\\.\iExtendStylus` on construction.  Requires the vhf-stylus driver
/// to be installed and the host process to be running as an elevated user or
/// a user with the appropriate ACL on the device object.
pub struct WindowsInjector {
    #[cfg(windows)]
    device: HANDLE,
    #[cfg(not(windows))]
    _device: (),
}

impl WindowsInjector {
    /// Open the vhf-stylus device node.
    pub fn new() -> io::Result<Self> {
        #[cfg(windows)]
        {
            use windows::core::PCWSTR;
            use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_WRITE};
            let path: Vec<u16> = "\\\\.\\iExtendStylus\0".encode_utf16().collect();
            let handle = unsafe {
                CreateFileW(
                    PCWSTR(path.as_ptr()),
                    FILE_GENERIC_WRITE.0,
                    FILE_SHARE_NONE,
                    None,
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL,
                    None,
                )
            };
            match handle {
                Ok(h) => Ok(Self { device: h }),
                Err(e) => Err(io::Error::from_raw_os_error(e.code().0)),
            }
        }
        #[cfg(not(windows))]
        {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "WindowsInjector only available on Windows",
            ))
        }
    }

    fn submit_hid_report(&self, report: &[u8; 18]) -> io::Result<()> {
        #[cfg(windows)]
        {
            let mut bytes_returned: u32 = 0;
            let ok = unsafe {
                DeviceIoControl(
                    self.device,
                    IOCTL_IEXTEND_SUBMIT_REPORT,
                    Some(report.as_ptr() as *const _),
                    report.len() as u32,
                    None,
                    0,
                    Some(&mut bytes_returned),
                    None,
                )
            };
            ok.map_err(|e| io::Error::from_raw_os_error(e.code().0))?;
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = report;
            Ok(())
        }
    }
}

impl Injector for WindowsInjector {
    fn inject(&self, p: &wire::Packet) {
        let report: [u8; 18] = match p.kind {
            Kind::PencilBegin | Kind::PencilMove | Kind::PencilEnd => {
                let raw: &[u8; 18] = p.payload[..18].try_into().unwrap();
                let pl = PencilPayload::from_bytes(raw);
                build_stylus_hid_report(&pl, p.kind == Kind::PencilEnd)
            }
            Kind::TouchBegin | Kind::TouchMove | Kind::TouchEnd => {
                let raw: &[u8; 18] = p.payload[..18].try_into().unwrap();
                let pl = TouchPayload::from_bytes(raw);
                build_touch_hid_report(&pl, p.kind == Kind::TouchEnd)
            }
            Kind::KeyDown | Kind::KeyUp | Kind::Modifier => {
                let raw: &[u8; 18] = p.payload[..18].try_into().unwrap();
                let _pl = KeyPayload::from_bytes(raw);
                // Keyboard injection goes through a separate SendInput path,
                // not the stylus HID device.  Skip for this report path.
                return;
            }
        };
        if let Err(e) = self.submit_hid_report(&report) {
            tracing::warn!("WindowsInjector: submit failed: {e}");
        }
    }
}

// ── HID report formatters ─────────────────────────────────────────────────────

/// Build an 18-byte HID input report for the stylus device.
///
/// Report layout (matches hid_report.h in the vhf-stylus driver):
///   0-3   X  i32 LE  logical 0..32767
///   4-7   Y  i32 LE  logical 0..32767
///   8-9   Pressure u16 LE  0..1024
///  10-11  Tilt-X  i16 LE  -9000..9000 (millidegrees/100)
///  12-13  Tilt-Y  i16 LE
///  14     Buttons  bit 0 = tip, bit 1 = barrel, bit 2 = hover
///  15     reserved
///  16-17  reserved
fn build_stylus_hid_report(pl: &PencilPayload, lift: bool) -> [u8; 18] {
    let mut r = [0u8; 18];
    let x = pl.x.round() as i32;
    let y = pl.y.round() as i32;
    let pressure = (pl.pressure * 1024.0).round().clamp(0.0, 1024.0) as u16;
    let tilt_x = (pl.tilt.cos() * pl.azimuth.cos() * 9000.0).round() as i16;
    let tilt_y = (pl.tilt.cos() * pl.azimuth.sin() * 9000.0).round() as i16;
    let buttons: u8 = (if lift { 0 } else { 1 })  // bit 0 = tip
                    | (if pl.barrel { 2 } else { 0 })
                    | (if pl.hover  { 4 } else { 0 });
    r[0..4].copy_from_slice(&x.to_le_bytes());
    r[4..8].copy_from_slice(&y.to_le_bytes());
    r[8..10].copy_from_slice(&pressure.to_le_bytes());
    r[10..12].copy_from_slice(&tilt_x.to_le_bytes());
    r[12..14].copy_from_slice(&tilt_y.to_le_bytes());
    r[14] = buttons;
    r
}

/// Build an 18-byte HID report for the touch device.
fn build_touch_hid_report(pl: &TouchPayload, lift: bool) -> [u8; 18] {
    let mut r = [0u8; 18];
    let x = pl.x.round() as i32;
    let y = pl.y.round() as i32;
    let contact: u8 = if lift { 0 } else { 1 };
    r[0..4].copy_from_slice(&x.to_le_bytes());
    r[4..8].copy_from_slice(&y.to_le_bytes());
    r[16] = contact;
    r
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stylus_report_tip_bit_set_on_contact() {
        let pl = PencilPayload {
            x: 100.0,
            y: 200.0,
            pressure: 0.5,
            tilt: 0.0,
            azimuth: 0.0,
            twist: 0.0,
            barrel: false,
            hover: false,
        };
        let r = build_stylus_hid_report(&pl, false /*lift=false → contact*/);
        assert_eq!(r[14] & 1, 1, "tip bit should be set on contact");
    }

    #[test]
    fn stylus_report_tip_bit_clear_on_lift() {
        let pl = PencilPayload {
            x: 0.0,
            y: 0.0,
            pressure: 0.0,
            tilt: 0.0,
            azimuth: 0.0,
            twist: 0.0,
            barrel: false,
            hover: false,
        };
        let r = build_stylus_hid_report(&pl, true /*lift*/);
        assert_eq!(r[14] & 1, 0, "tip bit should be clear on lift");
    }

    #[test]
    fn stylus_report_barrel_bit() {
        let pl = PencilPayload {
            x: 0.0,
            y: 0.0,
            pressure: 0.5,
            tilt: 0.0,
            azimuth: 0.0,
            twist: 0.0,
            barrel: true,
            hover: false,
        };
        let r = build_stylus_hid_report(&pl, false);
        assert_eq!(r[14] & 2, 2, "barrel bit should be set");
    }

    #[test]
    fn stylus_report_coordinates_in_bytes() {
        let pl = PencilPayload {
            x: 1000.0,
            y: 2000.0,
            pressure: 0.0,
            tilt: 0.0,
            azimuth: 0.0,
            twist: 0.0,
            barrel: false,
            hover: false,
        };
        let r = build_stylus_hid_report(&pl, false);
        let x = i32::from_le_bytes(r[0..4].try_into().unwrap());
        let y = i32::from_le_bytes(r[4..8].try_into().unwrap());
        assert_eq!(x, 1000);
        assert_eq!(y, 2000);
    }

    #[test]
    fn windows_injector_new_fails_on_linux() {
        // On Linux, WindowsInjector::new() must return an error rather than panic.
        let result = WindowsInjector::new();
        assert!(
            result.is_err(),
            "WindowsInjector should fail on non-Windows"
        );
    }
}
