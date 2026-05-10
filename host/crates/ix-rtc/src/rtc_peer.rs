//! Real WebRTC peer connection (Plan A milestone M4).
//!
//! Wraps `webrtc-rs::peer_connection::RTCPeerConnection` with the surface
//! `iextendd::session` needs:
//! - create with default codecs + interceptor registry
//! - add an H.264 video track that the encoder feeds NAL units into
//! - create/set local + remote SDP descriptions
//! - add ICE candidates as they arrive from the signaling channel
//!
//! Kept separate from the existing `peer::Peer` scaffold so the in-process
//! `smoke_loopback` test path stays compilable while real WebRTC is being
//! brought up. Once M4 lands and replaces the scaffold, the old `Peer`
//! type can be deleted.

use anyhow::{anyhow, Context, Result};
use std::sync::Arc;
use tracing::{info, warn};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_H264};
use webrtc::api::APIBuilder;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use webrtc::track::track_local::TrackLocal;

/// One real WebRTC peer connection paired with its outbound H.264 track.
///
/// `pc` is wrapped in `Arc` because webrtc-rs ICE / state callbacks need
/// to outlive the scope that constructs the peer.
pub struct RtcPeer {
    /// The underlying webrtc-rs peer connection.
    pub pc: Arc<RTCPeerConnection>,
    /// Outbound H.264 video track. The encoder writes encoded NAL access
    /// units into this via `write_sample`; webrtc-rs handles RTP framing
    /// and SRTP encryption.
    pub video_track: Arc<TrackLocalStaticSample>,
}

