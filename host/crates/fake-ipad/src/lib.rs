//! fake-ipad library interface — exposed for integration tests.

pub mod pairing_client;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use ix_pair_wire::{PairMsg, HEADER_LEN};

/// JSON record saved after a successful pair.
#[derive(Debug, Serialize, Deserialize)]
pub struct PairRecord {
    pub pair_id: String,
    pub host_pubkey_b64: String,
    pub display_name: String,
    pub paired_at_unix: i64,
}

pub fn last_pair_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home)
        .join(".config")
        .join("fake-ipad")
        .join("last-pair.json")
}

pub fn save_pair_record(record: &PairRecord) -> anyhow::Result<()> {
    let path = last_pair_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(record)?;
    std::fs::write(&path, bytes)?;
    Ok(())
}

pub async fn send_msg(stream: &mut TcpStream, msg: &PairMsg) -> anyhow::Result<()> {
    let bytes = msg.encode()?;
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    Ok(())
}

pub async fn recv_msg(stream: &mut TcpStream) -> anyhow::Result<PairMsg> {
    let mut header = [0u8; HEADER_LEN];
    stream.read_exact(&mut header).await?;
    let body_len = u16::from_be_bytes([header[6], header[7]]) as usize;
    let mut buf = Vec::with_capacity(HEADER_LEN + body_len);
    buf.extend_from_slice(&header);
    buf.resize(HEADER_LEN + body_len, 0);
    stream.read_exact(&mut buf[HEADER_LEN..]).await?;
    Ok(PairMsg::decode(&buf)?)
}
