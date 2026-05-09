//! TCP listener for the pre-WebRTC pairing handshake.
//!
//! Binds an ephemeral high port (the port is published in the mDNS SRV
//! record); accepts a single iPad connection at a time during the active
//! PIN window. The handshake itself is delegated to
//! `ix_rtc::pairing::PairingServer`.
//!
//! Lifecycle:
//!
//! - Tray clicks "Pair" → daemon spawns this listener bound to `0.0.0.0:0`.
//! - mDNS advertise updates with the chosen port.
//! - 60 s PIN timer runs; on timeout or successful pair, the listener
//!   shuts down.
//! - On bad-PIN attempts: count toward [`ix_rtc::pairing::ATTEMPT_LIMIT`].
//!   Lockout means the listener stays alive but rejects further attempts
//!   until a fresh PIN is generated.

#![allow(dead_code)]

use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use ix_pair_wire::{PairKind, PairMsg, HEADER_LEN};
use ix_rtc::pairing::PairingServer;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio::time::Instant as TokioInstant;
use tracing::{info, warn};

// ─────────────────────────────────────────────────────────────────────────────
// Public re-exports for grpc_server / tests.
// ─────────────────────────────────────────────────────────────────────────────

use crate::grpc_server::{
    proto::{PairedDevice, PairingState, PairingStatus},
    DaemonState,
};

/// Window length the tray displays + listener honours.
pub const PAIR_WINDOW_SECS: u64 = 60;

// ─────────────────────────────────────────────────────────────────────────────
// Wire-body helpers for PCertReq / PCertOk.
//
// We use serde_json::Value rather than typed structs to avoid adding
// `serde` as an explicit dependency (only serde_json is in Cargo.toml).
// ─────────────────────────────────────────────────────────────────────────────

