//! Video encoder trait. Real impls (NVENC, QSV, AMF, VAAPI, x264) land in Plan 5.

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CodecError {
    #[error("encoder not available")]
    Unavailable,
    #[error("encode failed: {0}")]
    Encode(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecKind {
    H264,
    Hevc,
    Av1,
}

#[async_trait]
pub trait Encoder: Send + Sync {
    fn kind(&self) -> CodecKind;
    async fn encode_frame(&mut self, _texture_handle: u64) -> Result<Vec<u8>, CodecError> {
        Err(CodecError::Unavailable)
    }
}

/// Compiles, never produces frames. Replaced by real impls in Plan 5.
pub struct NoOpEncoder;

#[async_trait]
impl Encoder for NoOpEncoder {
    fn kind(&self) -> CodecKind {
        CodecKind::Hevc
    }
}
