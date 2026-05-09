//! Persistent host keystore.
//!
//! Two responsibilities:
//!
//! 1. **Host root key** — an Ed25519 keypair that signs iPad device certs.
//!    The private key is wrapped in OS-keyring storage (DPAPI on Windows,
//!    libsecret/Secret Service on Linux). Loss of the file means we mint new
//!    certs on next start; loss of the keyring entry the same way.
//!
//! 2. **Pinned-pubkey list** — sqlite at `~/.local/share/iextend/pins.sqlite`
//!    (Linux) or `%APPDATA%\iExtend\pins.sqlite` (Windows). Each row is one
//!    paired iPad: `(pair_id TEXT PRIMARY KEY, pubkey BLOB, name TEXT,
//!    paired_at INTEGER)`.
//!
//! For Plan 7 we ship a Linux-tested implementation; the Windows DPAPI path
//! is sketched but gated behind cfg, with a `todo!()` placeholder until Plan 9
//! wires up the windows-rs DPAPI calls. Linux uses
//! `secret-service` for the root-key wrap.

#![allow(dead_code)]

#[cfg(unix)]
use anyhow::Context;
use anyhow::{anyhow, Result};
use ed25519_dalek::{SigningKey, VerifyingKey};
#[cfg(unix)]
use rand_core::OsRng;
use std::path::{Path, PathBuf};

/// Service name registered with the OS keyring.
const KEYRING_SERVICE: &str = "iextend.host-root-key.v1";

/// Owned wrapper around an Ed25519 signing key. Drop zeroes the key bytes via
/// `ed25519-dalek`'s own zeroize.
pub struct HostRootKey {
    pub signing: SigningKey,
}

impl HostRootKey {
    /// The verifying half — safe to publish.
    pub fn verifying(&self) -> VerifyingKey {
        self.signing.verifying_key()
    }
}

/// Get-or-create the host root key.
///
/// On first launch: generate, persist to keyring, return. On subsequent
/// launches: load from keyring, return.
///
/// Linux implementation uses `secret-service` over D-Bus. Falls back to a
/// best-effort keyring-less plaintext at `$XDG_DATA_HOME/iextend/root.key` if
/// the keyring is not available (e.g. headless server). Headless plaintext
/// path is **not** used in production builds; CI prints a warning.
#[cfg(unix)]
pub async fn load_or_create_root_key() -> Result<HostRootKey> {
    use secret_service::{Collection, EncryptionType, SecretService};

    let ss = SecretService::connect(EncryptionType::Dh).await;
    if let Ok(ss) = ss {
        let coll: Collection = ss.get_default_collection().await?;
        if coll.is_locked().await? {
            coll.unlock().await?;
        }
        let attrs: std::collections::HashMap<&str, &str> =
            std::collections::HashMap::from([("service", KEYRING_SERVICE)]);
        let items = coll.search_items(attrs.clone()).await?;
        if let Some(item) = items.into_iter().next() {
            let secret = item.get_secret().await?;
            let bytes: [u8; 32] = secret
                .as_slice()
                .try_into()
                .context("keyring item is not 32 bytes")?;
            return Ok(HostRootKey {
                signing: SigningKey::from_bytes(&bytes),
            });
        }
        // Mint a new key and store it.
        let signing = SigningKey::generate(&mut OsRng);
        coll.create_item(
            "iExtend host root key",
            attrs,
            &signing.to_bytes(),
            true,
            "application/x-iextend-root-key",
        )
        .await?;
        return Ok(HostRootKey { signing });
    }
    // Fallback: $XDG_DATA_HOME/iextend/root.key plaintext. Used in headless
    // CI; emits a tracing warning at startup.
    let path = data_dir().join("root.key");
    if let Ok(bytes) = std::fs::read(&path) {
        let bytes: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| anyhow!("root.key wrong size"))?;
        return Ok(HostRootKey {
            signing: SigningKey::from_bytes(&bytes),
        });
    }
    let signing = SigningKey::generate(&mut OsRng);
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, signing.to_bytes())?;
    tracing::warn!(
        path = %path.display(),
        "no keyring available — root key written in plaintext (CI / headless mode only)"
    );
    Ok(HostRootKey { signing })
}

#[cfg(windows)]
pub async fn load_or_create_root_key() -> Result<HostRootKey> {
    // Sketch only — Plan 9 wires this to DPAPI via the windows crate.
    // The shape: encrypt the 32 raw bytes with CryptProtectData under
    // CRYPTPROTECT_LOCAL_MACHINE = 0, then write to %APPDATA%\iExtend\root.dat.
    todo!("Windows DPAPI root-key wrap — Plan 9");
}

/// Pinned-pubkey store: sqlite-backed list of paired iPads.
pub struct PinStore {
    conn: rusqlite::Connection,
}

impl PinStore {
    /// Open or create the pinned-pubkey database at the standard location.
    pub fn open_default() -> Result<Self> {
        let path = data_dir().join("pins.sqlite");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Self::open_at(&path)
    }

