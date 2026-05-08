// Public.h — IOCTL interface between the vhf-stylus kernel driver and the
// iextendd user-mode partner (ix-input/src/windows.rs).
//
// The user-mode partner opens \\.\iExtendStylus and calls DeviceIoControl
// with IOCTL_IEXTEND_SUBMIT_REPORT to inject an 18-byte HID report.
//
// Both sides must agree on this header.  Keep it C-compatible so that the
// user-mode partner (Rust) can hard-code the IOCTL code without pulling in
// Windows DDK headers.

#pragma once
#include <initguid.h>

// Device interface GUID — used in INF [ClassInstall32] and CreateFileW path.
// {A1B2C3D4-E5F6-7890-ABCD-EF1234567890}
DEFINE_GUID(GUID_DEVINTERFACE_IEXTEND_STYLUS,
    0xa1b2c3d4, 0xe5f6, 0x7890,
    0xAB, 0xCD, 0xEF, 0x12, 0x34, 0x56, 0x78, 0x90);

// The symbolic link name exposed by the driver.
#define IEXTEND_STYLUS_SYMLINK  L"\\DosDevices\\iExtendStylus"

// IOCTL: submit an 18-byte HID input report.
//
// Input buffer:  18 bytes, layout per hid_report.h.
// Output buffer: none.
// Method:        METHOD_BUFFERED (kernel copies from user buffer).
//
// The numerical value (0x88001) matches what ix-input/src/windows.rs uses as
// IOCTL_IEXTEND_SUBMIT_REPORT.  Do not change without updating both sides.
#define FILE_DEVICE_IEXTEND_STYLUS  0x8800
#define IOCTL_IEXTEND_SUBMIT_REPORT \
    CTL_CODE(FILE_DEVICE_IEXTEND_STYLUS, 0x001, METHOD_BUFFERED, FILE_WRITE_DATA)
