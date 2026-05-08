//! SDP offer/answer signaling state machine with a cert-pinning hook.
//!
//! LAN-only: SDP swap travels through the local mDNS service record (Plan 7
//! owns mDNS discovery). This module just exposes the stateless helpers used
//! by `iextendd::session` to complete the SDP handshake:
//! - [`Signaling::create_offer`]
//! - [`Signaling::apply_answer`]
//! - [`Signaling::create_answer_for`]
//!
//! The cert-pinning hook is a callback the daemon supplies — it's where
//! `ix-discover` (Plan 7) plugs in once pairing is complete. For the
//! smoke-loopback test the hook is `|_| true` (accept everything).
//!
//! In the real webrtc-rs integration this wraps `RTCPeerConnection::create_offer`
//! and friends. The scaffold emits JSON blobs that represent SDP-like payloads
//! so that the session state machine can be tested without a real ICE stack.

use crate::RtcError;
use serde::{Deserialize, Serialize};
use tracing::debug;

/// A simplified SDP description. In the real integration this is
/// `webrtc::peer_connection::sdp::session_description::RTCSessionDescription`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDescription {
    /// `"offer"` or `"answer"`.
    pub sdp_type: String,
    /// The SDP body (or a JSON substitute in the scaffold).
    pub sdp: String,
}

/// Signaling state machine.
///
/// The `pin_check` callback receives the DER-encoded certificate of the
/// remote peer and returns `true` if the cert is in the pinned set. The
/// daemon initialises this from the keystore built in Plan 7.
pub struct Signaling {
    pin_check: Box<dyn Fn(&[u8]) -> bool + Send + Sync>,
}

impl Signaling {
    /// Create a new `Signaling` instance with the provided cert-pin checker.
    ///
    /// Pass `|_| true` in tests and during the smoke-loopback benchmark.
    pub fn new(pin_check: impl Fn(&[u8]) -> bool + Send + Sync + 'static) -> Self {
        Self {
            pin_check: Box::new(pin_check),
        }
    }

    /// Generate an SDP offer from the local peer connection.
    ///
    /// In the scaffold this returns a synthetic JSON offer. In the real
    /// webrtc-rs integration: `pc.create_offer(None).await?`.
    pub async fn create_offer(&self) -> Result<SessionDescription, RtcError> {
        debug!("signaling: creating offer");
        Ok(SessionDescription {
            sdp_type: "offer".into(),
            sdp: r#"{"codecs":["H265","H264"],"ice_ufrag":"synthetic","ice_pwd":"synthetic"}"#
                .into(),
        })
    }

    /// Apply an SDP answer received from the remote peer.
    ///
    /// Verifies the remote cert against the pinned set before accepting the
    /// answer. Returns [`RtcError::CertNotPinned`] if the check fails.
    pub async fn apply_answer(
        &self,
        answer: SessionDescription,
        peer_cert_der: &[u8],
    ) -> Result<(), RtcError> {
        if !(self.pin_check)(peer_cert_der) {
            return Err(RtcError::CertNotPinned);
        }
        debug!(sdp_type = %answer.sdp_type, "signaling: applied remote answer");
        Ok(())
    }

    /// Process an incoming SDP offer and produce an answer.
    ///
    /// Verifies the remote cert first.
    pub async fn create_answer_for(
        &self,
        offer: SessionDescription,
        peer_cert_der: &[u8],
    ) -> Result<SessionDescription, RtcError> {
        if !(self.pin_check)(peer_cert_der) {
            return Err(RtcError::CertNotPinned);
        }
        debug!(sdp_type = %offer.sdp_type, "signaling: creating answer");
        Ok(SessionDescription {
            sdp_type: "answer".into(),
            sdp: r#"{"codecs":["H265"],"ice_ufrag":"synthetic_ans","ice_pwd":"synthetic_ans"}"#
                .into(),
        })
    }
}

/// Perform an in-process SDP exchange between two scaffold peers.
///
/// Used by `smoke_loopback` to complete the handshake without a real network.
pub async fn in_process_exchange(
    offerer: &Signaling,
    answerer: &Signaling,
) -> Result<(), RtcError> {
    let offer = offerer.create_offer().await?;
    // Both peers trust synthetic certs in-process.
    let answer = answerer.create_answer_for(offer, b"synthetic_cert").await?;
    offerer.apply_answer(answer, b"synthetic_cert").await?;
    Ok(())
}