impl RtcPeer {
    /// Build a peer with default Google STUN servers, default codecs,
    /// default interceptors, and one outgoing H.264 video track.
    pub async fn new() -> Result<Self> {
        let mut media_engine = MediaEngine::default();
        media_engine
            .register_default_codecs()
            .context("register_default_codecs")?;

        let mut registry = Registry::new();
        registry = register_default_interceptors(registry, &mut media_engine)
            .context("register_default_interceptors")?;

        let api = APIBuilder::new()
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
            .build();

        // STUN-only by default — TURN comes later if the LAN-only deployment
        // assumption is broken. Most home networks need just STUN to discover
        // the public reflexive candidate.
        let config = RTCConfiguration {
            ice_servers: vec![RTCIceServer {
                urls: vec!["stun:stun.l.google.com:19302".into()],
                ..Default::default()
            }],
            ..Default::default()
        };

        let pc = Arc::new(api.new_peer_connection(config).await?);

        // Outbound video track. Encoder writes encoded H.264 samples into
        // `video_track` via `write_sample`; webrtc-rs handles RTP framing,
        // SRTP encryption, and transmission.
        let video_track = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: MIME_TYPE_H264.to_owned(),
                ..Default::default()
            },
            "video".to_owned(),          // track id
            "iextend-mirror".to_owned(), // stream id (groups tracks per session)
        ));

        // Add as a sender. webrtc-rs returns an RTCRtpSender we don't need
        // to retain — the peer connection holds the strong reference.
        pc.add_track(video_track.clone() as Arc<dyn TrackLocal + Send + Sync>)
            .await
            .context("add_track")?;

        // Wire connection-state changes for diagnostics. The session loop
        // separately observes pc.on_peer_connection_state_change for the
        // user-visible session state — this one just logs.
        pc.on_peer_connection_state_change(Box::new(|state: RTCPeerConnectionState| {
            info!(?state, "RTCPeerConnection state changed");
            Box::pin(async {})
        }));

        Ok(Self { pc, video_track })
    }

    /// Create a local SDP offer + apply it as the local description. Returns
    /// the offer SDP string the caller should send over the signaling
    /// channel.
    pub async fn create_offer(&self) -> Result<String> {
        let offer = self.pc.create_offer(None).await?;
        self.pc.set_local_description(offer.clone()).await?;
        Ok(offer.sdp)
    }

    /// Apply a remote SDP answer received via signaling.
    pub async fn apply_answer(&self, answer_sdp: &str) -> Result<()> {
        let answer = RTCSessionDescription::answer(answer_sdp.to_owned())?;
        self.pc.set_remote_description(answer).await?;
        Ok(())
    }

    /// Apply a remote SDP offer received via signaling. Returns the answer
    /// SDP we generated. Used in the iPad-initiated direction.
    pub async fn apply_offer_create_answer(&self, offer_sdp: &str) -> Result<String> {
        let offer = RTCSessionDescription::offer(offer_sdp.to_owned())?;
        self.pc.set_remote_description(offer).await?;
        let answer = self.pc.create_answer(None).await?;
        self.pc.set_local_description(answer.clone()).await?;
        Ok(answer.sdp)
    }

    /// Add a remote ICE candidate received via signaling.
    pub async fn add_ice_candidate(&self, candidate: &str) -> Result<()> {
        let cand = RTCIceCandidateInit {
            candidate: candidate.to_owned(),
            ..Default::default()
        };
        self.pc.add_ice_candidate(cand).await?;
        Ok(())
    }

    /// Push an encoded H.264 access unit (Annex-B NALs) into the video
    /// track. Call this from the encoder loop.
    pub async fn write_sample(&self, data: &[u8], duration: std::time::Duration) -> Result<()> {
        use webrtc::media::Sample;
        let sample = Sample {
            data: bytes::Bytes::copy_from_slice(data),
            duration,
            ..Default::default()
        };
        self.video_track
            .write_sample(&sample)
            .await
            .map_err(|e| anyhow!("write_sample: {e}"))?;
        Ok(())
    }

    /// Graceful shutdown — closes the peer connection, drops all tracks.
    pub async fn close(self) {
        if let Err(e) = self.pc.close().await {
            warn!(err = %e, "RtcPeer.close: pc.close failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test: can construct a peer + produce an offer SDP that looks
    /// like a real H.264 video offer.
    #[tokio::test]
    async fn create_offer_returns_h264_video_sdp() {
        let peer = RtcPeer::new().await.expect("RtcPeer::new failed");
        let offer = peer.create_offer().await.expect("create_offer failed");

        // SDPs are big — print the first ~600 chars for diagnostics, since
        // a regression in codec registration will show up here as missing
        // H.264 PT lines.
        let preview: String = offer.chars().take(600).collect();
        eprintln!("offer SDP preview:\n{preview}\n...");

        assert!(offer.starts_with("v=0"), "offer should start with SDP v=0");
        assert!(
            offer.contains("m=video"),
            "offer should advertise an m=video section"
        );
        assert!(
            offer.contains("H264") || offer.contains("h264"),
            "offer should advertise H.264 — got SDP without H264 codec mention"
        );

        // Tear down so the test exits cleanly without leaking the
        // background ICE agent.
        peer.close().await;
    }

    /// Verify the bidirectional offer/answer flow works between two
    /// in-process peers. This validates that SDP we generate can be
    /// consumed by another webrtc-rs instance — a strong proxy for real
    /// interop with the iPad's WebRTC.framework.
    #[tokio::test]
    async fn local_offer_answer_round_trip() {
        let alice = RtcPeer::new().await.unwrap();
        let bob = RtcPeer::new().await.unwrap();

        let offer = alice.create_offer().await.unwrap();
        let answer = bob.apply_offer_create_answer(&offer).await.unwrap();
        alice.apply_answer(&answer).await.unwrap();

        // Both sides should now be in have-local/remote-description state
        // (formal "Connected" requires DTLS handshake, which won't complete
        // because we don't exchange ICE candidates here — that's M4d).
        assert!(answer.starts_with("v=0"));
        assert!(answer.contains("m=video"));

        alice.close().await;
        bob.close().await;
    }
}
