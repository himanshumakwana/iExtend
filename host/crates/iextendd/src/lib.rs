//! iextendd library interface.
//!
//! Exposes public modules to examples and integration tests.
//! The binary entry point is in `main.rs`.

pub mod grpc_server;
pub mod keystore;
pub mod pair_listener;
pub mod session;
pub mod usb_listener;

// Re-export DaemonState for tests / external callers.
pub use grpc_server::DaemonState;
