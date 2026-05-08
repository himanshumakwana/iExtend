//! Intel Quick Sync HEVC encoder via oneVPL.
//!
//! Three-stage init dance:
//! 1. `MFXLoad()` → `mfxLoader`.
//! 2. `MFXCreateSession(loader, 0, &session)`.
//! 3. `mfxVideoParam` with HEVC + Main10 + intra-refresh + ultralowlatency,
//!    then `MFXVideoENCODE_Init(session, &params)`.
//!
//! Intra-refresh in oneVPL: `mfxExtCodingOption2.IntRefType = MFX_REFRESH_HORIZONTAL`,
//! `IntRefCycleSize = cfg.intra_refresh_rows`.
//!
//! This file is a **stub** that bails with [`CodecError::NotAvailable`] at
//! runtime. When `libvpl-dev` is installed and the feature is enabled, replace
//! the bodies with real oneVPL FFI calls.

#![cfg(feature = "qsv")]

use crate::common::SharedConfig;
use crate::{CodecError, ColorSpace, EncodedSlice, Encoder, EncoderKind, Negotiated, PeerCaps, Profile};
use ix_display::{DamageRect, GpuFrame};

/// Intel Quick Sync HEVC encoder handle (stub).
pub struct QsvHevc {
    cfg: SharedConfig,
    pending_keyframe: bool,
    current_kbps: u32,
}

impl QsvHevc {
    /// Initialise a oneVPL HEVC session.
    pub fn new(cfg: SharedConfig) -> Result<Self, CodecError> {
        Err(CodecError::NotAvailable(
            "QsvHevc: stub — install libvpl-dev and build with --features qsv".into(),
        ))
    }

    /// Check if `libvpl.so.2` or `libmfx.so` is present.
    pub fn probe() -> bool {
        #[cfg(target_os = "linux")]
        {
            ["/usr/lib/x86_64-linux-gnu/libvpl.so.2", "/usr/lib/libvpl.so.2"]
                .iter()
                .any(|p| std::path::Path::new(p).exists())
        }
        #[cfg(not(target_os = "linux"))]
        false
    }
}

impl Encoder for QsvHevc {
    fn kind(&self) -> EncoderKind {
        EncoderKind::QsvHevc
    }

    fn negotiate(&mut self, peer: &PeerCaps) -> Negotiated {
        Negotiated {
            profile: Profile::HevcMain10,
            color: if peer.supports_hdr() { ColorSpace::Bt2020Pq } else { ColorSpace::Bt709Sdr },
        }
    }

    fn encode(&mut self, _src: &GpuFrame, _dirty: &[DamageRect]) -> Result<EncodedSlice, CodecError> {
        Err(CodecError::NotAvailable("QsvHevc stub".into()))
    }

    fn force_keyframe(&mut self) {
        self.pending_keyframe = true;
    }

    fn set_bitrate(&mut self, kbps: u32) {
        self.current_kbps = kbps.clamp(self.cfg.min_bitrate_kbps, self.cfg.max_bitrate_kbps);
    }
}
