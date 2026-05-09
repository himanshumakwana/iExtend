//! Shared encoder configuration derived from spec §5.2.
//!
//! Each encoder impl receives a [`SharedConfig`] at construction time and
//! translates it to vendor-specific parameters. This struct is the single
//! source of truth for the "ultralowlatency + 16-row intra-refresh" preset.

use crate::{ColorSpace, Profile};

/// Encoder configuration shared across all impls.
///
/// All numeric fields are in the units shown by the field name (kbps, rows,
/// etc.) so that unit-conversion bugs stay in the per-vendor translation code,
/// not here.
#[derive(Debug, Clone)]
pub struct SharedConfig {
    /// Encoded frame width in pixels.
    pub width: u32,
    /// Encoded frame height in pixels.
    pub height: u32,
    /// Frame-rate numerator (denominator is always 1 in our use).
    pub fps_num: u32,
    /// Frame-rate denominator.
    pub fps_den: u32,
    /// Starting target bitrate (kbps). The bitrate controller will update this
    /// after the first transport-CC round-trip.
    pub initial_bitrate_kbps: u32,
    /// Absolute floor (kbps). Per spec §5.2: 6 000 kbps for 1080p120.
    pub min_bitrate_kbps: u32,
    /// Absolute ceiling (kbps). Per spec §5.2: 80 000 kbps.
    pub max_bitrate_kbps: u32,
    /// Negotiated codec profile.
    pub profile: Profile,
    /// Negotiated colour-space.
    pub color: ColorSpace,
    /// Rolling intra-refresh row count (spec §5.2 "16-row slice gradient").
    /// Zero disables intra-refresh and falls back to periodic full keyframes.
    pub intra_refresh_rows: u32,
}

impl SharedConfig {
    /// Default for the virtual 1080p/1200p monitor at 120 fps. This is what
    /// `iextendd` uses at startup before the peer announces its resolution.
    pub fn default_1080p120() -> Self {
        Self {
            width: 1920,
            height: 1200, // virtual monitor is 16:10 (1920×1200)
            fps_num: 120,
            fps_den: 1,
            initial_bitrate_kbps: 25_000,
            min_bitrate_kbps: 6_000,
            max_bitrate_kbps: 80_000,
            profile: Profile::HevcMain10,
            color: ColorSpace::Bt2020Pq,
            intra_refresh_rows: 16,
        }
    }

    /// Return a clone tuned for 60 fps (lower ceiling, lower floor). Used
    /// when the peer negotiates down to 60 fps or the bitrate controller
    /// detects sustained congestion that can't be solved by bitrate alone.
    pub fn at_60fps(mut self) -> Self {
        self.fps_num = 60;
        self.min_bitrate_kbps = 4_000;
        self
    }

    /// Number of slices in one full intra-refresh cycle: `ceil(height / (intra_refresh_rows * 16))`.
    ///
    /// Each macroblock row is 16 pixels tall. One refresh cycle covers
    /// `intra_refresh_rows` macroblock-rows per frame, so the cycle length is
    /// `ceil(height / (intra_refresh_rows * 16))` frames.
    pub fn intra_refresh_period(&self) -> u32 {
        let mb_rows = self.height.div_ceil(16);
        mb_rows.div_ceil(self.intra_refresh_rows)
    }

    /// AMF measures intra-refresh in macroblocks-per-slot. Conversion:
    /// `mbs_per_slot = ceil(width/16) * intra_refresh_rows`.
    pub fn amf_mbs_per_slot(&self) -> u32 {
        let mb_cols = self.width.div_ceil(16);
        mb_cols * self.intra_refresh_rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intra_refresh_period_1080p_16rows() {
        let cfg = SharedConfig::default_1080p120();
        // 1200 px / 16 px-per-mb = 75 mb-rows; 75 / 16 rows-per-frame = 4.7 → 5 frames
        assert_eq!(cfg.intra_refresh_period(), 5);
    }

    #[test]
    fn amf_mbs_per_slot_1080p_16rows() {
        let cfg = SharedConfig::default_1080p120();
        // 1920/16 = 120 mb-cols; * 16 refresh-rows = 1920 mbs/slot
        assert_eq!(cfg.amf_mbs_per_slot(), 1920);
    }
}
