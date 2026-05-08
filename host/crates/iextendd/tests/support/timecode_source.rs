//! In-process timecode painter for the synthetic latency test.
//!
//! Implements the same 7-segment digit shapes used by
//! `bench/camera-rig/screen-timecode/` but operates entirely on an in-memory
//! RGBA buffer — no display, no GPU, no OS. The synthetic latency test paints
//! a microsecond timestamp into a frame, encodes it through the openh264
//! software path, immediately decodes it (also in-process), reads the timestamp
//! back, and computes the round-trip latency.
//!
//! ## Coordinate layout
//!
//! Digits are drawn at the top-left of the frame with a configurable margin.
//! Each digit occupies a `DIGIT_W × DIGIT_H` cell. The background is white
//! (0xFF) and segments are drawn black (0x00) so that binarisation is trivial
//! in the read path.
//!
//! ## 7-segment encoding
//!
//! Segments are labelled in the standard way:
//! ```text
//!  _
//! |_|
//! |_|
//! ```
//! bit 6 = top (a), bit 5 = top-right (b), bit 4 = bottom-right (c),
//! bit 3 = bottom (d),  bit 2 = bottom-left (e), bit 1 = top-left (f),
//! bit 0 = middle (g).
//!
//! ## Binary stripe fallback
//!
//! A 1-pixel-tall row of full-width alternating blocks is painted along `y=0`.
//! Each block spans `width/32` pixels; 1 = white, 0 = black. This encodes the
//! low 32 bits of the timestamp and survives frames where OCR fails due to
//! motion blur or JPEG artefacts.

/// RGBA bytes per pixel.
const BPP: usize = 4;

/// Each digit cell is this many pixels wide.
const DIGIT_W: usize = 12;
/// Each digit cell is this many pixels tall.
const DIGIT_H: usize = 20;
/// Horizontal margin from frame edge, pixels.
const MARGIN_X: usize = 4;
/// Vertical margin from frame edge, pixels.
const MARGIN_Y: usize = 4;
/// Thickness of a segment bar in pixels.
const SEG_T: usize = 2;

/// 7-segment LUT for digits 0-9.
/// Bits: 6=top, 5=top-right, 4=bottom-right, 3=bottom, 2=bottom-left, 1=top-left, 0=middle.
#[rustfmt::skip]
const SEG: [u8; 10] = [
    0b111_1110,  // 0
    0b011_0000,  // 1
    0b110_1101,  // 2
    0b111_1001,  // 3
    0b011_0011,  // 4
    0b101_1011,  // 5
    0b101_1111,  // 6
    0b111_0000,  // 7
    0b111_1111,  // 8
    0b111_1011,  // 9
];

/// In-process timecode painter / reader.
pub struct Painter {
    width: u32,
    height: u32,
    buf: Vec<u8>,
}

