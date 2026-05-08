//! AMD AMF HEVC encoder.
//!
//! AMF measures intra-refresh in macroblocks-per-slot rather than rows:
//! `AMF_VIDEO_ENCODER_HEVC_INTRA_REFRESH_NUM_MBS_PER_SLOT`.
//!
//! Conversion from the spec's "16-row gradient":
//! `mbs_per_slot = ceil(width/16) * intra_refresh_rows`
//! e.g. for 1920×1200 @ 16 rows: 120 MBs/col × 16 = 1920 MBs/slot.
//!
//! See [`crate::common::SharedConfig::amf_mbs_per_slot`].
//!
//! This module is Windows-primary; on Linux AMF is not available and the
//! probe returns `false`. Use [`crate::vaapi_hevc`] on Linux AMD GPUs.

#![cfg(feature = "amf")]

use crate::common::SharedConfig;
use crate::{
    CodecError, ColorSpace, EncodedSlice, Encoder, EncoderKind, Negotiated, PeerCaps, Profile,
};
use ix_display::{DamageRect, GpuFrame};

/// AMD AMF HEVC encoder handle (stub).
pub struct AmfHevc {
    cfg: SharedConfig,
    pending_keyframe: bool,
    current_kbps: u32,
}

impl AmfHevc {
    /// Open an AMF HEVC encode context.
    pub fn new(cfg: SharedConfig) -> Result<Self, CodecError> {
        Err(CodecError::NotAvailable(
            "AmfHevc: stub — set AMF_HOME and build with --features amf".into(),
        ))
    }

    /// Probe: `amfrt64.dll` must be loadable on Windows.
    #[cfg(target_os = "windows")]
    pub fn probe_windows() -> bool {
        false // stub
    }
}

impl Encoder for AmfHevc {
    fn kind(&self) -> EncoderKind {
        EncoderKind::AmfHevc
    }

    fn negotiate(&mut self, peer: &PeerCaps) -> Negotiated {
        Negotiated {
            profile: Profile::HevcMain10,
            color: if peer.supports_hdr() {
                ColorSpace::Bt2020Pq
            } else {
                ColorSpace::Bt709Sdr
            },
        }
    }

    fn encode(
        &mut self,
        _src: &GpuFrame,
        _dirty: &[DamageRect],
    ) -> Result<EncodedSlice, CodecError> {
        Err(CodecError::NotAvailable("AmfHevc stub".into()))
    }

    fn force_keyframe(&mut self) {
        self.pending_keyframe = true;
    }

    fn set_bitrate(&mut self, kbps: u32) {
        self.current_kbps = kbps.clamp(self.cfg.min_bitrate_kbps, self.cfg.max_bitrate_kbps);
    }
}
