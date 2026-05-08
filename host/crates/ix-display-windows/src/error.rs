//! Error types for `ix-display-windows`.

#![cfg(windows)]

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    /// The driver is not installed or the device path cannot be opened.
    #[error("iexdd driver not installed: could not open {0}")]
    DriverNotInstalled(String),

    /// HELLO protocol version mismatch — rebuild user-mode and driver together.
    #[error("protocol version mismatch: expected {expected}, got {got}")]
    ProtocolVersion { expected: u32, got: u32 },

    /// A malformed or truncated IOCTL response was received.
    #[error("protocol error: {0}")]
    Protocol(String),

    /// Underlying Windows API error.
    #[error("windows error: {0}")]
    Windows(#[from] windows::core::Error),

    /// I/O error from the Rust standard library.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}
