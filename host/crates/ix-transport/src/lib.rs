//! Platform-abstracted localhost endpoint for daemon ↔ tray IPC.
//! Real impls land in Task 6 of this plan.

use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Platform-specific localhost endpoint name.
///   Linux/macOS: a filesystem path (UDS).
///   Windows:     a named-pipe name like `\\.\pipe\iextendd`.
#[derive(Debug, Clone)]
pub struct LocalEndpoint(pub String);

impl LocalEndpoint {
    pub fn default_for_user() -> Self {
        #[cfg(windows)]
        {
            Self(r"\\.\pipe\iextendd".to_string())
        }
        #[cfg(unix)]
        {
            let runtime = std::env::var("XDG_RUNTIME_DIR")
                .unwrap_or_else(|_| "/tmp".to_string());
            Self(format!("{runtime}/iextendd.sock"))
        }
    }

    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }
}
