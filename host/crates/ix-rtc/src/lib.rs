//! WebRTC peer connection lifecycle, pairing, and replay protection. The
//! actual `webrtc-rs` integration lands in Plan 5; Plan 7 fills in the pairing
//! and replay-protection pieces.

#![deny(missing_docs)]

// ── Plan 7 modules (pairing, replay) ───────────────────────────────────────
/// SPAKE2-P256 pairing server + AEAD-wrapped device-cert exchange.
pub mod pairing;
/// Per-DataChannel sliding-window replay protection.
pub mod replay;

// ── Plan 5 modules (peer, signaling, channels, bitrate, heartbeat) ─────────
/// Transport-CC → encoder bitrate controller (proportional, deadband, slew).
pub mod bitrate_controller;
/// Typed input + control DataChannel wrappers.
pub mod channels;
/// 250 ms heartbeat; 4 missed = 1 s disconnect detection.
pub mod heartbeat;
/// WebRTC peer connection — owns video track and DataChannels.
pub mod peer;
/// SDP offer/answer + cert-pinning hook.
pub mod signaling;

pub use peer::Peer;

use thiserror::Error;

/// Top-level RTC errors. Pairing errors live in [`pairing::PairingError`].
#[derive(Debug, Error)]
pub enum RtcError {
    /// Peer is not in `Live` state — operation is not valid.
    #[error("not connected")]
    NotConnected,
    /// Pairing failed; see inner.
    #[error("pairing: {0}")]
    Pairing(#[from] pairing::PairingError),
    /// Remote certificate not in pinned set.
    #[error("remote certificate not in pinned set")]
    CertNotPinned,
}

/// Connection-level state machine matching spec §9. Mirrored on the iPad as
/// `IExtendSession.State`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerState {
    /// Not connected, no pairing in flight.
    Idle,
    /// SPAKE2 PAKE in flight.
    Pairing,
    /// WebRTC handshake in flight.
    Connecting,
    /// First video frame arrived, normal operation.
    Live,
    /// Degraded — see latency badge in UI.
    Degraded,
    /// Lost connection; retry-with-backoff or `Failed`.
    Disconnected,
    /// Terminal failure; manual recovery only.
    Failed,
}

/// Lifecycle handle for a single peer connection. Plan 5 fills in the actual
/// webrtc-rs `RTCPeerConnection`; this struct tracks the agreed state machine.
pub struct PeerConnection {
    state: PeerState,
}

impl PeerConnection {
    /// New idle peer connection.
    pub fn new() -> Self {
        Self {
            state: PeerState::Idle,
        }
    }
    /// Current state.
    pub fn state(&self) -> PeerState {
        self.state
    }
    /// Force-set the state (used by Plan 5 transition logic + tests).
    pub fn set_state(&mut self, s: PeerState) {
        self.state = s;
    }
}

impl Default for PeerConnection {
    fn default() -> Self {
        Self::new()
    }
}
