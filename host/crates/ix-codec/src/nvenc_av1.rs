//! NVENC AV1 encoder (NVIDIA RTX 40-series / Ada Lovelace and newer).
//!
//! Structurally identical to [`crate::nvenc_hevc`]; the differences are:
//! - `encodeGUID = NV_ENC_CODEC_AV1_GUID` (errors on older cards → probe gate).
//! - `av1Config` struct instead of `hevcConfig`.
//! - Output is AV1 OBUs (RFC 9335 packetization handled by `ix-rtc::video_track`).
//!
//! Only offered to M4-iPad peers (see [`crate::probe::ProbeOutcome::candidates_for`]).

#![cfg(feature = "nvenc")]

use crate::common::SharedConfig;
use crate::{
    CodecError, ColorSpace, EncodedSlice, Encoder, EncoderKind, Negotiated, PeerCaps, Profile,
};
use ix_display::{DamageRect, GpuFrame};

/// NVENC AV1 encoder handle (stub — requires RTX 40-series and NVENC SDK).
pub struct NvencAv1 {
    cfg: SharedConfig,
    pending_keyframe: bool,
    next_slice: u32,
    current_kbps: u32,
}

impl NvencAv1 {
    /// Open an NVENC AV1 encode session.
    ///
    /// Returns [`CodecError::NotAvailable`] on cards that don't support
    /// `NV_ENC_CODEC_AV1_GUID` (pre-Ada). The probe should prevent this from
    /// being called on unsupported hardware, but the runtime check is here as
    /// a safety net.
    pub fn new(cfg: SharedConfig) -> Result<Self, CodecError> {
        Err(CodecError::NotAvailable(
            "NvencAv1: stub implementation — requires RTX 40-series and NVENC SDK".into(),
        ))
    }

    /// Probe AV1 capability: attempt `NvEncOpenEncodeSessionEx` + query
    /// `NV_ENC_CODEC_AV1_GUID` support. Returns false if the card is pre-Ada.
    pub fn probe_av1_capability() -> bool {
        false // stub
    }
}

impl Encoder for NvencAv1 {
    fn kind(&self) -> EncoderKind {
        EncoderKind::NvencAv1
    }

    fn negotiate(&mut self, peer: &PeerCaps) -> Negotiated {
        Negotiated {
            profile: Profile::Av1Main10,
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
        Err(CodecError::NotAvailable("NvencAv1 stub".into()))
    }

    fn force_keyframe(&mut self) {
        self.pending_keyframe = true;
    }

    fn set_bitrate(&mut self, kbps: u32) {
        self.current_kbps = kbps.clamp(self.cfg.min_bitrate_kbps, self.cfg.max_bitrate_kbps);
    }
}
