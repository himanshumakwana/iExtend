//! IddCx virtual monitor + DXGI Desktop Duplication capture. Real impl in Plan 3.
//! On non-Windows targets this crate compiles to an empty module so the workspace
//! still builds; consumers gate use behind `cfg(windows)`.

#![cfg(windows)]

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DisplayWindowsError {
    #[error("IddCx unavailable")]
    Unavailable,
}
