// hid_report.h — HID report descriptor for the iExtend virtual stylus device.
//
// This descriptor defines an 18-byte digitizer input report that exposes:
//   X / Y coordinates   (i32, logical 0..32767 each)
//   Pressure            (u16, logical 0..1024)
//   Tilt-X / Tilt-Y     (i16, logical -9000..9000, in centidegrees)
//   Buttons             (u8: bit 0 = tip, bit 1 = barrel, bit 2 = hover)
//   Padding             (u8, reserved)
//
// The descriptor follows HID 1.11 §16.7 (Digitizer Page) and is compatible
// with the Windows Ink API (WM_POINTER / Pointer Input Messages).
//
// Plan 3's IddCx driver uses the same EV cert and WHQL submission pipeline;
// see that project's README for the signing workflow.

#pragma once
#include <ntddk.h>

#define HID_REPORT_ID               0x01
#define HID_REPORT_SIZE_BYTES       18

// The descriptor byte array length — computed from the descriptor below.
#define HID_REPORT_DESC_SIZE        96

// Report layout (matches ix-input/src/windows.rs build_stylus_hid_report):
//   Byte  0-3   : X   i32 LE  logical 0..32767
//   Byte  4-7   : Y   i32 LE  logical 0..32767
//   Byte  8-9   : Pressure  u16 LE  0..1024
//   Byte  10-11 : Tilt-X    i16 LE  -9000..9000
//   Byte  12-13 : Tilt-Y    i16 LE  -9000..9000
//   Byte  14    : Buttons   u8  bit 0=tip, 1=barrel, 2=hover
//   Byte  15-17 : reserved

extern const UCHAR HidReportDescriptor[HID_REPORT_DESC_SIZE];

// Physical device attributes (for IOCTL_HID_GET_DEVICE_ATTRIBUTES)
#define HID_VENDOR_ID    0x05AC  // Apple
#define HID_PRODUCT_ID   0x0002  // iExtend Stylus
#define HID_VERSION_NUM  0x0100
