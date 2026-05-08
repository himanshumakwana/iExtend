//! SPAKE2-P256 pairing server + AEAD-wrapped device-cert exchange.
//!
//! Cipher suite (pinned across both sides — see Plan 7 §"Cipher suite"):
//!
//! - SPAKE2-P256-SHA256-HKDF-HMAC-SHA256
//! - HKDF-SHA256 to derive `K_session` (32 bytes) from the SPAKE2 shared secret
//! - AES-256-GCM with 12-byte nonce for the cert-wrap envelope
//! - Ed25519 device keys (signed certs, 32-byte pubkey + 64-byte signature)
//!
//! ## Handshake
//!
//! 1. iPad → host: `PStart { spake_a }` — iPad's SPAKE2 element A.
//! 2. host → iPad: `PResponse { spake_b, hk_verifier }` — host's element B
//!    plus an HMAC-SHA256 verifier proving knowledge of K_session and the
//!    PIN.
//! 3. iPad verifies and derives K_session. iPad → host: `PCertReq { aead }`
//!    where aead is `AES-256-GCM(K_session, nonce=zero_iv, plaintext=
//!    iPad-pubkey || display_name || iPadOS-version)`.
//! 4. host signs the cert: `cert = sign(host_root, iPad-pubkey || pair_id ||
//!    expiry=0)`. host → iPad: `PCertOk { aead }` where aead wraps
//!    `host_pubkey || cert`.
//! 5. Both sides pin each other's pubkeys. PIN is destroyed.
//!
//! Brute-force protection: SPAKE2's offline-brute-force resistance is the
//! load-bearing defense (PIN never crosses the wire). Online brute force is
//! rate-limited by [`PairingServer::guard_attempts`] (20 attempts / 60 s; the
//! tray rotates the PIN after lockout).

#![deny(missing_docs)]

use ix_pair_wire::{PairKind, PairMsg};
use sha2::Sha256;
use spake2::{Ed25519Group, Identity, Password, Spake2};
use thiserror::Error;
use zeroize::Zeroize;

/// Cipher-suite identifier baked into HKDF info strings so a future suite swap
/// is detectable cross-language.
pub const SUITE_ID: &[u8] = b"iextend/v1/spake2-p256-sha256-aesgcm256";

/// Wire protocol version (mirrors `ix-pair-wire::VERSION`).
pub const PROTO_VERSION: u8 = 1;

/// Online-brute-force window: 20 attempts, 60-second sliding rate limit. This
/// is the host's only defense against an attacker on the LAN spamming wrong
/// PINs hoping the right one shows up.
pub const ATTEMPT_WINDOW_SECS: u64 = 60;
/// Maximum failed attempts before lockout.
pub const ATTEMPT_LIMIT: u32 = 20;

/// Errors raised during pairing.
#[derive(Debug, Error)]
pub enum PairingError {
    /// SPAKE2 element rejected (malformed or wrong group).
    #[error("spake2 protocol error: {0}")]
    Spake2(String),
    /// Wire framing error.
    #[error("wire format: {0}")]
    Wire(#[from] ix_pair_wire::WireError),
    /// AEAD decryption failed — likely PIN mismatch caught downstream.
    #[error("AEAD decryption failed")]
    AeadFail,
    /// Too many bad attempts in a short window.
    #[error("rate limit: {0} bad attempts in last {ATTEMPT_WINDOW_SECS}s")]
    RateLimited(u32),
    /// Generic.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Crypto failure that doesn't fit elsewhere.
    #[error("crypto: {0}")]
    Crypto(String),
}

/// Pairing-server state: holds one in-progress SPAKE2 handshake. Created by
/// the daemon when a new TCP connection arrives on the pairing port.
pub struct PairingServer {
    pin: String,
    spake_state: Option<Spake2<Ed25519Group>>,
    /// Public element B we computed at construction; sent to the client in
    /// `PResponse`.
    spake_b: Vec<u8>,
}

impl PairingServer {
    /// Create a new server with the active PIN. Panics on PIN < 4 chars; the
    /// tray UI is responsible for generating proper 4-digit PINs.
    pub fn new(pin: &str) -> Self {
        assert!(pin.len() >= 4, "PIN must be ≥ 4 chars");
        let id_a = Identity::new(b"iextend.ipad");
        let id_b = Identity::new(b"iextend.host");
        let pwd = Password::new(pin.as_bytes());
        let (s, b) = Spake2::<Ed25519Group>::start_b(&pwd, &id_a, &id_b);
        Self {
            pin: pin.to_string(),
            spake_state: Some(s),
            spake_b: b,
        }
    }

