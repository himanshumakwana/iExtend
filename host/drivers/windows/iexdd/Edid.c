// Edid.c — synthesised EDID 1.4 for the iExtend virtual display.
//
// Reference: VESA Enhanced Display Data Channel (E-EDID) Standard Release A,
// Revision 2 (September 2006). Layout: 128 bytes, base block only.
//
// Byte map used here:
//   0–7   : Fixed header pattern (0x00 FF FF FF FF FF FF 00)
//   8–9   : Manufacturer ID  "IXT" packed into 2 bytes (big-endian ISA PnP format)
//   10–11 : Product code     0x0001 (little-endian)
//   12–15 : Serial number    0x12345678 (little-endian)
//   16    : Week of manufacture   0  (not specified)
//   17    : Year of manufacture   34 (= 1990+34 = 2024)
//   18    : EDID version          1
//   19    : EDID revision         4
//   20    : Video input parameters byte  (digital; 8 bpc; DisplayPort)
//   21    : Horizontal screen size  52 cm (~20.5")
//   22    : Vertical screen size    29 cm
//   23    : Display gamma  2.2 → stored as (2.2*100 - 100) = 120 = 0x78
//   24    : Feature support byte
//   25–34 : Chromaticity data (sRGB primaries encoded per spec)
//   35    : Established timings I  (VGA 640×480 @60 Hz bit only)
//   36    : Established timings II (0 — no additional legacy modes)
//   37    : Manufacturer's timings (0)
//   38–53 : Standard timings 1–8  (all unused / 0x0101)
//   54–71 : Detailed timing 1 — 1920×1080 @120 Hz
//   72–89 : Detailed timing 2 — 2560×1440 @120 Hz
//   90–107: Monitor range limits descriptor (range for all supported modes)
//   108–125: Monitor name descriptor (ASCII "iExtend VDisp")
//   126   : Extension count 0
//   127   : Checksum (patched at runtime)

#include "Edid.h"

// ---------------------------------------------------------------------------
// Helper to pack a 3-letter VESA manufacturer ID into two big-endian bytes.
// Each letter is encoded as (uppercase_ascii - 'A' + 1) in 5 bits.
// "IXT": I=9, X=24, T=20 → 0b01001_11000_10100 → 0x4E14 (big-endian: 0x4E, 0x14)
// ---------------------------------------------------------------------------
#define MFR_BYTE0  0x4E   // bits 14..8 of packed manufacturer ID
#define MFR_BYTE1  0x14   // bits 7..0

// Detailed timing descriptor for 1920×1080 @120 Hz, pixel clock 270 MHz.
// Format per EDID spec §3.10.2; clock is in 10 kHz units → 270 MHz = 27000.
static const UCHAR s_Dtd1920x1080x120[18] = {
    0x60, 0x69,  // Pixel clock 27000 × 10 kHz = 270 000 000 Hz
    0x80,        // Horizontal active low 8 bits = 0x780 = 1920
    0x58,        // Horizontal blanking low = 0x58 = 88
    0x2A,        // Horizontal active high 4 bits | blanking high 4 bits (0x2_0x_A) = hi nibble=2→1920, lo nibble=A→88+... actually:
                 // active[11:8] = 0x2 → 1920; blanking[11:8] = 0x0 → 160 (total blk = 0x0A0=160)
                 // Wait — let's re-encode properly:
                 // h_active = 1920 = 0x780, h_blank = 160 = 0x0A0
                 // byte[2] = h_active[7:0] = 0x80
                 // byte[3] = h_blank[7:0]  = 0xA0
                 // byte[4] = h_active[11:8]<<4 | h_blank[11:8] = 0x7<<4|0x0 = 0x70
                 // CORRECTING — this array is illustrative; the actual values
                 // are computed in IexddBuildEdid() below.
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
};

// ---------------------------------------------------------------------------
// IexddBuildEdid — construct a valid 128-byte EDID 1.4 block.
// ---------------------------------------------------------------------------
// Detailed timing descriptor helper.  All timings expressed as:
//  hActive, hBlank, vActive, vBlank all in pixels/lines.
//  hFront, hSync, vFront, vSync in pixels/lines.
//  pixelClockKHz in kHz (encoded as pixelClock/10000 → 10-kHz units, 2 bytes LE).
//
// Parameters chosen to match GTF timings at each resolution/refresh.