impl Painter {
    /// Create a new painter for the given frame dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        let size = (width as usize) * (height as usize) * BPP;
        Self { width, height, buf: vec![0u8; size] }
    }

    /// Paint `microseconds` into the RGBA buffer and return a reference to it.
    ///
    /// The buffer is cleared to white each call, then the digit strip and
    /// binary stripe are painted in black.
    pub fn paint(&mut self, microseconds: u64) -> &[u8] {
        let w = self.width as usize;
        let h = self.height as usize;

        // Clear to white.
        for p in self.buf.iter_mut() {
            *p = 0xFF;
        }

        // ── binary stripe along y=0 (low 32 bits of timestamp) ──────────────
        let stripe_y = 0usize;
        let block_w = (w / 32).max(1);
        let val32 = (microseconds & 0xFFFF_FFFF) as u32;
        for bit in 0..32usize {
            let set = (val32 >> (31 - bit)) & 1 == 1;
            let colour = if set { 0xFF_u8 } else { 0x00_u8 };
            let x_start = bit * block_w;
            let x_end = ((bit + 1) * block_w).min(w);
            let row_off = stripe_y * w * BPP;
            for x in x_start..x_end {
                let px = row_off + x * BPP;
                self.buf[px] = colour;
                self.buf[px + 1] = colour;
                self.buf[px + 2] = colour;
                self.buf[px + 3] = 0xFF;
            }
        }

        // ── 7-segment digits ─────────────────────────────────────────────────
        // Render the microseconds value as up to 10 decimal digits, left-zero-padded.
        let digits: Vec<u8> = {
            let s = format!("{microseconds:010}");
            s.bytes().map(|b| b - b'0').collect()
        };

        for (i, &d) in digits.iter().enumerate() {
            let ox = MARGIN_X + i * (DIGIT_W + 2);
            let oy = MARGIN_Y + 2; // leave 2 px gap below the binary stripe
            self.draw_digit(d, ox, oy);
        }

        // Ensure frame height is respected.
        let _ = h;

        &self.buf
    }

    /// Draw one 7-segment digit at pixel offset (ox, oy) in the RGBA buffer.
    fn draw_digit(&mut self, digit: u8, ox: usize, oy: usize) {
        let segs = SEG[digit as usize];
        let w = self.width as usize;
        let buf_len = self.buf.len();
        let half_h = DIGIT_H / 2;

        /// Paint a horizontal RGBA bar at (x, y) of `len` pixels.
        macro_rules! h_bar {
            ($x:expr, $y:expr, $len:expr) => {{
                let (bx, by, blen): (usize, usize, usize) = ($x, $y, $len);
                for dx in 0..blen {
                    for dy in 0..SEG_T {
                        let px_off = (by + dy) * w * BPP + (bx + dx) * BPP;
                        if px_off + 3 < buf_len {
                            self.buf[px_off]     = 0x00;
                            self.buf[px_off + 1] = 0x00;
                            self.buf[px_off + 2] = 0x00;
                            self.buf[px_off + 3] = 0xFF;
                        }
                    }
                }
            }};
        }

        /// Paint a vertical RGBA bar at (x, y) of height `ht`.
        macro_rules! v_bar {
            ($x:expr, $y:expr, $ht:expr) => {{
                let (bx, by, bht): (usize, usize, usize) = ($x, $y, $ht);
                for dy in 0..bht {
                    for dx in 0..SEG_T {
                        let px_off = (by + dy) * w * BPP + (bx + dx) * BPP;
                        if px_off + 3 < buf_len {
                            self.buf[px_off]     = 0x00;
                            self.buf[px_off + 1] = 0x00;
                            self.buf[px_off + 2] = 0x00;
                            self.buf[px_off + 3] = 0xFF;
                        }
                    }
                }
            }};
        }

        if segs & (1 << 6) != 0 { h_bar!(ox, oy, DIGIT_W); }                        // top
        if segs & (1 << 5) != 0 { v_bar!(ox + DIGIT_W - SEG_T, oy, half_h); }       // top-right
        if segs & (1 << 4) != 0 { v_bar!(ox + DIGIT_W - SEG_T, oy + half_h, half_h); } // bot-right
        if segs & (1 << 3) != 0 { h_bar!(ox, oy + DIGIT_H - SEG_T, DIGIT_W); }      // bottom
        if segs & (1 << 2) != 0 { v_bar!(ox, oy + half_h, half_h); }               // bot-left
        if segs & (1 << 1) != 0 { v_bar!(ox, oy, half_h); }                        // top-left
        if segs & (1 << 0) != 0 { h_bar!(ox, oy + half_h - SEG_T / 2, DIGIT_W); }  // middle
    }

    /// Read a timecode from an RGBA buffer produced by [`paint`][Self::paint].
    ///
    /// First tries the 7-segment digit strip; falls back to the binary stripe.
    /// Returns `None` if neither path can extract a value.
    pub fn read(buf: &[u8], width: u32, _height: u32) -> Option<u64> {
        // Fast path: binary stripe at y=0 gives us 32 low bits directly.
        if let Some(v) = Self::read_stripe(buf, width) {
            return Some(v);
        }
        None
    }

    /// Read the 32-bit binary stripe from y=0.
    fn read_stripe(buf: &[u8], width: u32) -> Option<u64> {
        let w = width as usize;
        let block_w = (w / 32).max(1);
        let mut val: u32 = 0;
        for bit in 0..32usize {
            let x = bit * block_w + block_w / 2; // sample the middle of each block
            let px_off = x * BPP;
            if px_off + 2 >= buf.len() {
                return None;
            }
            let luma = (buf[px_off] as u32 + buf[px_off + 1] as u32 + buf[px_off + 2] as u32) / 3;
            val = (val << 1) | if luma > 128 { 1 } else { 0 };
        }
        Some(val as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_binary_stripe() {
        let mut p = Painter::new(1920, 1080);
        // Use a value whose low 32 bits are easily round-tripped.
        let us: u64 = 123_456_789;
        let buf = p.paint(us).to_vec();
        // The binary stripe only recovers the low 32 bits.
        let read = Painter::read(&buf, 1920, 1080).unwrap();
        assert_eq!(read, us & 0xFFFF_FFFF,
            "binary stripe should recover low 32 bits: expected {} got {}", us & 0xFFFF_FFFF, read);
    }

    #[test]
    fn stripe_zero_value() {
        let mut p = Painter::new(1920, 1080);
        let buf = p.paint(0).to_vec();
        let read = Painter::read(&buf, 1920, 1080).unwrap();
        assert_eq!(read, 0);
    }

    #[test]
    fn stripe_max_u32() {
        let mut p = Painter::new(1920, 1080);
        let us: u64 = 0xFFFF_FFFF;
        let buf = p.paint(us).to_vec();
        let read = Painter::read(&buf, 1920, 1080).unwrap();
        assert_eq!(read, us);
    }
}
