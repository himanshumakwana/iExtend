//! Public encoder trait + value types.
//!
//! Every encoder impl in this crate implements [`Encoder`]. Callers should only
//! depend on this module — they never need to name a concrete encoder type.

use ix_display::{DamageRect, GpuFrame};
use thiserror::Error;

/// Errors that any encoder can surface.
#[derive(Debug, Error)]
pub enum CodecError {
    /// The encoder is not available on this host (GPU absent, SDK not installed,
    /// or the feature was not compiled in).
    #[error("encoder not available on this host: {0}")]
    NotAvailable(String),

    /// Initialization sequence failed (e.g. session open returned an SDK error).
    #[error("encoder initialization failed: {0}")]
    Init(String),

    /// A per-frame encode call failed.
    #[error("encode failed: {0}")]
    EncodeFailed(String),

    /// Requested bitrate is outside the encoder's allowed range.
    #[error(
        "set_bitrate out of range: requested {requested_kbps} kbps, \
         allowed {min_kbps}–{max_kbps} kbps"
    )]
    BitrateOutOfRange {
        requested_kbps: u32,
        min_kbps: u32,
        max_kbps: u32,
    },
}

/// Identifies which encoder implementation is in use.
///
/// Used for telemetry, logging, and the codec-preference selection in
/// [`crate::probe`].
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum EncoderKind {
    /// NVIDIA NVENC — AV1 (RTX 40-series, M4-iPad gated).
    NvencAv1,
    /// NVIDIA NVENC — HEVC Main10.
    NvencHevc,
    /// Intel Quick Sync via oneVPL — HEVC Main10.
    QsvHevc,
    /// AMD AMF — HEVC Main10.
    AmfHevc,
    /// VAAPI (Intel/AMD on Linux) — HEVC Main10.
    VaapiHevc,
    /// x264/openh264 software — H.264 ultralow-latency. Battery-drain warning.
    X264SoftwareUlllSw,
}

impl EncoderKind {
    /// Preference ordering: lower = better. NVENC AV1 wins when peer supports it.
    pub fn priority(self) -> u8 {
        match self {
            EncoderKind::NvencAv1 => 0,
            EncoderKind::NvencHevc | EncoderKind::QsvHevc | EncoderKind::AmfHevc => 1,
            EncoderKind::VaapiHevc => 2,
            EncoderKind::X264SoftwareUlllSw => 99,
        }
    }

    /// `true` for the software-only fallback path.
    pub fn is_software(self) -> bool {
        matches!(self, EncoderKind::X264SoftwareUlllSw)
    }
}

/// Negotiated codec profile.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Profile {
    /// HEVC Main10 — HDR or SDR depending on [`ColorSpace`].
    HevcMain10,
    /// AV1 Main10 — only offered to M4 iPad.
    Av1Main10,
    /// H.264 ultralow-latency — software fallback only.
    H264UlllFallback,
}

/// Colour-space and transfer-characteristics for the negotiated stream.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ColorSpace {
    /// HDR10 — BT.2020 PQ (SMPTE ST 2084).
    Bt2020Pq,
    /// SDR — BT.709.
    Bt709Sdr,
}

/// Peer device class, decoded from the SDP User-Agent string.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum PeerKind {
    /// iPad Pro M4 — supports AV1 decode.
    IpadProM4,
    /// iPad Pro M2 or M1 — HEVC only.
    IpadProM2OrM1,
    /// iPad Air Mx — HEVC only.
    IpadAirM,
    /// iPad (A-series, base model) — H.264 fallback.
    IpadAseries,
    /// Unknown / unidentified peer.
    Unknown,
}

/// Decode capabilities announced by the remote peer during SDP exchange.
#[derive(Debug, Clone)]
pub struct PeerCaps {
    /// Peer can decode AV1 (RTX-class; M4 iPad only on the tablet side).
    pub av1_decode: bool,
    /// Peer can decode HEVC (all current iPads).
    pub hevc_decode: bool,
    /// Maximum frame size the peer advertises.
    pub max_resolution: (u32, u32),
    /// Device classification (determines AV1 gating, HDR support, etc.).
    pub peer_kind: PeerKind,
}

impl PeerCaps {
    /// True when this peer class supports HDR10 display.
    pub fn supports_hdr(&self) -> bool {
        matches!(
            self.peer_kind,
            PeerKind::IpadProM4 | PeerKind::IpadProM2OrM1 | PeerKind::IpadAirM
        )
    }
}

/// What the two sides agreed on after SDP negotiation.
#[derive(Debug, Clone)]
pub struct Negotiated {
    /// Agreed codec profile.
    pub profile: Profile,
    /// Agreed colour-space / transfer characteristics.
    pub color: ColorSpace,
}

/// One encoded video slice.
///
/// For HEVC this is raw Annex-B NAL units (SPS/PPS prepended on IDR slices).
/// For AV1 this is a sequence of OBUs. The RTP packetizer in `ix-rtc` handles
/// RFC 7798 / RFC 9335 framing.
#[derive(Debug, Clone)]
pub struct EncodedSlice {
    /// Raw bitstream bytes.
    pub data: Vec<u8>,
    /// True if this slice begins a new intra-refresh cycle (IDR/keyframe).
    pub is_keyframe: bool,
    /// Presentation timestamp in microseconds from session start.
    pub pts_us: i64,
    /// Intra-refresh slice index (0..N); useful for gap-detection telemetry.
    pub slice_index: u32,
}

/// The single encoder trait all impls must satisfy.
///
/// All methods take `&mut self` — the encoder is single-threaded and owns its
/// session. `Session` wraps it in an `Arc<Mutex<Box<dyn Encoder>>>` for sharing
/// between the encode pump and the bitrate-controller tick.
pub trait Encoder: Send {
    /// Which encoder variant this is.
    fn kind(&self) -> EncoderKind;

    /// Intersect the encoder's capabilities with the peer's announced decode
    /// caps and return what was agreed. Called once per session, before the
    /// first [`encode`][Self::encode] call.
    fn negotiate(&mut self, peer: &PeerCaps) -> Negotiated;

    /// Encode one frame. `dirty` is the set of damage rectangles reported by
    /// the capture backend; encoders may use them for ROI/priority hints.
    fn encode(&mut self, src: &GpuFrame, dirty: &[DamageRect]) -> Result<EncodedSlice, CodecError>;

    /// Request an intra-refresh reset (IDR) on the next [`encode`][Self::encode]
    /// call. One-shot: only the immediately following call is affected.
    fn force_keyframe(&mut self);

    /// Update the encoder's target bitrate. Silently clamps to
    /// `[min_bitrate_kbps, max_bitrate_kbps]` from `SharedConfig`; does not
    /// error on out-of-range values (the bitrate controller already clamps).
    fn set_bitrate(&mut self, kbps: u32);
}