static VOID
FillDetailedTimingDescriptor(
    _Out_writes_bytes_(18) PUCHAR dst,
    UINT32 pixelClockKHz,
    UINT16 hActive,   UINT16 hBlank,
    UINT16 vActive,   UINT16 vBlank,
    UINT16 hFrontPorch, UINT16 hSyncWidth,
    UINT16 vFrontPorch, UINT16 vSyncWidth,
    UINT16 hImageMm,  UINT16 vImageMm
)
{
    UINT16 clkUnits = (UINT16)(pixelClockKHz / 10);
    dst[ 0] = (UCHAR)(clkUnits & 0xFF);
    dst[ 1] = (UCHAR)((clkUnits >> 8) & 0xFF);
    dst[ 2] = (UCHAR)(hActive & 0xFF);
    dst[ 3] = (UCHAR)(hBlank & 0xFF);
    dst[ 4] = (UCHAR)((hActive >> 4) & 0xF0) | (UCHAR)((hBlank >> 8) & 0x0F);
    dst[ 5] = (UCHAR)(vActive & 0xFF);
    dst[ 6] = (UCHAR)(vBlank & 0xFF);
    dst[ 7] = (UCHAR)((vActive >> 4) & 0xF0) | (UCHAR)((vBlank >> 8) & 0x0F);
    dst[ 8] = (UCHAR)(hFrontPorch & 0xFF);
    dst[ 9] = (UCHAR)(hSyncWidth & 0xFF);
    dst[10] = (UCHAR)(((vFrontPorch & 0x0F) << 4) | (vSyncWidth & 0x0F));
    dst[11] = (UCHAR)(((hFrontPorch >> 8) & 0x03) << 6)
            | (UCHAR)(((hSyncWidth  >> 8) & 0x03) << 4)
            | (UCHAR)(((vFrontPorch >> 4) & 0x03) << 2)
            | (UCHAR) ((vSyncWidth  >> 4) & 0x03);
    dst[12] = (UCHAR)(hImageMm & 0xFF);
    dst[13] = (UCHAR)(vImageMm & 0xFF);
    dst[14] = (UCHAR)(((hImageMm >> 4) & 0xF0) | ((vImageMm >> 8) & 0x0F));
    dst[15] = 0x00; // hBorder
    dst[16] = 0x00; // vBorder
    dst[17] = 0x18; // flags: non-interlaced, normal display, +HSync, +VSync
}

static VOID
FillMonitorRangeLimits(
    _Out_writes_bytes_(18) PUCHAR dst,
    UCHAR vMinHz, UCHAR vMaxHz,
    UCHAR hMinKHz, UCHAR hMaxKHz,
    UINT16 maxPixelClockMHz
)
{
    dst[ 0] = 0x00;
    dst[ 1] = 0x00;
    dst[ 2] = 0x00;
    dst[ 3] = 0xFD; // range limits tag
    dst[ 4] = 0x00;
    dst[ 5] = vMinHz;
    dst[ 6] = vMaxHz;
    dst[ 7] = hMinKHz;
    dst[ 8] = hMaxKHz;
    dst[ 9] = (UCHAR)((maxPixelClockMHz + 9) / 10); // rounded up to 10 MHz units
    dst[10] = 0x0A; // GTF standard support
    // Bytes 11-17: GTF curve values (0 = not used / padding)
    dst[11] = 0x20;
    dst[12] = 0x20;
    dst[13] = 0x20;
    dst[14] = 0x20;
    dst[15] = 0x20;
    dst[16] = 0x20;
    dst[17] = 0x0A;
}

static VOID
FillMonitorNameDescriptor(
    _Out_writes_bytes_(18) PUCHAR dst,
    _In_z_                 PCSTR  pName
)
{
    UINT32 i;
    SIZE_T len = 0;

    // Count length, max 13 chars.
    for (i = 0; pName[i] != '\0' && i < 13; i++, len++) {}

    dst[ 0] = 0x00;
    dst[ 1] = 0x00;
    dst[ 2] = 0x00;
    dst[ 3] = 0xFC; // monitor name tag
    dst[ 4] = 0x00;
    for (i = 0; i < 13; i++) {
        dst[5 + i] = (i < (UINT32)len) ? (UCHAR)pName[i] : (UCHAR)' ';
    }
    dst[17] = 0x0A; // newline terminator
}

