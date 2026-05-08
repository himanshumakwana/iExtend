//! NVENC HEVC encoder (NVIDIA Video Codec SDK 12.2+).
//!
//! This module is only compiled when `--features nvenc` is passed. On builds
//! without that feature the entire file is excluded from compilation, so there
//! are no link requirements for normal (software-only) development.
//!
//! ## Compile-time gate
//! ```toml
//! [features]
//! nvenc = []
//! ```
//!
//! ## Runtime bail-out
//! [`NvencHevc::new`] returns [`CodecError::NotAvailable`] immediately if the
//! NVIDIA encode library is not present on the host at runtime. The daemon
//! never calls `new` without first confirming availability via
//! [`crate::probe::Probe::detect`].

#![cfg(feature = "nvenc")]

use crate::common::SharedConfig;
use crate::{
    CodecError, ColorSpace, EncodedSlice, Encoder, EncoderKind, Negotiated, PeerCaps, Profile,
};
use ix_display::{DamageRect, GpuFrame};
use std::time::Instant;

/// NVENC HEVC encoder handle.
///
/// This is a **stub** implementation for Linux boxes without the NVIDIA GPU
/// SDK. Every public method compiles successfully and returns
/// [`CodecError::NotAvailable`] at runtime. When the feature is enabled on a
/// host with the real SDK, replace the `unimplemented!` bodies with the actual
/// FFI calls documented in the comments.
pub struct NvencHevc {
    cfg: SharedConfig,
    _origin: Instant,
    pending_keyframe: bool,
    next_slice: u32,
    current_kbps: u32,
}

impl NvencHevc {
    /// Open an NVENC HEVC encode session.
    ///
    /// # Real implementation sketch
    /// 1. Load `nvEncodeAPI64.dll` / `libnvidia-encode.so.1`.
    /// 2. `NvEncOpenEncodeSessionEx` with `deviceType = CUDA` (Linux) or
    ///    `DIRECTX` (Windows).
    /// 3. `NvEncInitializeEncoder` with:
    ///    - `encodeGUID = NV_ENC_CODEC_HEVC_GUID`
    ///    - `presetGUID = NV_ENC_PRESET_LOW_LATENCY_HQ_GUID`
    ///    - `tuningInfo = NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY`
    ///    - `gopLength = NVENC_INFINITE_GOPLENGTH`, `frameIntervalP = 1`
    ///    - `rcParams.rateControlMode = NV_ENC_PARAMS_RC_CBR_HQ`
    ///    - `hevcConfig.intraRefreshPeriod = cfg.intra_refresh_period()`
    ///    - `hevcConfig.intraRefreshCnt = cfg.intra_refresh_rows`
    ///    - `hevcConfig.pixelBitDepthMinus8 = 2` (Main10)
    ///    - HDR colour primaries (BT.2020 PQ)
    pub fn new(cfg: SharedConfig) -> Result<Self, CodecError> {
        // Runtime bail-out: NVENC is feature-gated but the SDK may not be
        // installed even when the feature is enabled.
        Err(CodecError::NotAvailable(
            "NvencHevc: stub implementation — link against NVENC SDK to enable".into(),
        ))
    }

    /// Check if the NVIDIA encode library exists on this host.
    #[cfg(target_os = "linux")]
    pub fn probe_linux() -> bool {
        std::path::Path::new("/usr/lib/x86_64-linux-gnu/libnvidia-encode.so.1").exists()
    }

    #[cfg(target_os = "windows")]
    pub fn probe_windows() -> bool {
        false // stub — real impl uses LoadLibraryExA
    }
}

impl Encoder for NvencHevc {
    fn kind(&self) -> EncoderKind {
        EncoderKind::NvencHevc
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
        Err(CodecError::NotAvailable("NvencHevc stub".into()))
    }

    fn force_keyframe(&mut self) {
        self.pending_keyframe = true;
    }

    fn set_bitrate(&mut self, kbps: u32) {
        let kbps = kbps.clamp(self.cfg.min_bitrate_kbps, self.cfg.max_bitrate_kbps);
        // Real impl: NvEncReconfigureEncoder with updated rcParams.averageBitRate.
        self.current_kbps = kbps;
    }
}
