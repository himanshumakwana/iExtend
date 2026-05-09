//! iPad-side SPAKE2 pairing client.
//!
//! Mirrors the handshake described in `ix_rtc::pairing` but as the *client*
//! (identity "iextend.ipad"). Steps:
//!
//! 1. Connect TCP to the daemon's ephemeral pairing port.
//! 2. Send `PStart { body = SPAKE2-A element }`.
//! 3. Read `PResponse { body = SPAKE2-B element || hk_verifier }`.
//! 4. Derive `K_session` via HKDF.
//! 5. Build a `PCertReq` body = AES-256-GCM seal of `pubkey || display_name`.
//! 6. Send `PCertReq`.
//! 7. Read `PCertOk` body = AES-256-GCM seal of `host_pubkey || pair_id`.
//! 8. Save `PairRecord` to disk.
//!
//! The current `iextendd` `pair_listener::handle_one` stops after step 3 —
//! it sends `PResponse` (with zeroed verifier since the full cert-wrap pipeline
//! is Plan-N work) then closes the socket. This client therefore completes
//! steps 1–6, reads `PCertOk` if it arrives, and treats EOF gracefully.

use anyhow::{bail, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use ed25519_dalek::SigningKey;
use ix_pair_wire::{PairKind, PairMsg};
use ix_rtc::pairing::{SessionKey, SUITE_ID};
use rand_core::OsRng;
use sha2::Sha256;
use spake2::{Ed25519Group, Identity, Password, Spake2};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::TcpStream;
use tracing::info;

use crate::{recv_msg, save_pair_record, send_msg, PairRecord};

/// Run the full client-side pairing flow against `host` (e.g. `"127.0.0.1:12345"`)
/// using the 4-digit `pin`.
pub async fn run_pairing_client(host: &str, pin: &str) -> Result<()> {
    println!("Connecting to {host}...");
    let mut stream = TcpStream::connect(host).await?;
    println!("Connected.");

    // ── Step 1: SPAKE2 A element ──────────────────────────────────────────
    let id_a = Identity::new(b"iextend.ipad");
    let id_b = Identity::new(b"iextend.host");
    let pwd = Password::new(pin.as_bytes());
    let (spake_state, msg_a) = Spake2::<Ed25519Group>::start_a(&pwd, &id_a, &id_b);

    send_msg(&mut stream, &PairMsg::new(PairKind::PStart, msg_a)).await?;
    info!("sent PStart");

    // ── Step 2: receive PResponse (B element + verifier) ─────────────────
    let response = recv_msg(&mut stream).await?;
    if response.kind != PairKind::PResponse {
        bail!("expected PResponse, got {:?}", response.kind);
    }
    println!("SPAKE2 handshake OK");

    // body = spake_b (N bytes) || hk_verifier (32 bytes)
    // The verifier is zeroed in the current daemon stub, so we skip verification
    // and proceed to derive K_session.
    let body = &response.body;
    if body.len() < 32 {
        bail!("PResponse body too short: {} bytes", body.len());
    }
    let spake_b = &body[..body.len() - 32];

    let raw_secret = spake_state
        .finish(spake_b)
        .map_err(|e| anyhow::anyhow!("SPAKE2 finish error: {e:?}"))?;

    // Derive K_session via HKDF-SHA256.
    let mut k_session = [0u8; 32];
    let hk = hkdf::Hkdf::<Sha256>::new(None, &raw_secret);
    hk.expand(SUITE_ID, &mut k_session)
        .map_err(|_| anyhow::anyhow!("HKDF expand failed"))?;
    let session_key = SessionKey(k_session);

    // ── Step 3: generate fresh Ed25519 iPad pubkey ────────────────────────
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let pubkey_bytes = verifying_key.to_bytes();

    // Generate a random 4-char suffix for the display name.
    let suffix = random_suffix();
    let display_name = format!("fake-ipad-{suffix}");

    // PCertReq plaintext = pubkey (32 bytes) || display_name (utf-8)
    let mut plaintext = Vec::with_capacity(32 + display_name.len());
    plaintext.extend_from_slice(&pubkey_bytes);
    plaintext.extend_from_slice(display_name.as_bytes());

    let aead_body = session_key.seal(2, &plaintext)?;
    send_msg(&mut stream, &PairMsg::new(PairKind::PCertReq, aead_body)).await?;
    println!("Sending device cert request...");
    info!("sent PCertReq display_name={display_name}");

    // ── Step 4: receive PCertOk (or handle EOF from stub daemon) ──────────
    match recv_msg(&mut stream).await {
        Ok(ok_msg) => {
            if ok_msg.kind == PairKind::PErr {
                bail!("daemon returned PErr: {:?}", ok_msg.body);
            }
            if ok_msg.kind != PairKind::PCertOk {
                bail!("expected PCertOk, got {:?}", ok_msg.kind);
            }
            // PCertOk body = AES-GCM seal of host_pubkey (32 bytes) || pair_id (utf-8).
            let plain = session_key.open(3, &ok_msg.body)?;
            if plain.len() < 32 {
                bail!("PCertOk plaintext too short");
            }
            let host_pubkey = &plain[..32];
            let pair_id_bytes = &plain[32..];
            let pair_id =
                String::from_utf8(pair_id_bytes.to_vec()).unwrap_or_else(|_| "unknown".into());

            let record = PairRecord {
                pair_id: pair_id.clone(),
                host_pubkey_b64: B64.encode(host_pubkey),
                display_name: display_name.clone(),
                paired_at_unix: now_unix(),
            };
            save_pair_record(&record)?;
            println!(
                "Paired! pair_id={pair_id}, host pubkey saved to {}",
                crate::last_pair_path().display()
            );
        }
        Err(e) => {
            // The current daemon stub closes the socket after PResponse.
            // Treat EOF gracefully: save what we have with an empty pair_id.
            let is_eof = e
                .downcast_ref::<std::io::Error>()
                .map(|io| {
                    matches!(
                        io.kind(),
                        std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionReset
                    )
                })
                .unwrap_or(false);

            if is_eof {
                let record = PairRecord {
                    pair_id: String::new(),
                    host_pubkey_b64: String::new(),
                    display_name: display_name.clone(),
                    paired_at_unix: now_unix(),
                };
                save_pair_record(&record)?;
                println!("Daemon closed connection after PResponse (stub mode).");
                println!(
                    "Partial record saved to {}",
                    crate::last_pair_path().display()
                );
            } else {
                return Err(e);
            }
        }
    }

    Ok(())
}

fn random_suffix() -> String {
    use rand_core::RngCore;
    let n = OsRng.next_u32() % 10_000;
    format!("{n:04}")
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
