//! ix-codec — multi-encoder dispatch with hardware and software paths.
//!
//! # Architecture
//! - [`Encoder`] trait is the only type callers need to name.
//! - [`probe`] module probes available hardware at daemon startup.
//! - [`common`] holds the shared [`SharedConfig`] (spec §5.2 preset).
//! - Each encoder module is feature-gated; they compile to stub impls that
//!   return [`CodecError::NotAvailable`] when the vendor SDK is absent.
//!
//! # Feature flags
//! | Feature    | What it enables                                 |
//! |------------|-------------------------------------------------|
//! | `sw-only`  | `openh264` software H.264 fallback (default)    |
//! | `nvenc`    | NVIDIA NVENC HEVC + AV1 (stubs without SDK)    |
//! | `qsv`      | Intel Quick Sync via oneVPL                     |
//! | `amf`      | AMD AMF HEVC (Windows-primary)                 |
//! | `vaapi`    | VAAPI HEVC (Linux Intel/AMD)                   |
//! | `all-codecs` | all of the above                              |

// ── public trait surface ────────────────────────────────────────────────────
pub mod trait_;
pub use trait_::{
    CodecError, ColorSpace, EncodedSlice, Encoder, EncoderKind, Negotiated, PeerCaps, PeerKind,
    Profile,
};

// ── shared configuration ───────────────────────────────────────────────────
pub mod common;
pub use common::SharedConfig;

// ── runtime probe ──────────────────────────────────────────────────────────
pub mod probe;
pub use probe::probe_available_encoders;

// ── encoder implementations (feature-gated) ────────────────────────────────
#[cfg(feature = "nvenc")]
pub mod nvenc_av1;
#[cfg(feature = "nvenc")]
pub mod nvenc_hevc;

#[cfg(feature = "qsv")]
pub mod qsv_hevc;

#[cfg(feature = "amf")]
pub mod amf_hevc;

#[cfg(feature = "vaapi")]
pub mod vaapi_hevc;

#[cfg(feature = "sw-only")]
pub mod x264_sw;
