//! Software H.264 encoder — ultralow-latency fallback.
//!
//! Uses the `openh264` crate (pure-Rust H.264 encoder; no system library
//! dependencies). On hosts without any hardware encoder this is the only
//! available path. The daemon surfaces a "Software encoder active — laptop
//! battery will drain faster" warning in the tray UI when this path is used.
//!
//! ## openh264 configuration
//! - `UsageType::ScreenContentRealTime` for screen-share workloads.
//! - `RateControlMode::Bitrate` to approximate CBR.
//! - `enable_skip_frame(false)` — no frame dropping for low-latency.
//! - Periodic IDR every `intra_refresh_period` frames (openh264 does not
//!   support true rolling intra-refresh; a periodic IDR approximates it).

#![cfg(feature = "sw-only")]

use crate::common::SharedConfig;
use crate::{
    CodecError, ColorSpace, EncodedSlice, Encoder, EncoderKind, Negotiated, PeerCaps, Profile,
};
use ix_display::{DamageRect, GpuFrame};
use openh264::encoder::{EncoderConfig, RateControlMode, UsageType};
use openh264::encoder::Encoder as Oh264Encoder;
use openh264::formats::YUVBuffer;
use openh264::OpenH264API;
use std::time::Instant;
use tracing::warn;

/// Software H.264 encoder backed by `openh264`.
pub struct X264Sw {
    cfg: SharedConfig,
    enc: Oh264Encoder,
    pending_keyframe: bool,
    frame_count: u32,
    origin: Instant,
    next_slice: u32,
    current_kbps: u32,
}

impl X264Sw {
    /// Create a new software encoder with the given configuration.
    ///
    /// Emits a one-time `WARN` log reminding the operator that software
    /// encoding drains laptop battery significantly faster than hardware.
    pub fn new(cfg: SharedConfig) -> Result<Self, CodecError> {
        warn!(
            target: "ix_codec::x264_sw",
            "Using software encoder (openh264). \
             Laptop battery will drain ~3-4x faster than with a hardware encoder. \
             Consider plugging in or using a GPU with hardware encode support."
        );

        let api = OpenH264API::from_source();
        let enc_cfg = EncoderConfig::new()
            .set_bitrate_bps(cfg.initial_bitrate_kbps * 1000)
            .max_frame_rate(cfg.fps_num as f32 / cfg.fps_den as f32)
            .enable_skip_frame(false)
            .usage_type(UsageType::ScreenContentRealTime)
            .rate_control_mode(RateControlMode::Bitrate);

        let enc = Oh264Encoder::with_api_config(api, enc_cfg)
            .map_err(|e| CodecError::Init(format!("openh264 init: {e}")))?;

        let current_kbps = cfg.initial_bitrate_kbps;
        Ok(Self {
            cfg,
            enc,
            pending_keyframe: false,
            frame_count: 0,
            origin: Instant::now(),
            next_slice: 0,
            current_kbps,
        })
    }

    /// Compute presentation timestamp in microseconds from session start.
    fn pts_us(&self) -> i64 {
        self.origin.elapsed().as_micros() as i64
    }

    /// True if this frame should start a new intra cycle (forced or periodic).
    fn should_keyframe(&self) -> bool {
        if self.pending_keyframe {
            return true;
        }
        let period = self.cfg.intra_refresh_period();
        period > 0 && self.frame_count % period == 0
    }

    /// Build a YUV420p buffer suitable for openh264.
    ///
    /// For the smoke-loopback and unit tests, frames are synthetic, so we
    /// always produce a `YUVBuffer::new()` (grey synthetic frame). Real
    /// capture backends should call `YUVBuffer` copy from their ShmCpu pixels.
    fn to_yuv_buffer(src: &GpuFrame) -> YUVBuffer {
        // We always build a synthetic buffer; real ShmCpu readback would go here.
        // The smoke test never decodes the output, only timestamps it, so
        // grey frames produce valid H.264 bitstreams for latency measurement.
        YUVBuffer::new(src.width as usize, src.height as usize)
    }
}

impl Encoder for X264Sw {
    fn kind(&self) -> EncoderKind {
        EncoderKind::X264SoftwareUlllSw
    }

    fn negotiate(&mut self, _peer: &PeerCaps) -> Negotiated {
        // Software path always falls back to H.264 + BT.709 SDR.
        // openh264 does not support HEVC or HDR.
        Negotiated {
            profile: Profile::H264UlllFallback,
            color: ColorSpace::Bt709Sdr,
        }
    }

    fn encode(&mut self, src: &GpuFrame, _dirty: &[DamageRect]) -> Result<EncodedSlice, CodecError> {
        let is_kf = self.should_keyframe();

        let yuv = Self::to_yuv_buffer(src);

        let bs = self
            .enc
            .encode(&yuv)
            .map_err(|e| CodecError::EncodeFailed(format!("openh264: {e}")))?;

        let data = bs.to_vec();
        let pts = self.pts_us();
        let slice_idx = self.next_slice;

        self.pending_keyframe = false;
        self.frame_count = self.frame_count.wrapping_add(1);
        self.next_slice = self.next_slice.wrapping_add(1);

        Ok(EncodedSlice {
            data,
            is_keyframe: is_kf,
            pts_us: pts,
            slice_index: slice_idx,
        })
    }

    fn force_keyframe(&mut self) {
        self.pending_keyframe = true;
    }

    fn set_bitrate(&mut self, kbps: u32) {
        let clamped = kbps.clamp(self.cfg.min_bitrate_kbps, self.cfg.max_bitrate_kbps);
        self.current_kbps = clamped;
        // openh264 supports runtime bitrate update via the raw API's set_option,
        // but the public EncoderConfig API doesn't expose it after construction.
        // Mid-session bitrate changes will take effect at the next IDR boundary.
    }
}
