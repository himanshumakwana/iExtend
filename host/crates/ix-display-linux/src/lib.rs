//! evdi virtual monitor + PipeWire / XDamage capture. Real impl in Plan 4.

#![cfg(unix)]

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DisplayLinuxError {
    #[error("evdi module not loaded")]
    EvdiUnavailable,
}