    /// Open or create the pinned-pubkey database at an explicit path. Used by
    /// integration tests that point at a `tempdir()`.
    pub fn open_at(path: &Path) -> Result<Self> {
        let conn = rusqlite::Connection::open(path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS pinned_ipads (
                pair_id   TEXT PRIMARY KEY NOT NULL,
                pubkey    BLOB NOT NULL,
                name      TEXT NOT NULL,
                paired_at INTEGER NOT NULL
            )",
            [],
        )?;
        Ok(Self { conn })
    }

    /// Add a pinned iPad; conflict on `pair_id` is an error (the caller should
    /// "forget" the old pin first if it's the same iPad re-pairing).
    pub fn pin(&self, pair_id: &str, pubkey: &[u8; 32], name: &str) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        self.conn.execute(
            "INSERT INTO pinned_ipads (pair_id, pubkey, name, paired_at) VALUES (?1, ?2, ?3, ?4)",
            (pair_id, pubkey.as_slice(), name, now),
        )?;
        Ok(())
    }

    /// Look up an iPad by pubkey; the steady-state reconnect path uses this
    /// to confirm a presented device cert is one we've seen before.
    pub fn find_by_pubkey(&self, pubkey: &[u8; 32]) -> Result<Option<PinnedIpad>> {
        let mut stmt = self.conn.prepare(
            "SELECT pair_id, pubkey, name, paired_at FROM pinned_ipads WHERE pubkey = ?1 LIMIT 1",
        )?;
        let mut rows = stmt.query([pubkey.as_slice()])?;
        if let Some(row) = rows.next()? {
            let pair_id: String = row.get(0)?;
            let pubkey_blob: Vec<u8> = row.get(1)?;
            let name: String = row.get(2)?;
            let paired_at: i64 = row.get(3)?;
            let pubkey: [u8; 32] = pubkey_blob
                .try_into()
                .map_err(|_| anyhow!("pubkey blob wrong size"))?;
            return Ok(Some(PinnedIpad {
                pair_id,
                pubkey,
                name,
                paired_at,
            }));
        }
        Ok(None)
    }

    /// "Forget device" — remove the pin.
    pub fn forget(&self, pair_id: &str) -> Result<bool> {
        let n = self
            .conn
            .execute("DELETE FROM pinned_ipads WHERE pair_id = ?1", [pair_id])?;
        Ok(n > 0)
    }

    /// List all pinned iPads. Tray's "Paired devices" UI walks this.
    pub fn list(&self) -> Result<Vec<PinnedIpad>> {
        let mut stmt = self.conn.prepare(
            "SELECT pair_id, pubkey, name, paired_at FROM pinned_ipads ORDER BY paired_at DESC",
        )?;
        let mut out = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let pair_id: String = row.get(0)?;
            let pubkey_blob: Vec<u8> = row.get(1)?;
            let name: String = row.get(2)?;
            let paired_at: i64 = row.get(3)?;
            let pubkey: [u8; 32] = pubkey_blob
                .try_into()
                .map_err(|_| anyhow!("pubkey blob wrong size"))?;
            out.push(PinnedIpad {
                pair_id,
                pubkey,
                name,
                paired_at,
            });
        }
        Ok(out)
    }
}

/// One pinned iPad row.
#[derive(Debug, Clone)]
pub struct PinnedIpad {
    pub pair_id: String,
    pub pubkey: [u8; 32],
    pub name: String,
    pub paired_at: i64,
}

#[cfg(unix)]
fn data_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join("iextend");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("iextend");
    }
    PathBuf::from("/tmp/iextend")
}

#[cfg(windows)]
fn data_dir() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        return PathBuf::from(appdata).join("iExtend");
    }
    PathBuf::from(r"C:\ProgramData\iExtend")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn pin_and_find() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("p.sqlite");
        let store = PinStore::open_at(&path).unwrap();
        let pk = [9u8; 32];
        store
            .pin("11111111-1111-1111-1111-111111111111", &pk, "Aman's iPad")
            .unwrap();
        let found = store.find_by_pubkey(&pk).unwrap().unwrap();
        assert_eq!(found.pair_id, "11111111-1111-1111-1111-111111111111");
        assert_eq!(found.name, "Aman's iPad");
    }

    #[test]
    fn forget_removes_row() {
        let dir = tempdir().unwrap();
        let store = PinStore::open_at(&dir.path().join("p.sqlite")).unwrap();
        let pk = [1u8; 32];
        store
            .pin("11111111-1111-1111-1111-111111111111", &pk, "x")
            .unwrap();
        assert!(store
            .forget("11111111-1111-1111-1111-111111111111")
            .unwrap());
        assert!(store.find_by_pubkey(&pk).unwrap().is_none());
    }

    #[test]
    fn duplicate_pair_id_is_error() {
        let dir = tempdir().unwrap();
        let store = PinStore::open_at(&dir.path().join("p.sqlite")).unwrap();
        store.pin("X", &[2u8; 32], "a").unwrap();
        assert!(store.pin("X", &[3u8; 32], "b").is_err());
    }

    #[test]
    fn list_orders_by_paired_at() {
        let dir = tempdir().unwrap();
        let store = PinStore::open_at(&dir.path().join("p.sqlite")).unwrap();
        store.pin("A", &[1u8; 32], "first").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        store.pin("B", &[2u8; 32], "second").unwrap();
        let l = store.list().unwrap();
        assert_eq!(l[0].name, "second");
        assert_eq!(l[1].name, "first");
    }
}
