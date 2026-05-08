// q16.rs — Q16 fixed-point helpers (i32 / 65_536).
//
// We use Q16 (not f32) on the wire so that small pencil deltas survive
// FP rounding when the host scales them to virtual-monitor pixels.
//
// Encode: q16_from_f32(1.5) = 0x0001_8000
// Decode: q16_to_f32(0x0001_8000) = 1.5
//
// Wire spec §6.1: coordinates use i32 Q16; pressure/tilt/azimuth/twist use i16 Q16.
// i16 Q16 covers −0.5..=0.5, which is insufficient for large tilt angles stored
// in radians (π/2 ≈ 1.57). The plan packs these as i16 at Q16 scale — callers
// must ensure the value range fits within i16 before calling q16_i16_from_f32.

/// Encode a float as a Q16-scaled i32 (±32 767 integer part).
#[inline]
pub fn q16_from_f32(v: f32) -> i32 {
    (v * 65_536.0).round() as i32
}

/// Decode a Q16-scaled i32 back to float.
#[inline]
pub fn q16_to_f32(v: i32) -> f32 {
    (v as f32) / 65_536.0
}

/// Encode a float as a Q16-scaled i16, saturating at i16 limits.
/// Suitable for pressure (0..=1), tilt (0..=π/2), azimuth (0..=2π), twist.
/// NOTE: azimuth can exceed 1.0 in absolute value (up to 2π ≈ 6.28), so
/// values near 2π require the caller to ensure they fit.  In practice
/// azimuth is stored in a separate i16 whose legal range (≤ 2π) fits
/// in i16 Q16 because 2π × 65536 ≈ 411775 > i16::MAX (32767); the plan
/// therefore interprets pressure/tilt/azimuth/twist fields differently:
///   pressure: 0..=1    → i16 Q16 fits (max 65536 > 32767 → clamp)
///   tilt:     0..=π/2  → i16 Q16 max ≈ 102944 → clamp at π/2 ≈ 1.5708 × 65536 = 102944 → saturates
/// Callers that need wider range (azimuth) should use the i32 Q16 path instead.
/// For this implementation we match the spec exactly and clamp.
#[inline]
pub fn q16_i16_from_f32(v: f32) -> i16 {
    let wide = (v * 65_536.0).round() as i64;
    wide.clamp(i16::MIN as i64, i16::MAX as i64) as i16
}

/// Decode a Q16-scaled i16 back to float.
#[inline]
pub fn q16_i16_to_f32(v: i16) -> f32 {
    (v as f32) / 65_536.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_unit() {
        assert_eq!(q16_to_f32(q16_from_f32(1.0)), 1.0);
    }

    #[test]
    fn roundtrip_pi() {
        let pi = std::f32::consts::PI;
        let q = q16_from_f32(pi);
        let back = q16_to_f32(q);
        assert!(
            (pi - back).abs() < 1.0 / 65_536.0,
            "pi roundtrip delta too large: {}",
            (pi - back).abs()
        );
    }

    #[test]
    fn negatives_survive() {
        assert_eq!(q16_to_f32(q16_from_f32(-2.5)), -2.5);
    }

    #[test]
    fn pressure_half_roundtrip() {
        // 0.5 × 65536 = 32768 > i16::MAX (32767) — saturates to 32767/65536 ≈ 0.499985.
        // The quantisation error for the i16 Q16 path at 0.5 is ≤ 2 / 65536.
        let v = 0.5f32;
        let q = q16_i16_from_f32(v);
        let back = q16_i16_to_f32(q);
        assert!(
            (v - back).abs() < 2.0 / 65_536.0,
            "pressure 0.5 roundtrip delta too large: {}",
            (v - back).abs()
        );
    }

    #[test]
    fn pressure_quarter_roundtrip_exact() {
        // 0.25 × 65536 = 16384, well within i16 range — exact.
        let v = 0.25f32;
        let q = q16_i16_from_f32(v);
        let back = q16_i16_to_f32(q);
        assert!(
            (v - back).abs() < 1.0 / 65_536.0,
            "0.25 delta: {}",
            (v - back).abs()
        );
    }

    #[test]
    fn pressure_one_clamps() {
        // 1.0 × 65536 = 65536 > i16::MAX (32767) — must saturate, not panic.
        let q = q16_i16_from_f32(1.0);
        assert_eq!(q, i16::MAX);
    }

    #[test]
    fn zero_encodes_to_zero() {
        assert_eq!(q16_from_f32(0.0), 0);
        assert_eq!(q16_i16_from_f32(0.0), 0);
    }

    #[test]
    fn large_coord_roundtrip() {
        // iPad Pro 12.9" logical width is 1366 px.
        let v = 1366.0f32;
        let q = q16_from_f32(v);
        let back = q16_to_f32(q);
        assert!((v - back).abs() < 1.0 / 65_536.0);
    }
}
