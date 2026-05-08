//! WebRTC peer connection lifecycle. Real impl in Plan 5 (likely webrtc-rs 0.x).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RtcError {
    #[error("not connected")]
    NotConnected,
}

#[derive(Debug, Clone, Copy)]
pub enum PeerState {
    Idle,
    Negotiating,
    Live,
    Failed,
}

pub struct PeerConnection {
    state: PeerState,
}

impl PeerConnection {
    pub fn new() -> Self {
        Self {
            state: PeerState::Idle,
        }
    }
    pub fn state(&self) -> PeerState {
        self.state
    }
}

impl Default for PeerConnection {
    fn default() -> Self {
        Self::new()
    }
}
