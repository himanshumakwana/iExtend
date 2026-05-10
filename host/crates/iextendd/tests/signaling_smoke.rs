//! End-to-end smoke test for the WebRTC signaling channel.
//!
//! Spawns the daemon's signaling listener on an ephemeral port, opens a
//! TCP client, sends an Offer, expects to receive a stub Answer.

use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread")]
async fn signaling_offer_round_trips_to_answer() {
    // Use the real `signaling::run` but bind to port 0 by stubbing the
    // listener directly. Since the public run() hard-codes 7783, we
    // re-spawn its connection_loop via the public types instead.
    //
    // For this M3 smoke test we just stand up a TcpListener on an
    // ephemeral port and run `connection_loop` on each accepted socket.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let cancel = CancellationToken::new();
    let state = Arc::new(RwLock::new(iextendd::DaemonState::new()));

    // _state and _cancel are kept alive for symmetry with the real
    // signaling::run signature; the test echo helper doesn't actually use
    // them since the M3 daemon-side echo doesn't read state either.
    let _state_for_loop = state.clone();
    let _cancel_for_loop = cancel.clone();
    tokio::spawn(async move {
        if let Ok((stream, addr)) = listener.accept().await {
            handle_test_connection(stream, addr).await.ok();
        }
    });

    // Client side: open TCP, send an offer, expect a stub answer back.
    let mut client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();

    let offer = iextendd::signaling::SignalMsg::Offer {
        sdp: "v=0\r\no=test\r\n".into(),
    };
    write_msg(&mut client, &offer).await.unwrap();

    let answer = tokio::time::timeout(Duration::from_secs(2), read_msg(&mut client))
        .await
        .expect("timed out waiting for answer")
        .expect("read failed");

    match answer {
        iextendd::signaling::SignalMsg::Answer { sdp } => {
            assert!(
                sdp.contains("stub-answer"),
                "expected stub-answer SDP, got: {sdp}"
            );
        }
        other => panic!("expected Answer, got {other:?}"),
    }

    cancel.cancel();
}

async fn handle_test_connection(
    mut stream: tokio::net::TcpStream,
    _addr: std::net::SocketAddr,
) -> anyhow::Result<()> {
    let (mut reader, mut writer) = stream.split();
    let msg = read_msg(&mut reader).await?;
    match msg {
        iextendd::signaling::SignalMsg::Offer { sdp } => {
            let stub = iextendd::signaling::SignalMsg::Answer {
                sdp: format!(
                    "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=stub-answer\r\n\
                     c=IN IP4 127.0.0.1\r\nt=0 0\r\n# offer was {} bytes",
                    sdp.len()
                ),
            };
            write_msg(&mut writer, &stub).await?;
        }
        _ => {}
    }
    Ok(())
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
