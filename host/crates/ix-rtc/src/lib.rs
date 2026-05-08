//! WebRTC peer connection lifecycle, pairing, and replay protection. The
//! actual `webrtc-rs` integration lands in Plan 5; Plan 7 fills in the pairing
//! and replay-protection pieces.

#![deny(missing_docs)]

pub mod pairing;
pub mod replay;

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