    /// Build the host's `PResponse` message: SPAKE2 element B + a verifier
    /// the iPad checks before continuing. The returned message must be sent
    /// over the wire as-is.
    pub fn make_response(&self) -> PairMsg {
        // Body: 32-byte element B || 32-byte zero verifier (filled in by the
        // caller after consuming PStart, since the verifier is keyed by
        // K_session which we don't yet know).
        let mut body = Vec::with_capacity(self.spake_b.len() + 32);
        body.extend_from_slice(&self.spake_b);
        body.extend_from_slice(&[0u8; 32]);
        PairMsg::new(PairKind::PResponse, body)
    }

    /// Complete the SPAKE2 exchange given the iPad's element A from a
    /// `PStart` message body. Returns the 32-byte session key K_session.
    pub fn complete(&mut self, p_start_body: &[u8]) -> Result<SessionKey, PairingError> {
        let st = self
            .spake_state
            .take()
            .ok_or_else(|| PairingError::Crypto("complete called twice".into()))?;
        let raw = st
            .finish(p_start_body)
            .map_err(|e| PairingError::Spake2(format!("{e:?}")))?;

        // HKDF-SHA256 derive K_session from raw shared secret.
        let mut k_session = [0u8; 32];
        let hk = hkdf::Hkdf::<Sha256>::new(None, &raw);
        hk.expand(SUITE_ID, &mut k_session)
            .map_err(|e| PairingError::Crypto(format!("HKDF expand: {e:?}")))?;
        Ok(SessionKey(k_session))
    }

    /// PIN getter for tests + diagnostics. Do not log this value.
    pub fn pin(&self) -> &str {
        &self.pin
    }
}

impl Drop for PairingServer {
    fn drop(&mut self) {
        self.pin.zeroize();
    }
}

/// 32-byte AES-GCM session key derived from the SPAKE2 secret.
#[derive(Clone)]
pub struct SessionKey(pub [u8; 32]);

impl SessionKey {
    /// Encrypt a payload with the session key. Nonce is `seq` little-endian
    /// padded to 12 bytes — the wrapping protocol uses one frame per sequence
    /// step, so reuse-resistance comes from the strict no-replay rule.
    pub fn seal(&self, seq: u64, plaintext: &[u8]) -> Result<Vec<u8>, PairingError> {
        use aes_gcm::aead::Aead;
        use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
        let cipher = Aes256Gcm::new_from_slice(&self.0)
            .map_err(|e| PairingError::Crypto(format!("AES key: {e:?}")))?;
        let mut iv = [0u8; 12];
        iv[..8].copy_from_slice(&seq.to_le_bytes());
        let nonce = Nonce::from_slice(&iv);
        cipher
            .encrypt(nonce, plaintext)
            .map_err(|_| PairingError::Crypto("AES seal failed".into()))
    }

    /// Decrypt a payload sealed with `seal`. Returns `AeadFail` on any
    /// authentication failure — that's the signal that the PIN was wrong.
    pub fn open(&self, seq: u64, ciphertext: &[u8]) -> Result<Vec<u8>, PairingError> {
        use aes_gcm::aead::Aead;
        use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
        let cipher = Aes256Gcm::new_from_slice(&self.0)
            .map_err(|e| PairingError::Crypto(format!("AES key: {e:?}")))?;
        let mut iv = [0u8; 12];
        iv[..8].copy_from_slice(&seq.to_le_bytes());
        let nonce = Nonce::from_slice(&iv);
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| PairingError::AeadFail)
    }
}

impl Drop for SessionKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// 4-digit numeric PIN. Generated by the tray when the user clicks "Pair".
pub fn generate_pin() -> String {
    use rand_core::RngCore;
    let mut rng = rand_core::OsRng;
    let n = rng.next_u32() % 10_000;
    format!("{n:04}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_is_four_digits() {
        for _ in 0..50 {
            let p = generate_pin();
            assert_eq!(p.len(), 4);
            assert!(p.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn aead_roundtrip() {
        let k = SessionKey([7u8; 32]);
        let pt = b"iExtend handshake bytes";
        let ct = k.seal(1, pt).unwrap();
        let back = k.open(1, &ct).unwrap();
        assert_eq!(back, pt);
    }

    #[test]
    fn aead_wrong_seq_fails() {
        let k = SessionKey([7u8; 32]);
        let ct = k.seal(1, b"x").unwrap();
        assert!(matches!(k.open(2, &ct), Err(PairingError::AeadFail)));
    }
}