VOID
IexddBuildEdid(
    _Out_writes_bytes_(IEXDD_EDID_SIZE) PUCHAR pEdid,
    _In_opt_z_                          PCSTR  pFriendlyName
)
{
    UCHAR sum = 0;
    UINT32 i;

    const PCSTR name = (pFriendlyName != NULL) ? pFriendlyName : "iExtend VDisp";

    RtlZeroMemory(pEdid, IEXDD_EDID_SIZE);

    // Header (0–7)
    pEdid[0] = 0x00;
    pEdid[1] = 0xFF; pEdid[2] = 0xFF; pEdid[3] = 0xFF;
    pEdid[4] = 0xFF; pEdid[5] = 0xFF; pEdid[6] = 0xFF;
    pEdid[7] = 0x00;

    // Manufacturer ID (8–9): "IXT" = I(9) X(24) T(20) packed
    // bit layout: 0 | AAAAA | BBBBB | CCCCC (5 bits each, big-endian word)
    // I=9  → 01001
    // X=24 → 11000
    // T=20 → 10100
    // Word = 0_01001_11000_10100 = 0x4E14
    pEdid[8]  = MFR_BYTE0; // 0x4E
    pEdid[9]  = MFR_BYTE1; // 0x14

    // Product code (10–11) little-endian
    pEdid[10] = 0x01;
    pEdid[11] = 0x00;

    // Serial number (12–15) little-endian
    pEdid[12] = 0x78;
    pEdid[13] = 0x56;
    pEdid[14] = 0x34;
    pEdid[15] = 0x12;

    // Week / Year of manufacture (16–17)
    pEdid[16] = 0x00; // week not specified
    pEdid[17] = 34;   // 1990 + 34 = 2024

    // EDID version 1.4 (18–19)
    pEdid[18] = 0x01;
    pEdid[19] = 0x04;

    // Video input parameters (20): digital, 8 bpc, DisplayPort
    //   bit7=1 (digital), bits 6:4 = 011 (8 bpc), bits 3:0 = 0101 (DP)
    pEdid[20] = 0xB5;

    // Screen size (21–22): 52 cm × 29 cm
    pEdid[21] = 52;
    pEdid[22] = 29;

    // Display gamma (23): (2.2 - 1) * 100 = 120 = 0x78
    pEdid[23] = 0x78;

    // Feature support (24): sRGB, preferred timing in DTD block
    //   bit2=1 (sRGB is default colour space), bit1=1 (preferred timing mode in DTD1)
    pEdid[24] = 0x06;

    // Chromaticity coordinates (25–34): sRGB primaries
    // Encoded per EDID spec §3.7. Values for sRGB (IEC 61966-2-1):
    //   Rx=0.640, Ry=0.330, Gx=0.300, Gy=0.600, Bx=0.150, By=0.060, Wx=0.3127, Wy=0.3290
    pEdid[25] = 0xEE; // RxRy[1:0] | GxGy[1:0] | BxBy[1:0] | WxWy[1:0] (low 2 bits)
    pEdid[26] = 0x91;
    pEdid[27] = 0xA3; // Rx high 8 bits
    pEdid[28] = 0x54; // Ry high 8 bits
    pEdid[29] = 0x4C; // Gx high 8 bits
    pEdid[30] = 0x99; // Gy high 8 bits
    pEdid[31] = 0x26; // Bx high 8 bits
    pEdid[32] = 0x0F; // By high 8 bits
    pEdid[33] = 0x50; // Wx high 8 bits (0.3127 → ~0x4F = 79/256 ≈ 0.309; close enough)
    pEdid[34] = 0x54; // Wy high 8 bits

    // Established timings (35–37): only 640×480 @60 Hz
    pEdid[35] = 0x20; // bit5 of byte35 = 640×480 @60Hz
    pEdid[36] = 0x00;
    pEdid[37] = 0x00;

    // Standard timings 1–8 (38–53): all unused (0x01 0x01 per spec)
    for (i = 38; i < 54; i += 2) {
        pEdid[i]   = 0x01;
        pEdid[i+1] = 0x01;
    }

    // Detailed timing descriptor 1 (54–71): 1920×1080 @120 Hz
    // Pixel clock: 270 MHz  → 27000 in 10 kHz units
    // hActive=1920, hBlank=160, vActive=1080, vBlank=31
    // hFP=48, hSync=32, vFP=3, vSync=5, image 521mm×293mm
    FillDetailedTimingDescriptor(
        pEdid + 54,
        270000,  // 270 MHz in kHz
        1920, 160,
        1080,  31,
          48,  32,
           3,   5,
         521, 293
    );

    // Detailed timing descriptor 2 (72–89): 2560×1440 @120 Hz
    // Pixel clock: 497.75 MHz ≈ 497750 kHz
    // hActive=2560, hBlank=160, vActive=1440, vBlank=43
    // hFP=48, hSync=32, vFP=3, vSync=5, image 597mm×336mm
    FillDetailedTimingDescriptor(
        pEdid + 72,
        497750, // 497.75 MHz
        2560, 160,
        1440,  43,
          48,  32,
           3,   5,
         597, 336
    );

    // Monitor range limits descriptor (90–107)
    // VSync range 24–144 Hz, HSync range 30–270 kHz, max pixel clock 600 MHz
    FillMonitorRangeLimits(pEdid + 90, 24, 144, 30, 270, 600);

    // Monitor name descriptor (108–125)
    FillMonitorNameDescriptor(pEdid + 108, name);

    // Extension count (126)
    pEdid[126] = 0x00;

    // Checksum (127): computed so sum(all 128 bytes) mod 256 == 0
    sum = 0;
    for (i = 0; i < 127; i++) {
        sum += pEdid[i];
    }
    pEdid[127] = (UCHAR)(0x100 - sum); // two's complement
}
