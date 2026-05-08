//! VAAPI HEVC encoder (Intel and AMD on Linux).
//!
//! Uses `VAEncMiscParameterRIR` for rolling intra-refresh (spec §5.2).
//! NVIDIA proprietary driver does **not** expose this VAAPI extension even
//! though the hardware supports it — that's why NVENC is a separate impl.
//!
//! High-level flow:
//! 1. `vaGetDisplay(drm_fd)` where `drm_fd` is opened from `/dev/dri/renderD128`.
//! 2. `vaInitialize(dpy, &major, &minor)`.
//! 3. `vaCreateConfig(dpy, VAProfileHEVCMain10, VAEntrypointEncSlice, &attrs, n, &cfg_id)`.
//! 4. `vaCreateContext(dpy, cfg_id, w, h, ...)`.
//! 5. Per-frame: `vaBeginPicture` → `vaRenderPicture` (slice + misc params) →
//!    `vaEndPicture` → `vaSyncSurface` → `vaMapBuffer` (coded buffer) → copy →
//!    `vaUnmapBuffer`.
//!
//! This module is Linux-only and is a **stub** that bails at runtime.

#![cfg(feature = "vaapi")]

use crate::common::SharedConfig;
use crate::{CodecError, ColorSpace, EncodedSlice, Encoder, EncoderKind, Negotiated, PeerCaps, Profile};
use ix_display::{DamageRect, GpuFrame};

/// VAAPI HEVC encoder handle (stub).
pub struct VaapiHevc {
    cfg: SharedConfig,
    pending_keyframe: bool,
    current_kbps: u32,
}

impl VaapiHevc {
    /// Open a VAAPI HEVC encode context.
    pub fn new(cfg: SharedConfig) -> Result<Self, CodecError> {
        Err(CodecError::NotAvailable(
            "VaapiHevc: stub — install libva-dev libva-drm and build with --features vaapi".into(),
        ))
    }

    /// Probe: at least one `/dev/dri/renderD*` node exists and `libva.so.2` is present.
    #[cfg(target_os = "linux")]
    pub fn probe_linux() -> bool {
        let has_drm = std::fs::read_dir("/dev/dri")
            .map(|mut d| {
                d.any(|e| {
                    e.map(|e| e.file_name().to_string_lossy().starts_with("renderD"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        let has_va = [
            "/usr/lib/x86_64-linux-gnu/libva.so.2",
            "/usr/lib/libva.so.2",
        ]
        .iter()
        .any(|p| std::path::Path::new(p).exists());
        has_drm && has_va
    }
}

impl Encoder for VaapiHevc {
    fn kind(&self) -> EncoderKind {
        EncoderKind::VaapiHevc
    }

    fn negotiate(&mut self, peer: &PeerCaps) -> Negotiated {
        Negotiated {
            profile: Profile::HevcMain10,
            color: if peer.supports_hdr() { ColorSpace::Bt2020Pq } else { ColorSpace::Bt709Sdr },
        }
    }

    fn encode(&mut self, _src: &GpuFrame, _dirty: &[DamageRect]) -> Result<EncodedSlice, CodecError> {
        Err(CodecError::NotAvailable("VaapiHevc stub".into()))
    }

    fn force_keyframe(&mut self) {
        self.pending_keyframe = true;
    }

    fn set_bitrate(&mut self, kbps: u32) {
        self.current_kbps = kbps.clamp(self.cfg.min_bitrate_kbps, self.cfg.max_bitrate_kbps);
    }
}
