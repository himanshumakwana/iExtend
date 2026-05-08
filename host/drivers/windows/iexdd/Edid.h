// Edid.h — synthesised EDID declarations.
//
// Manufacturer ID: "IXT" (custom, not registered with VESA).
// This is acceptable for an indirect display driver — VESA registration is only
// required for physical display hardware submitted to the EDID database.
//
// The 128-byte EDID 1.4 block is built in Edid.c with correct checksums.

#pragma once
#include <ntddk.h>

// Size of the base EDID block in bytes (EDID 1.x; no extension blocks).
#define IEXDD_EDID_SIZE 128

EXTERN_C_START

// Build a 128-byte EDID 1.4 block into `pEdid`.
//
// `pFriendlyName` is stored as the monitor serial-number ASCII string in
// descriptor block 3 (18-byte text field, max 13 chars). If NULL the default
// "iExtend VDisp" is used.
//
// The final byte (offset 127) is patched so the checksum of all 128 bytes
// equals 0 mod 256 as required by the EDID specification.
VOID IexddBuildEdid(
    _Out_writes_bytes_(IEXDD_EDID_SIZE) PUCHAR pEdid,
    _In_opt_z_                          PCSTR  pFriendlyName
);

EXTERN_C_END
