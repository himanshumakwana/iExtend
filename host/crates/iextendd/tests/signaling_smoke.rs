//! End-to-end smoke test for the WebRTC signaling channel + RtcPeer
//! bridge (Plan A milestones M3 + M4b).
//!
//! Drives the real `connection_loop` with a real `RtcPeer` client over
//! a TCP socket. Validates that:
//!   - daemon side accepts an SDP offer
//!   - daemon's RtcPeer generates a compatible Answer
//!   - the answer is structurally valid WebRTC SDP (v=0, m=video, H264)
//!   - the client's RtcPeer accepts the daemon's answer without error

use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread")]
async fn signaling_offer_round_trips_through_real_rtc_peer() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let cancel = CancellationToken::new();
    let state = Arc::new(RwLock::new(iextendd::DaemonState::new()));

    // Spawn the real daemon-side connection_loop. ScreenShare's worker
    // idles on Linux (no DXGI capture) so the test doesn't pump frames;
    // signaling SDP/ICE exchange happens regardless.
    let cancel_for_loop = cancel.clone();
    let state_for_loop = state.clone();
    let screen_share = iextendd::screen_share::ScreenShare::start(cancel.clone());
    tokio::spawn(async move {
        if let Ok((stream, addr)) = listener.accept().await {
            let _ = iextendd::signaling::connection_loop(
                stream,
                addr,
                state_for_loop,
                cancel_for_loop,
                screen_share,
            )
            .await;
        }
    });

    // Client side: build a real RtcPeer to generate a valid offer.
    let alice = ix_rtc::rtc_peer::RtcPeer::new().await.unwrap();
    let offer_sdp = alice.create_offer().await.unwrap();

    let mut client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();

    // Send the offer.
    let offer = iextendd::signaling::SignalMsg::Offer {
        sdp: offer_sdp.clone(),
    };
    write_msg(&mut client, &offer).await.unwrap();

    // Drain incoming frames until we get an Answer. The daemon may
    // interleave Ice candidates ahead of the answer (webrtc-rs's
    // gathering callback fires concurrently with set_local_description),
    // so we filter on the variant.
    let answer_sdp = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match read_msg(&mut client).await.unwrap() {
                iextendd::signaling::SignalMsg::Answer { sdp } => return sdp,
                iextendd::signaling::SignalMsg::Ice { candidate } => {
                    eprintln!("(received ICE candidate during answer wait: {candidate})");
                }
                other => panic!("unexpected variant before Answer: {other:?}"),
            }
        }
    })
    .await
    .expect("timed out waiting for real Answer");

    eprintln!("daemon answered with SDP ({} bytes)", answer_sdp.len());
    assert!(answer_sdp.starts_with("v=0"));
    assert!(answer_sdp.contains("m=video"));
    assert!(
        answer_sdp.contains("H264") || answer_sdp.contains("h264"),
        "answer should advertise H.264"
    );

    // Round-trip the answer back into the client peer to confirm SDP
    // compatibility — this is the strongest signal that the daemon's peer
    // produced something webrtc-rs can ingest.
    alice
        .apply_answer(&answer_sdp)
        .await
        .expect("client peer rejected daemon's answer");

    cancel.cancel();
    alice.close().await;
}

async fn read_msg<R: AsyncReadExt + Unpin>(
    reader: &mut R,
) -> anyhow::Result<iextendd::signaling::SignalMsg> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    Ok(serde_json::from_slice(&buf)?)
}

async fn write_msg<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    msg: &iextendd::signaling::SignalMsg,
) -> anyhow::Result<()> {
    let body = serde_json::to_vec(msg)?;
    let len = (body.len() as u32).to_be_bytes();
    writer.write_all(&len).await?;
    writer.write_all(&body).await?;
    writer.flush().await?;
    Ok(())
}
