//! WebRTC peer connection lifecycle.
//!
//! [`Peer`] is the top-level handle for a single WebRTC session. It owns the
//! simulated peer-connection state, the video track stub, both DataChannels,
//! and the bitrate controller. Construction is async because in the real
//! implementation it initialises the ICE agent and creates DataChannels.
//!
//! This is a **scaffold** that stubs out the webrtc-rs types so that
//! `iextendd::session` and `smoke_loopback` can compile and run without the
//! full WebRTC stack. When `webrtc` crate integration lands (Plan 6), this
//! file gains real `RTCPeerConnection` fields and the scaffold methods become
//! real implementations.

use crate::bitrate_controller::BitrateController;
use crate::channels::{ControlChannel, InputChannel};
use crate::heartbeat::Heartbeat;
use crate::RtcError;
use ix_codec::{EncodedSlice, Negotiated};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tracing::debug;

/// Connection state machine — mirrors spec §9 and Plan 7's `PeerState`.
/// Re-exported here so callers don't need to import `ix_rtc::lib` directly.
pub use crate::PeerState;

/// Video sink: receives encoded slices and delivers them to the network.
///
/// In the real implementation this wraps a `TrackLocalStaticSample` from
/// webrtc-rs. For the scaffold it's an in-process loopback queue.
pub struct VideoSink {
    tx: mpsc::UnboundedSender<EncodedSlice>,
    /// For the loopback test: subscribe to slices arriving at this peer.
    pub rx: Arc<Mutex<mpsc::UnboundedReceiver<EncodedSlice>>>,
}

impl VideoSink {
    fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            tx,
            rx: Arc::new(Mutex::new(rx)),
        }
    }

    /// Push an encoded slice into the video track. In the real impl this
    /// calls `TrackLocalStaticSample::write_sample`.
    pub async fn write_slice(&self, slice: &EncodedSlice) -> Result<(), RtcError> {
        self.tx
            .send(slice.clone())
            .map_err(|_| RtcError::NotConnected)?;
        Ok(())
    }
}

/// The top-level peer connection handle.
///
/// Create via [`PeerBuilder::build`] (obtained from [`Peer::builder`]).
pub struct Peer {
    state: Arc<Mutex<PeerState>>,
    /// Video track: push encoded slices here.
    pub video: VideoSink,
    /// Input DataChannel (unreliable, unordered).
    pub input: InputChannel,
    /// Control DataChannel (reliable, ordered).
    pub control: ControlChannel,
    /// Bitrate controller (transport-CC feedback → encoder target).
    pub bitrate: Arc<Mutex<BitrateController>>,
    /// Heartbeat state machine.
    pub heartbeat: Arc<Mutex<Heartbeat>>,
}

impl Peer {
    /// Create a builder.
    pub fn builder() -> PeerBuilder {
        PeerBuilder::new()
    }

    /// True if the peer connection is in the initial `Idle` state.
    pub async fn is_idle(&self) -> bool {
        *self.state.lock().await == PeerState::Idle
    }

    /// Current state.
    pub async fn state(&self) -> PeerState {
        *self.state.lock().await
    }

    /// Transition to a new state (used by session + tests).
    pub async fn set_state(&self, s: PeerState) {
        *self.state.lock().await = s;
    }

    /// Apply the result of SDP negotiation to this peer.
    pub async fn apply_negotiated(&self, _negotiated: Negotiated) {
        // In the real impl: configure the RTP sender's codec parameters.
        // Scaffold: no-op.
    }

    /// Graceful shutdown: drain any queued slices, close DataChannels.
    pub async fn shutdown(self) {
        let mut state = self.state.lock().await;
        *state = PeerState::Disconnected;
        debug!("peer shutdown");
    }
}

/// Builds a [`Peer`] with sensible defaults.
#[derive(Default)]
pub struct PeerBuilder {
    initial_bitrate_kbps: u32,
    min_bitrate_kbps: u32,
    max_bitrate_kbps: u32,
}

impl PeerBuilder {
    fn new() -> Self {
        Self {
            initial_bitrate_kbps: 25_000,
            min_bitrate_kbps: 6_000,
            max_bitrate_kbps: 80_000,
        }
    }

    /// Override initial bitrate (kbps).
    pub fn initial_bitrate(mut self, kbps: u32) -> Self {
        self.initial_bitrate_kbps = kbps;
        self
    }

    /// Build the peer. Async because the real webrtc-rs init path is async.
    pub async fn build(self) -> Result<Peer, RtcError> {
        let bc = BitrateController::new(
            self.initial_bitrate_kbps,
            self.min_bitrate_kbps,
            self.max_bitrate_kbps,
        );
        let hb = Heartbeat::new(Duration::from_millis(250));

        Ok(Peer {
            state: Arc::new(Mutex::new(PeerState::Idle)),
            video: VideoSink::new(),
            input: InputChannel::new(),
            control: ControlChannel::new(),
            bitrate: Arc::new(Mutex::new(bc)),
            heartbeat: Arc::new(Mutex::new(hb)),
        })
    }
}