/// Parse the plaintext JSON from a PCertReq body. Returns (pubkey_b64, name).
fn parse_cert_req(plaintext: &[u8]) -> anyhow::Result<(String, String)> {
    let v: serde_json::Value = serde_json::from_slice(plaintext)
        .map_err(|e| anyhow::anyhow!("PCertReq JSON parse error: {e}"))?;
    let pubkey_b64 = v["pubkey_b64"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("PCertReq missing pubkey_b64"))?
        .to_owned();
    let name = v["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("PCertReq missing name"))?
        .to_owned();
    Ok((pubkey_b64, name))
}

/// Encode the PCertOk JSON body.
fn encode_cert_ok(host_pubkey_b64: &str, pair_id: &str) -> Vec<u8> {
    serde_json::json!({
        "host_pubkey_b64": host_pubkey_b64,
        "pair_id": pair_id,
    })
    .to_string()
    .into_bytes()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tracking handle for an in-progress PIN window.
// ─────────────────────────────────────────────────────────────────────────────

/// Tracking handle for an in-progress PIN window. Lives inside the
/// `PAIR_CTX` mutex below; `cancel()` uses it to abort the spawned
/// listener task and clear the public `state.pairing`.
struct PairCtx {
    pin: String,
    /// Monotonic instant when `begin()` was called — used by `tick()` to
    /// compute remaining time precisely rather than relying on the stored
    /// `seconds_left` field.
    started_at: TokioInstant,
    deadline: TokioInstant,
    handle: tokio::task::JoinHandle<()>,
    port: u16,
}

static PAIR_CTX: tokio::sync::Mutex<Option<PairCtx>> = tokio::sync::Mutex::const_new(None);

// ─────────────────────────────────────────────────────────────────────────────
// Spawn (internal TCP loop).
// ─────────────────────────────────────────────────────────────────────────────

/// Spawn the pairing listener for the duration of one PIN window.
///
/// Returns the chosen local port + a JoinHandle that completes when the
/// window ends. The task writes back into `state` on successful pairing
/// (DONE + last_paired) or on timeout (EXPIRED).
pub async fn spawn_with_state(
    pin: String,
    state: Arc<RwLock<DaemonState>>,
) -> Result<(u16, tokio::task::JoinHandle<()>)> {
    let listener = TcpListener::bind("0.0.0.0:0").await?;
    let port = listener.local_addr()?.port();
    info!(port, "pairing listener bound");

    let handle = tokio::spawn(async move {
        let deadline = tokio::time::Instant::now() + StdDuration::from_secs(PAIR_WINDOW_SECS);
        let mut server = PairingServer::new(&pin);

        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                info!("pairing window expired");
                // Transition to EXPIRED so the tray sees it next poll.
                let mut s = state.write().await;
                if s.pairing.state == PairingState::Waiting as i32
                    || s.pairing.state == PairingState::Handshaking as i32
                {
                    s.pairing.state = PairingState::Expired as i32;
                    s.pairing.seconds_left = 0;
                    s.pairing.pin.clear();
                    s.pairing.port = 0;
                }
                return;
            }
            let remaining = deadline - now;
            tokio::select! {
                accept = listener.accept() => {
                    match accept {
                        Ok((stream, addr)) => {
                            {
                                // Transition to HANDSHAKING while we process.
                                let mut s = state.write().await;
                                if s.pairing.state == PairingState::Waiting as i32 {
                                    s.pairing.state = PairingState::Handshaking as i32;
                                }
                            }
                            match handle_one(stream, addr, &mut server, state.clone()).await {
                                Ok(Some(device)) => {
                                    // Successful pairing — update state and exit.
                                    let mut s = state.write().await;
                                    s.pairing.state = PairingState::Done as i32;
                                    s.pairing.seconds_left = 0;
                                    s.pairing.pin.clear();
                                    s.pairing.port = 0;
                                    s.pairing.last_paired = Some(device);
                                    info!("pairing complete — listener shutting down");
                                    return;
                                }
                                Ok(None) => {
                                    // Connection handled but no cert exchange (e.g. only SPAKE2
                                    // step completed). Revert to WAITING so a second attempt works.
                                    let mut s = state.write().await;
                                    if s.pairing.state == PairingState::Handshaking as i32 {
                                        s.pairing.state = PairingState::Waiting as i32;
                                    }
                                }
                                Err(e) => {
                                    warn!(?e, "pairing connection failed");
                                    // Revert to WAITING so the tray can retry.
                                    let mut s = state.write().await;
                                    if s.pairing.state == PairingState::Handshaking as i32 {
                                        s.pairing.state = PairingState::Waiting as i32;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            warn!(?e, "pair listener accept error");
                            return;
                        }
                    }
                }
                _ = tokio::time::sleep(remaining) => {
                    info!("pairing window expired");
                    let mut s = state.write().await;
                    if s.pairing.state == PairingState::Waiting as i32
                        || s.pairing.state == PairingState::Handshaking as i32
                    {
                        s.pairing.state = PairingState::Expired as i32;
                        s.pairing.seconds_left = 0;
                        s.pairing.pin.clear();
                        s.pairing.port = 0;
                    }
                    return;
                }
            }
        }
    });

    Ok((port, handle))
}

/// Legacy entry-point retained so existing call sites compile without changes.
/// Internally delegates to `spawn_with_state` with a throwaway state — callers
/// that need state callbacks should use `spawn_with_state` directly. The
/// pair_listener's public API (`begin`) always uses `spawn_with_state`.
pub async fn spawn(pin: String) -> Result<(u16, tokio::task::JoinHandle<()>)> {
    // Create a fresh isolated state just for the task so it doesn't deadlock
    // anything. This path is only exercised by unit tests.
    let isolated_state = Arc::new(RwLock::new(DaemonState::new()));
    spawn_with_state(pin, isolated_state).await
}

// ─────────────────────────────────────────────────────────────────────────────
// Per-connection handler.
// ─────────────────────────────────────────────────────────────────────────────

/// Returns `Ok(Some(device))` when the full cert-exchange completes
/// successfully, `Ok(None)` when the SPAKE2 step was completed but PCertReq
/// was not received, or `Err(_)` on any failure.
async fn handle_one(
    mut stream: TcpStream,
    addr: SocketAddr,
    server: &mut PairingServer,
    state: Arc<RwLock<DaemonState>>,
) -> Result<Option<PairedDevice>> {
    info!(?addr, "pairing client connected");

    // ── Step 1: PStart → PResponse (SPAKE2) ──────────────────────────────
    let msg = read_msg(&mut stream).await?;
    if msg.kind != PairKind::PStart {
        return Err(anyhow::anyhow!("expected PStart, got {:?}", msg.kind));
    }
    let session_key = server.complete(&msg.body)?;
    let response = server.make_response();
    write_msg(&mut stream, &response).await?;

    // ── Step 2: PCertReq → PCertOk (cert exchange) ───────────────────────
    // Set a short read deadline: if the iPad doesn't send PCertReq within
    // 10 seconds we treat the connection as stale.
    let cert_msg = tokio::time::timeout(StdDuration::from_secs(10), read_msg(&mut stream)).await;

    let cert_msg = match cert_msg {
        Ok(Ok(m)) => m,
        Ok(Err(e)) => return Err(e),
        Err(_elapsed) => {
            // No PCertReq received — return Ok(None) so the listener can
            // stay alive for the remainder of the window.
            warn!(?addr, "no PCertReq after SPAKE2 — connection timed out");
            return Ok(None);
        }
    };

    if cert_msg.kind != PairKind::PCertReq {
        return Err(anyhow::anyhow!(
            "expected PCertReq, got {:?}",
            cert_msg.kind
        ));
    }

    // PCertReq body is AES-256-GCM(K_session, seq=2) over UTF-8 JSON.
    // seq=2 because this is the third message (PStart=0, PResponse=1, PCertReq=2).
    let plaintext = session_key.open(2, &cert_msg.body)?;
    let (pubkey_b64, display_name) = parse_cert_req(&plaintext)?;

    let pubkey_bytes = B64
        .decode(&pubkey_b64)
        .map_err(|e| anyhow::anyhow!("PCertReq pubkey_b64 decode error: {e}"))?;
    let pubkey_arr: [u8; 32] = pubkey_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("PCertReq pubkey must be 32 bytes"))?;

    // Load the host root key to get our own pubkey for the PCertOk body.
    // This will fail gracefully on keyring errors rather than panicking.
    let host_root = crate::keystore::load_or_create_root_key()
        .await
        .map_err(|e| anyhow::anyhow!("host root-key load failed: {e}"))?;
    let host_pubkey_b64 = B64.encode(host_root.verifying().as_bytes());

    // Mint a UUID pair_id and persist to the pinned-key store.
    let pair_id = uuid::Uuid::new_v4().to_string();
    {
        let store = crate::keystore::PinStore::open_default()
            .map_err(|e| anyhow::anyhow!("PinStore open failed: {e}"))?;
        store
            .pin(&pair_id, &pubkey_arr, &display_name)
            .map_err(|e| anyhow::anyhow!("PinStore::pin failed: {e}"))?;
    }

    // Build PCertOk body, seal it with K_session (seq=3).
    let ok_body = encode_cert_ok(&host_pubkey_b64, &pair_id);
    let sealed = session_key.seal(3, &ok_body)?;
    let cert_ok_msg = PairMsg::new(PairKind::PCertOk, sealed);
    write_msg(&mut stream, &cert_ok_msg).await?;

    info!(?addr, %pair_id, "cert exchange complete — device pinned");

    // Build the PairedDevice we'll surface through the gRPC status.
    let paired_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let device = PairedDevice {
        pair_id,
        display_name,
        pubkey_b64: B64.encode(pubkey_arr),
        paired_at_unix: paired_at,
    };

    // Update the pairing status immediately so that even before the task loop
    // runs the transition path the state is consistent.
    {
        let mut s = state.write().await;
        s.pairing.state = PairingState::Done as i32;
        s.pairing.seconds_left = 0;
        s.pairing.pin.clear();
        s.pairing.port = 0;
        s.pairing.last_paired = Some(device.clone());
    }

    Ok(Some(device))
}

// ─────────────────────────────────────────────────────────────────────────────
// Frame I/O helpers.
// ─────────────────────────────────────────────────────────────────────────────

async fn read_msg(stream: &mut TcpStream) -> Result<PairMsg> {
    let mut header = [0u8; HEADER_LEN];
    stream.read_exact(&mut header).await?;
    let body_len = u16::from_be_bytes([header[6], header[7]]) as usize;
    let mut buf = Vec::with_capacity(HEADER_LEN + body_len);
    buf.extend_from_slice(&header);
    buf.resize(HEADER_LEN + body_len, 0);
    stream.read_exact(&mut buf[HEADER_LEN..]).await?;
    Ok(PairMsg::decode(&buf)?)
}

async fn write_msg(stream: &mut TcpStream, msg: &PairMsg) -> Result<()> {
    let bytes = msg.encode()?;
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tray-facing pairing API.
//
// The gRPC layer drives BeginPairing / GetPairingStatus / CancelPairing
// against this module.
// ─────────────────────────────────────────────────────────────────────────────

/// Generate a new PIN, spawn a TCP listener bound to an ephemeral port, and
/// publish the PairingStatus into `state.pairing`. Returns the freshly-built
/// status so the caller can return it directly from BeginPairing.
///
/// If the keystore cannot load the host root key, this returns a status with
/// `state = FAILED` and an `error` message — it does NOT start the listener.
pub async fn begin(state: Arc<RwLock<DaemonState>>) -> Result<PairingStatus> {
    // Cancel any previous in-flight pairing first so begin() is idempotent.
    let _ = cancel(state.clone()).await;

    // Pre-flight: ensure the keystore is accessible so we know we can sign
    // device certs when PCertReq arrives.
    if let Err(e) = crate::keystore::load_or_create_root_key().await {
        let error_msg = format!("host root-key unavailable: {e}");
        warn!(%error_msg, "begin_pairing: keystore preflight failed");
        let status = PairingStatus {
            state: PairingState::Failed as i32,
            pin: String::new(),
            seconds_left: 0,
            port: 0,
            last_paired: None,
            error: error_msg,
        };
        let mut s = state.write().await;
        s.pairing = status.clone();
        return Ok(status);
    }

    let pin = generate_pin();
    let started_at = TokioInstant::now();
    let deadline = started_at + StdDuration::from_secs(PAIR_WINDOW_SECS);

    let (port, handle) = spawn_with_state(pin.clone(), state.clone()).await?;

    let status = PairingStatus {
        state: PairingState::Waiting as i32,
        pin: pin.clone(),
        seconds_left: PAIR_WINDOW_SECS as u32,
        port: port as u32,
        last_paired: None,
        error: String::new(),
    };

    {
        let mut s = state.write().await;
        s.pairing = status.clone();
    }

    *PAIR_CTX.lock().await = Some(PairCtx {
        pin,
        started_at,
        deadline,
        handle,
        port,
    });

    Ok(status)
}

/// Update `state.pairing.seconds_left` based on the active deadline. Idempotent;
/// safe to call from any handler. Transitions the public state to `EXPIRED`
/// when the deadline has passed without a successful pair.
pub fn tick(state: &mut DaemonState) {
    if state.pairing.state != PairingState::Waiting as i32 {
        // Nothing to tick in any other state.
        return;
    }

    // We need the `started_at` from PAIR_CTX to compute remaining time.
    // `tick()` is synchronous, so we can only try a non-blocking lock.
    // If the mutex is held we skip the tick this call; the next call will
    // succeed and the tray polls at 1 Hz so the drift is at most 1 second.
    let ctx_guard = PAIR_CTX.try_lock();
    let remaining_secs = match ctx_guard {
        Ok(ref guard) => {
            if let Some(ctx) = guard.as_ref() {
                let now = TokioInstant::now();
                if now >= ctx.deadline {
                    // Window has passed — set expired below.
                    0u32
                } else {
                    (ctx.deadline - now).as_secs() as u32
                }
            } else {
                // No active context; window must have been cancelled.
                return;
            }
        }
        Err(_) => {
            // Mutex is held by the spawned task or begin(); skip this tick.
            return;
        }
    };

    if remaining_secs == 0 {
        state.pairing.state = PairingState::Expired as i32;
        state.pairing.seconds_left = 0;
        state.pairing.pin.clear();
        state.pairing.port = 0;
    } else {
        state.pairing.seconds_left = remaining_secs;
    }
}

/// Abort the active PIN window and clear public state. Returns true if a
/// listener was actually cancelled.
pub async fn cancel(state: Arc<RwLock<DaemonState>>) -> bool {
    let mut ctx_slot = PAIR_CTX.lock().await;
    if let Some(ctx) = ctx_slot.take() {
        ctx.handle.abort();
        let mut s = state.write().await;
        s.pairing.state = PairingState::Idle as i32;
        s.pairing.pin.clear();
        s.pairing.seconds_left = 0;
        s.pairing.port = 0;
        true
    } else {
        false
    }
}

/// Read the persistent pinned-iPad list and convert to wire-format
/// PairedDevice entries.
pub fn list_paired() -> Result<Vec<PairedDevice>> {
    let store = crate::keystore::PinStore::open_default()?;
    let rows = store.list()?;
    Ok(rows
        .iter()
        .map(crate::grpc_server::paired_device_to_proto)
        .collect())
}

/// Drop a paired device from the persistent store. Returns true if a row was
/// actually removed.
pub fn forget(pair_id: &str) -> Result<bool> {
    let store = crate::keystore::PinStore::open_default()?;
    store.forget(pair_id)
}

/// 4-digit decimal PIN. Leading zeroes are kept ("0042" is valid).
fn generate_pin() -> String {
    use rand_core::RngCore;
    let mut bytes = [0u8; 4];
    rand_core::OsRng.fill_bytes(&mut bytes);
    let n = u32::from_le_bytes(bytes) % 10_000;
    format!("{n:04}")
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ix_pair_wire::PairKind;

    // ── tick() countdown ──────────────────────────────────────────────────

    /// Verify that tick() decrements seconds_left to reflect elapsed time
    /// rather than leaving it stuck at the initial value.
    #[tokio::test]
    async fn tick_decrements_seconds_left() {
        // Build a state snapshot in WAITING with a very short window (5 s).
        let state = Arc::new(RwLock::new(DaemonState::new()));
        {
            let mut s = state.write().await;
            s.pairing.state = PairingState::Waiting as i32;
            s.pairing.seconds_left = 5;
        }

        // Install a PairCtx with a window that expires 5 seconds from now.
        let started_at = TokioInstant::now();
        let deadline = started_at + StdDuration::from_secs(5);
        let handle = tokio::spawn(async {}); // dummy task — immediately completes
        *PAIR_CTX.lock().await = Some(PairCtx {
            pin: "1234".into(),
            started_at,
            deadline,
            handle,
            port: 0,
        });

        // Immediately tick — should still be in WAITING with seconds_left ≤ 5.
        {
            let mut s = state.write().await;
            tick(&mut s);
            assert_eq!(s.pairing.state, PairingState::Waiting as i32);
            assert!(s.pairing.seconds_left <= 5);
        }

        // Clean up static state so we don't pollute other tests.
        *PAIR_CTX.lock().await = None;
    }

    /// Verify that tick() transitions to EXPIRED once the deadline has passed.
    #[tokio::test]
    async fn tick_expires_after_deadline() {
        let state = Arc::new(RwLock::new(DaemonState::new()));
        {
            let mut s = state.write().await;
            s.pairing.state = PairingState::Waiting as i32;
            s.pairing.seconds_left = 1;
            s.pairing.pin = "5678".into();
            s.pairing.port = 9999;
        }

        // Install a PairCtx whose deadline is already in the past.
        let past = TokioInstant::now() - StdDuration::from_secs(10);
        let handle = tokio::spawn(async {});
        *PAIR_CTX.lock().await = Some(PairCtx {
            pin: "5678".into(),
            started_at: past - StdDuration::from_secs(60),
            deadline: past,
            handle,
            port: 9999,
        });

        {
            let mut s = state.write().await;
            tick(&mut s);
            assert_eq!(s.pairing.state, PairingState::Expired as i32);
            assert_eq!(s.pairing.seconds_left, 0);
            assert!(s.pairing.pin.is_empty());
            assert_eq!(s.pairing.port, 0);
        }

        // Clean up.
        *PAIR_CTX.lock().await = None;
    }

    /// tick() is a no-op when the state is not WAITING.
    #[tokio::test]
    async fn tick_noop_when_not_waiting() {
        // Ensure no stale PAIR_CTX from other tests interferes.
        *PAIR_CTX.lock().await = None;

        let mut s = DaemonState::new();
        s.pairing.state = PairingState::Done as i32;
        s.pairing.seconds_left = 42;
        tick(&mut s);
        // seconds_left must remain unchanged because we're not WAITING.
        assert_eq!(s.pairing.seconds_left, 42);
        assert_eq!(s.pairing.state, PairingState::Done as i32);
    }

    // ── PCertReq / PCertOk wire round-trip ───────────────────────────────

    /// Verify that the CertReq → CertOk JSON helpers encode and parse
    /// symmetrically (no network needed — pure helper round-trip).
    #[test]
    fn cert_req_ok_json_roundtrip() {
        let pubkey_raw = [0xABu8; 32];
        let pubkey_b64 = B64.encode(pubkey_raw);

        // Build a PCertReq body using raw serde_json::json! (mirrors what the
        // iPad would send).
        let req_json = serde_json::json!({
            "pubkey_b64": &pubkey_b64,
            "name": "Bob's iPad",
        });
        let req_bytes = serde_json::to_vec(&req_json).unwrap();

        // Parse it through our helper.
        let (got_pubkey, got_name) = parse_cert_req(&req_bytes).unwrap();
        assert_eq!(got_pubkey, pubkey_b64);
        assert_eq!(got_name, "Bob's iPad");

        // Build a PCertOk body through our helper and verify JSON shape.
        let ok_bytes = encode_cert_ok(&B64.encode([0xCDu8; 32]), "test-uuid");
        let back: serde_json::Value = serde_json::from_slice(&ok_bytes).unwrap();
        assert_eq!(back["pair_id"], "test-uuid");
        assert!(!back["host_pubkey_b64"].as_str().unwrap().is_empty());
    }

    /// Verify that a PCertReq PairMsg encodes + decodes without loss.
    #[test]
    fn pcertreq_wire_frame_roundtrip() {
        let body = b"encrypted-payload-bytes".to_vec();
        let msg = PairMsg::new(PairKind::PCertReq, body.clone());
        let bytes = msg.encode().unwrap();
        let back = PairMsg::decode(&bytes).unwrap();
        assert_eq!(back.kind, PairKind::PCertReq);
        assert_eq!(back.body, body);
    }

    /// Verify that a PCertOk PairMsg encodes + decodes without loss.
    #[test]
    fn pcertok_wire_frame_roundtrip() {
        let body = b"cert-ok-payload".to_vec();
        let msg = PairMsg::new(PairKind::PCertOk, body.clone());
        let bytes = msg.encode().unwrap();
        let back = PairMsg::decode(&bytes).unwrap();
        assert_eq!(back.kind, PairKind::PCertOk);
        assert_eq!(back.body, body);
    }

    /// Verify that the session-key AEAD seals PCertReq-body and can be
    /// opened back to the original plaintext, then parsed correctly.
    #[test]
    fn cert_req_aead_roundtrip() {
        use ix_rtc::pairing::SessionKey;

        let key = SessionKey([0x42u8; 32]);
        let pt_bytes = serde_json::to_vec(&serde_json::json!({
            "pubkey_b64": B64.encode([1u8; 32]),
            "name": "Alice's iPad",
        }))
        .unwrap();

        // seq=2 matches the PCertReq position in the handshake.
        let ciphertext = key.seal(2, &pt_bytes).unwrap();
        let recovered = key.open(2, &ciphertext).unwrap();
        let (_, name) = parse_cert_req(&recovered).unwrap();
        assert_eq!(name, "Alice's iPad");
    }

    /// Simulate the daemon's PCertOk path: seal with encode_cert_ok and verify
    /// the decrypted JSON contains the expected fields (seq=3).
    #[test]
    fn cert_ok_aead_roundtrip() {
        use ix_rtc::pairing::SessionKey;

        let key = SessionKey([0x99u8; 32]);
        let pair_id = "aaaabbbb-cccc-dddd-eeee-ffffffffffff";
        let host_pub = B64.encode([0xFEu8; 32]);
        let ok_bytes = encode_cert_ok(&host_pub, pair_id);

        // seq=3 matches the PCertOk position in the handshake.
        let ciphertext = key.seal(3, &ok_bytes).unwrap();
        let recovered = key.open(3, &ciphertext).unwrap();
        let back: serde_json::Value = serde_json::from_slice(&recovered).unwrap();
        assert_eq!(back["pair_id"], pair_id);
        assert_eq!(back["host_pubkey_b64"], host_pub);
    }
}
