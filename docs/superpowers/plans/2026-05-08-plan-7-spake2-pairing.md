# SPAKE2 Pairing & mDNS Discovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first-pair flow (4-digit PIN + SPAKE2 PAKE → device cert exchange → mutual pubkey pinning) and the steady-state cert-based reconnect, on both host (Rust) and iPad (Swift). Includes the `ix-discover` mDNS crate and the keystore subsystem. After this plan, an iPad and a host can pair once over a coffee-shop-hostile LAN, and reconnect on every subsequent session without re-entering the PIN.

**Architecture:** Pairing runs over a temporary plain TCP control connection (port 5353) using a fixed-size framed wire format (`PairMsg`). SPAKE2 derives a one-shot AEAD key (`K_pair`) that wraps the cert-exchange step. Once both sides have pinned each other's Ed25519 pubkeys, the temporary TCP closes; from then on, every session uses a cert-based DTLS handshake at the WebRTC layer (configured from `ix-rtc`'s existing `RTCCertificate` slot, populated from the pinned device cert). mDNS discovery uses `mdns-sd` on the host and `NWBrowser` from Network.framework on the iPad; the TXT record carries protocol version, host pubkey (base64), and post-pairing pair-id.

**Tech Stack:**
- **Host:** Rust 1.79+, `spake2 = "0.4"` (RustCrypto, RFC 9382), `ed25519-dalek = "2"`, `mdns-sd = "0.11"`, `aes-gcm = "0.10"` (AEAD for cert wrap), `rusqlite = "0.31"` (pinned-pubkey store), `keyring = "3"` for OS keystore (DPAPI on Windows, Secret Service on Linux), `tokio = "1"`, `egui` 0.28 (tray UI).
- **iPad:** Swift 5.10+, `swift-crypto` 3.x (provides SPAKE2; matches RFC 9382 vectors), `Network.framework` (NWBrowser, NWConnection, AEAD primitives), `Security` framework (Keychain), iPadOS 17+.
- **Wire-shared:** small Rust crate `ix-pair-wire` with `serde`-free hand-rolled `PairMsg` codec; mirrored by hand in `PairWire.swift`.

**Plan scope:** This is **Plan 7 of 10**. **Depends on:**
- Plan 2 — cargo workspace under `host/`, `iextendd` and `iextend-tray` binaries scaffolded, `ix-rtc` crate exists with a `PeerConnection` struct.
- Plan 5 — `ix-rtc` knows how to take an `RTCCertificate` and run a cert-pinned DTLS handshake; SDP offer/answer happens through a transport hook (this plan supplies the *post-pair* hook).
- Plan 6 — iPad app shell (`iExtend.app`, `iExtendKit`, `iExtendUI`) exists. `PairingFlow.swift` is a stub file; `Onboarding/PairView.swift` already lays out the pin-entry UI but its callback is wired to a placeholder. This plan fills both in.

**Out of scope (handled by other plans):**
- WebRTC / SDP offer-answer flow (Plan 5)
- Encoder negotiation (Plan 5)
- Input forwarding & cursor reprojection (Plan 8)
- Installer & code-signing (Plan 9)
- Bench rig (Plan 10)

**Honest risk note:** SPAKE2 is well-specified, but cross-language interop between RustCrypto and swift-crypto is the real failure mode. Both must agree on **exactly** the same group, hash function, M/N points (RFC 9382 §4), and PIN-encoding rule. We pin to **SPAKE2-P256-SHA256-HKDF-HMAC-SHA256** (the RFC 9382 mandatory-to-implement suite), encode the PIN as the 4-byte big-endian ASCII representation (e.g. `"4729"` → `0x34 0x37 0x32 0x39`), and verify against the test vectors from RFC 9382 §C.1 in **both** Rust and Swift unit tests as a CI gate. If one side disagrees with the vector, the build fails.

---

## File Structure

```
iExtend/
├── host/
│   └── crates/
│       ├── ix-pair-wire/                       # NEW — shared frame codec
│       │   ├── Cargo.toml
│       │   └── src/
│       │       ├── lib.rs                      # PairMsg type + read/write
│       │       └── kinds.rs                    # PSTART, PRESPONSE, PCERT_REQ, PCERT_OK, PERR
│       ├── ix-discover/                        # NEW — mDNS
│       │   ├── Cargo.toml
│       │   └── src/
│       │       ├── lib.rs                      # public Browser, Advertiser, Found event
│       │       ├── advertise.rs                # _iextend._tcp + TXT record
│       │       └── browse.rs                   # browse + service-record parse
│       ├── ix-rtc/
│       │   └── src/
│       │       ├── pairing.rs                  # NEW — server-side SPAKE2 handshake
│       │       └── reconnect.rs                # NEW — cert-pinned DTLS reconnect helper
│       └── iextendd/
│           └── src/
│               ├── keystore.rs                 # NEW — OS keystore + sqlite pinned list
│               ├── pair_listener.rs            # NEW — TCP :5353 server task
│               └── main.rs                     # MODIFIED — wire keystore + pair_listener
├── host/
│   └── crates/
│       └── iextend-tray/
│           └── src/
│               ├── pair_screen.rs              # NEW — egui PIN-display screen
│               └── main.rs                     # MODIFIED — add Pair tab
├── ipad/
│   └── iExtendKit/
│       └── Sources/
│           └── iExtendKit/
│               ├── Connection/
│               │   ├── PairingFlow.swift       # FILL — SPAKE2 client + PairMsg I/O
│               │   ├── PairWire.swift          # NEW — Swift mirror of PairMsg
│               │   ├── Keychain.swift          # NEW — pinned host pubkey
│               │   └── Reconnect.swift         # NEW — cert-presented DTLS at WebRTC layer
│               └── Discovery/
│                   └── Browser.swift           # NEW — NWBrowser wrapper, parses TXT
│   └── iExtendUI/
│       └── Sources/
│           └── iExtendUI/
│               └── Onboarding/
│                   └── PinEntryView.swift      # FILL — submit/error states
└── tests/
    └── interop/                                # NEW — host↔iPad simulator interop
        ├── Cargo.toml                          # Rust SPAKE2 server harness
        ├── src/
        │   └── server_main.rs                  # spawns server, accepts one pairing
        ├── ipad-sim-driver/                    # xcrun simctl wrapper script
        │   └── run.sh
        └── README.md                           # how to run cross-language interop locally
```

**Single-responsibility rule:** every file targets one concern and stays under 300 lines. The pairing handshake state machine lives in `ix-rtc/src/pairing.rs` (host) and `PairingFlow.swift` (iPad); both encode the same five-state machine (`Start`, `AwaitClient`, `AwaitCertReq`, `IssueCert`, `Done`/`Error`).

---

## Cipher suite — pinned across both sides

| Parameter | Value | Source |
|---|---|---|
| SPAKE2 group | NIST P-256 | RFC 9382 §4 |
| SPAKE2 M | `02886e2f97ace46e55ba9dd7242579f2993b64e16ef3dcab95afd497333d8fa12f` | RFC 9382 §4 |
| SPAKE2 N | `03d8bbd6c639c62937b04d997f38c3770719c629d7014d49a24b4f98baa1292b49` | RFC 9382 §4 |
| Hash | SHA-256 | RFC 9382 §3 |
| KDF | HKDF-SHA256 | RFC 9382 §3 |
| MAC | HMAC-SHA256 | RFC 9382 §3 |
| AEAD (cert wrap) | AES-256-GCM | NIST SP 800-38D |
| Device key type | Ed25519 | RFC 8032 |
| Cert sig | Ed25519 over a custom CBOR struct | RFC 8949 |
| PIN encoding | 4-byte ASCII big-endian | local convention |

The exact byte values for M and N must appear as named constants in both `ix-pair-wire/src/lib.rs` and `PairWire.swift`. **A unit test on each side asserts these equal RFC 9382 §C.1 vector inputs**; if a future RustCrypto/swift-crypto release alters defaults, the test fires.

---

## Wire format (`PairMsg`)

```
0           4         5         6         8                       8+len
+-----------+---------+---------+---------+-----------------------+
| magic(4)  | ver(1)  | kind(1) | len(2)  |   body (len bytes)    |
+-----------+---------+---------+---------+-----------------------+
   "IXPD"      1                  big-endian u16
```

- `magic` = `0x49585044` (`b"IXPD"`).
- `ver` = `1` (this plan).
- `kind` (one byte): `0x01 PSTART`, `0x02 PRESPONSE`, `0x03 PCERT_REQ`, `0x04 PCERT_OK`, `0xFF PERR`.
- `len` ≤ 4096; the recipient closes the connection on overflow.
- Bodies are length-prefixed concatenated fields documented per-kind in `ix-pair-wire/src/kinds.rs` (no JSON, no protobuf; hand-rolled to keep parse paths simple and fuzzed).

**State machine (both sides agree):**

```
iPad (client)                                         host (server)
   │                                                       │
   │  PSTART {iPad-ephemeral-pubkey, device-name-utf8}     │
   │  ─────────────────────────────────────────────────►  │
   │                                                       │
   │  PRESPONSE {host-ephemeral-pubkey, host-name-utf8,    │
   │             host-confirm-mac}                         │
   │  ◄─────────────────────────────────────────────────  │
   │                                                       │
   │  (each side derives K_pair via SPAKE2 + HKDF;         │
   │   each verifies the other's confirm-MAC; abort on     │
   │   mismatch)                                           │
   │                                                       │
   │  PCERT_REQ aead_seal(K_pair, {                        │
   │     ed25519_pub: 32B,                                 │
   │     device_name: utf8 ≤ 64B,                          │
   │     ipados_version: u32,                              │
   │     nonce: 12B })                                     │
   │  ─────────────────────────────────────────────────►  │
   │                                                       │
   │  PCERT_OK  aead_seal(K_pair, {                        │
   │     cert_cbor: bytes,        // Ed25519-signed CBOR   │
   │     host_pub: 32B,                                    │
   │     pair_id: 16B random })                            │
   │  ◄─────────────────────────────────────────────────  │
   │                                                       │
   │  TCP closes.  WebRTC takes over from here.            │
```

`PERR` is only sent on protocol-level errors (bad magic, bad MAC, AEAD fail, version mismatch). Pin verification, brute-force detection, etc., produce `PERR` with a small numeric reason code so logs are useful.

---

### Task 1: Bootstrap the `ix-pair-wire` crate

**Files:**
- Create: `host/crates/ix-pair-wire/Cargo.toml`
- Create: `host/crates/ix-pair-wire/src/lib.rs`
- Create: `host/crates/ix-pair-wire/src/kinds.rs`
- Modify: `host/Cargo.toml` (workspace `members += ["crates/ix-pair-wire"]`)

- [ ] **Step 1: Add the crate to the workspace**

Append to `host/Cargo.toml`:
```toml
[workspace]
members = [..., "crates/ix-pair-wire"]
```

- [ ] **Step 2: Write `host/crates/ix-pair-wire/Cargo.toml`**

```toml
[package]
name = "ix-pair-wire"
version = "0.1.0"
edition = "2021"

[dependencies]
thiserror = "1"
zeroize = { version = "1", features = ["zeroize_derive"] }

[dev-dependencies]
proptest = "1"
```

- [ ] **Step 3: Write `src/kinds.rs`**

```rust
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Kind {
    PStart    = 0x01,
    PResponse = 0x02,
    PCertReq  = 0x03,
    PCertOk   = 0x04,
    PErr      = 0xFF,
}

impl TryFrom<u8> for Kind {
    type Error = crate::Error;
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        Ok(match v {
            0x01 => Self::PStart,
            0x02 => Self::PResponse,
            0x03 => Self::PCertReq,
            0x04 => Self::PCertOk,
            0xFF => Self::PErr,
            other => return Err(crate::Error::UnknownKind(other)),
        })
    }
}

pub const MAGIC: u32 = 0x4958_5044; // "IXPD"
pub const PROTOCOL_VERSION: u8 = 1;
pub const MAX_BODY_LEN: usize = 4096;
```

- [ ] **Step 4: Write `src/lib.rs`**

```rust
mod kinds;
pub use kinds::*;

use std::io::{Read, Write};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("io: {0}")] Io(#[from] std::io::Error),
    #[error("bad magic: 0x{0:08x}")]   BadMagic(u32),
    #[error("unsupported version: {0}")] BadVersion(u8),
    #[error("unknown kind: 0x{0:02x}")]  UnknownKind(u8),
    #[error("body too large: {0}")]      TooLarge(usize),
}

#[derive(Debug, zeroize::ZeroizeOnDrop)]
pub struct PairMsg {
    #[zeroize(skip)] pub kind: Kind,
    pub body: Vec<u8>,
}

impl PairMsg {
    pub fn write_to<W: Write>(&self, mut w: W) -> Result<(), Error> {
        if self.body.len() > MAX_BODY_LEN { return Err(Error::TooLarge(self.body.len())); }
        w.write_all(&MAGIC.to_be_bytes())?;
        w.write_all(&[PROTOCOL_VERSION, self.kind as u8])?;
        w.write_all(&(self.body.len() as u16).to_be_bytes())?;
        w.write_all(&self.body)?;
        Ok(())
    }

    pub fn read_from<R: Read>(mut r: R) -> Result<Self, Error> {
        let mut header = [0u8; 8];
        r.read_exact(&mut header)?;
        let magic = u32::from_be_bytes(header[..4].try_into().unwrap());
        if magic != MAGIC { return Err(Error::BadMagic(magic)); }
        if header[4] != PROTOCOL_VERSION { return Err(Error::BadVersion(header[4])); }
        let kind = Kind::try_from(header[5])?;
        let len = u16::from_be_bytes(header[6..8].try_into().unwrap()) as usize;
        if len > MAX_BODY_LEN { return Err(Error::TooLarge(len)); }
        let mut body = vec![0u8; len];
        r.read_exact(&mut body)?;
        Ok(Self { kind, body })
    }
}
```

- [ ] **Step 5: Round-trip + fuzz tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn roundtrip(body in proptest::collection::vec(any::<u8>(), 0..MAX_BODY_LEN)) {
            let msg = PairMsg { kind: Kind::PStart, body };
            let mut buf = Vec::new();
            msg.write_to(&mut buf).unwrap();
            let back = PairMsg::read_from(&buf[..]).unwrap();
            prop_assert_eq!(msg.kind, back.kind);
            prop_assert_eq!(msg.body, back.body);
        }
    }

    #[test]
    fn rejects_bad_magic() {
        let buf = [0u8; 16];
        assert!(matches!(PairMsg::read_from(&buf[..]), Err(Error::BadMagic(_))));
    }

    #[test]
    fn rejects_oversize() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&MAGIC.to_be_bytes());
        buf.extend_from_slice(&[1, 1, 0xFF, 0xFF]); // len=65535
        buf.extend(std::iter::repeat(0).take(65535));
        assert!(matches!(PairMsg::read_from(&buf[..]), Err(Error::TooLarge(_))));
    }
}
```

- [ ] **Step 6: Build + test + commit**

```bash
cd /home/tops/Projects/iExtend/host
cargo build -p ix-pair-wire
cargo test -p ix-pair-wire
git add Cargo.toml crates/ix-pair-wire
git commit -m "feat(ix-pair-wire): add framed PairMsg codec with proptest roundtrip"
```

Expected: builds clean; `cargo test -p ix-pair-wire` shows ≥ 3 passing (proptest counts as 1).

---

### Task 2: `PairWire.swift` — mirror the wire format on iPad

**Files:**
- Create: `ipad/iExtendKit/Sources/iExtendKit/Connection/PairWire.swift`
- Modify: `ipad/iExtendKit/Tests/iExtendKitTests/PairWireTests.swift` (new file too)

- [ ] **Step 1: Write `PairWire.swift`**

```swift
import Foundation

public enum PairKind: UInt8 {
    case pStart    = 0x01
    case pResponse = 0x02
    case pCertReq  = 0x03
    case pCertOk   = 0x04
    case pErr      = 0xFF
}

public enum PairWireError: Error, Equatable {
    case badMagic(UInt32)
    case badVersion(UInt8)
    case unknownKind(UInt8)
    case tooLarge(Int)
    case truncated
}

public struct PairMsg {
    public static let magic: UInt32 = 0x4958_5044   // "IXPD"
    public static let protocolVersion: UInt8 = 1
    public static let maxBodyLen = 4096

    public let kind: PairKind
    public let body: Data

    public func encoded() -> Data {
        var out = Data(capacity: 8 + body.count)
        var m = Self.magic.bigEndian; withUnsafeBytes(of: &m) { out.append(contentsOf: $0) }
        out.append(Self.protocolVersion)
        out.append(kind.rawValue)
        var l = UInt16(body.count).bigEndian
        withUnsafeBytes(of: &l) { out.append(contentsOf: $0) }
        out.append(body)
        return out
    }

    public static func decode(_ data: Data) throws -> (PairMsg, Int) {
        guard data.count >= 8 else { throw PairWireError.truncated }
        let magic = data.prefix(4).withUnsafeBytes { $0.load(as: UInt32.self).bigEndian }
        guard magic == Self.magic else { throw PairWireError.badMagic(magic) }
        guard data[4] == Self.protocolVersion else { throw PairWireError.badVersion(data[4]) }
        guard let kind = PairKind(rawValue: data[5]) else { throw PairWireError.unknownKind(data[5]) }
        let len = Int(data[6]) << 8 | Int(data[7])
        guard len <= Self.maxBodyLen else { throw PairWireError.tooLarge(len) }
        guard data.count >= 8 + len else { throw PairWireError.truncated }
        return (PairMsg(kind: kind, body: data.subdata(in: 8..<8+len)), 8 + len)
    }
}
```

- [ ] **Step 2: Write `PairWireTests.swift`**

```swift
import XCTest
@testable import iExtendKit

final class PairWireTests: XCTestCase {
    func testRoundtrip() throws {
        let msg = PairMsg(kind: .pStart, body: Data([1,2,3,4]))
        let bytes = msg.encoded()
        let (back, n) = try PairMsg.decode(bytes)
        XCTAssertEqual(n, bytes.count)
        XCTAssertEqual(back.kind, .pStart)
        XCTAssertEqual(back.body, Data([1,2,3,4]))
    }

    func testRejectsBadMagic() {
        let bytes = Data([0,0,0,0, 1, 1, 0,0])
        XCTAssertThrowsError(try PairMsg.decode(bytes)) { e in
            guard case PairWireError.badMagic(_) = e else { return XCTFail() }
        }
    }
}
```

- [ ] **Step 3: Cross-language interop test fixture**

Add a Rust test that emits a known `PairMsg` to a hex string and a Swift test that decodes that exact hex; both assert the same `(kind, body)`. Hex literal lives in `tests/interop/fixtures/pair_msg_v1.hex`. This is the tripwire that catches "Rust changed and Swift didn't" before the SPAKE2 layer ever runs.

- [ ] **Step 4: Commit**

```bash
git add ipad/iExtendKit/Sources/iExtendKit/Connection/PairWire.swift \
        ipad/iExtendKit/Tests/iExtendKitTests/PairWireTests.swift \
        tests/interop/fixtures/pair_msg_v1.hex
git commit -m "feat(ipad): mirror PairMsg wire format in Swift; add interop fixture"
```

---

### Task 3: SPAKE2 vector tests (RFC 9382 §C.1) on both sides

**Files:**
- Create: `host/crates/ix-rtc/src/pairing/vectors.rs`
- Create: `ipad/iExtendKit/Tests/iExtendKitTests/SPAKE2VectorsTests.swift`

- [ ] **Step 1: Capture the RFC 9382 §C.1 P-256 vector**

The vector specifies (a, b, w0, w1, X, Y, Z, V, K). Hard-code these as hex strings in **both** test files. Source of truth: `https://www.rfc-editor.org/rfc/rfc9382#section-C.1`.

- [ ] **Step 2: Rust vector test**

```rust
#[test]
fn rfc9382_p256_c1() {
    use spake2::{Ed25519Group, Identity, Password, Spake2};
    // ... feed (w0, w1) into both halves with the fixed scalars from the RFC,
    //     assert derived K matches.
    // (The exact API surface of `spake2 = "0.4"` differs from the RFC's parameter
    //  names; the test driver loads `vectors.rs::RFC9382_P256_C1` and pins it.)
}
```

- [ ] **Step 3: Swift vector test**

```swift
func testRFC9382_P256_C1() throws {
    let v = SPAKE2Vector.rfc9382_p256_c1
    let derived = try SPAKE2.deriveK_static(w0: v.w0, w1: v.w1, X: v.X, Y: v.Y)
    XCTAssertEqual(derived.hex, v.K.hex)
}
```

- [ ] **Step 4: Make this a CI gate**

Both tests run on every host CI build and every iOS simulator CI build. If either fails, no other pairing-related test is allowed to run (set up via `#[ignore]` cascade in Rust and `XCTSkip` in Swift gated on a process-wide bool).

- [ ] **Step 5: Commit**

```bash
git add host/crates/ix-rtc/src/pairing/vectors.rs \
        ipad/iExtendKit/Tests/iExtendKitTests/SPAKE2VectorsTests.swift
git commit -m "test: RFC 9382 §C.1 SPAKE2 vector — pinned in Rust and Swift CI"
```

---

### Task 4: `ix-discover` — host-side mDNS advertise

**Files:**
- Create: `host/crates/ix-discover/Cargo.toml`
- Create: `host/crates/ix-discover/src/lib.rs`
- Create: `host/crates/ix-discover/src/advertise.rs`
- Create: `host/crates/ix-discover/src/browse.rs`

- [ ] **Step 1: Cargo.toml**

```toml
[package]
name = "ix-discover"
version = "0.1.0"
edition = "2021"

[dependencies]
mdns-sd = "0.11"
base64 = "0.22"
thiserror = "1"
tracing = "0.1"
```

- [ ] **Step 2: `advertise.rs`**

```rust
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::collections::HashMap;

pub struct Advertiser { handle: ServiceDaemon, fullname: String }

impl Advertiser {
    pub fn start(host_name: &str, host_pub_b64: &str, pair_id_b64: Option<&str>, port: u16)
        -> Result<Self, mdns_sd::Error>
    {
        let daemon = ServiceDaemon::new()?;
        let mut txt = HashMap::new();
        txt.insert("ver".into(), "1".into());
        txt.insert("hk".into(), host_pub_b64.into()); // host pubkey, base64
        if let Some(pid) = pair_id_b64 { txt.insert("pid".into(), pid.into()); }
        let svc = ServiceInfo::new(
            "_iextend._tcp.local.",
            host_name,
            &format!("{host_name}.local."),
            "",            // host's IP — daemon fills in
            port,
            txt,
        )?;
        let fullname = svc.get_fullname().to_string();
        daemon.register(svc)?;
        Ok(Self { handle: daemon, fullname })
    }

    pub fn stop(self) -> Result<(), mdns_sd::Error> { self.handle.unregister(&self.fullname).map(|_| ()) }
}
```

- [ ] **Step 3: `browse.rs`**

```rust
use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::time::Duration;
use tokio::sync::mpsc;

pub struct Found {
    pub host_name: String,
    pub addr: std::net::IpAddr,
    pub port: u16,
    pub host_pub_b64: Option<String>,
    pub pair_id_b64: Option<String>,
}

pub fn browse(timeout: Duration) -> Result<mpsc::Receiver<Found>, mdns_sd::Error> {
    let (tx, rx) = mpsc::channel(64);
    let daemon = ServiceDaemon::new()?;
    let receiver = daemon.browse("_iextend._tcp.local.")?;
    tokio::spawn(async move {
        let deadline = tokio::time::Instant::now() + timeout;
        while let Some(ev) = receiver.recv_timeout(deadline.saturating_duration_since(tokio::time::Instant::now())).ok() {
            if let ServiceEvent::ServiceResolved(info) = ev {
                let addr = match info.get_addresses().iter().next() { Some(&a) => a, None => continue };
                let txt = info.get_properties();
                let _ = tx.send(Found {
                    host_name: info.get_hostname().trim_end_matches('.').to_string(),
                    addr,
                    port: info.get_port(),
                    host_pub_b64: txt.get_property_val_str("hk").map(String::from),
                    pair_id_b64: txt.get_property_val_str("pid").map(String::from),
                }).await;
            }
        }
    });
    Ok(rx)
}
```

- [ ] **Step 4: `lib.rs`**

```rust
mod advertise;
mod browse;

pub use advertise::Advertiser;
pub use browse::{browse, Found};
```

- [ ] **Step 5: Local-loopback advertise/browse test**

Spawn an `Advertiser` in one tokio task; in a second, run `browse(2s)`; assert at least one `Found` matches the advertised host name and TXT fields. (Linux CI: requires `avahi-daemon` running.)

- [ ] **Step 6: Commit**

```bash
git add host/crates/ix-discover host/Cargo.toml
git commit -m "feat(ix-discover): mDNS advertise + browse with TXT (ver/hk/pid)"
```

---

### Task 5: `Browser.swift` — iPad-side `NWBrowser`

**Files:**
- Create: `ipad/iExtendKit/Sources/iExtendKit/Discovery/Browser.swift`
- Create: `ipad/iExtendKit/Tests/iExtendKitTests/BrowserTests.swift`

- [ ] **Step 1: Write the `Browser`**

```swift
import Network

public actor IXBrowser {
    public struct Found: Sendable, Equatable {
        public let name: String
        public let endpoint: NWEndpoint
        public let hostPubB64: String?
        public let pairIdB64: String?
    }

    private var browser: NWBrowser?
    private var continuation: AsyncStream<Found>.Continuation?

    public func stream() -> AsyncStream<Found> {
        AsyncStream { cont in
            self.continuation = cont
            let params = NWParameters()
            params.includePeerToPeer = true
            let b = NWBrowser(for: .bonjourWithTXTRecord(type: "_iextend._tcp", domain: nil), using: params)
            b.browseResultsChangedHandler = { results, _ in
                for r in results {
                    guard case .service(let name, _, _, _) = r.endpoint else { continue }
                    let txt = (try? r.metadata.txtRecord()) ?? NWTXTRecord()
                    cont.yield(Found(
                        name: name,
                        endpoint: r.endpoint,
                        hostPubB64: txt.getEntry(for: "hk")?.stringValue,
                        pairIdB64: txt.getEntry(for: "pid")?.stringValue
                    ))
                }
            }
            b.start(queue: .main)
            self.browser = b
            cont.onTermination = { _ in b.cancel() }
        }
    }
}
```

- [ ] **Step 2: Add the Bonjour service to Info.plist**

`NSBonjourServices = ["_iextend._tcp"]` and `NSLocalNetworkUsageDescription = "iExtend connects to your PC over Wi-Fi."`. iOS 14+ requires the latter or browse silently returns nothing. Do not skip.

- [ ] **Step 3: Loopback simulator test (manual / CI nightly)**

Stand up a Rust `Advertiser` on the dev mac; confirm the iPad simulator's browser yields it within 2 s. Document the steps in `tests/interop/README.md`.

- [ ] **Step 4: Commit**

```bash
git add ipad/iExtendKit/Sources/iExtendKit/Discovery/Browser.swift \
        ipad/iExtendKit/Tests/iExtendKitTests/BrowserTests.swift \
        ipad/iExtend/Info.plist
git commit -m "feat(ipad): NWBrowser-backed IXBrowser; Info.plist Bonjour entry"
```

---

### Task 6: Host keystore — root key + pinned-pubkey store

**Files:**
- Create: `host/crates/iextendd/src/keystore.rs`
- Modify: `host/crates/iextendd/Cargo.toml` (add `keyring`, `rusqlite`, `ed25519-dalek`)

- [ ] **Step 1: Cargo.toml deps**

```toml
[dependencies]
keyring = "3"
rusqlite = { version = "0.31", features = ["bundled"] }
ed25519-dalek = { version = "2", features = ["rand_core"] }
rand = "0.8"
hex = "0.4"
thiserror = "1"
```

- [ ] **Step 2: `keystore.rs`**

```rust
use ed25519_dalek::{SigningKey, VerifyingKey};
use rusqlite::{params, Connection};
use std::path::Path;

const KEYRING_SERVICE: &str = "iextend";
const KEYRING_USER:    &str = "host-root";

pub struct Keystore { db: Connection }

impl Keystore {
    pub fn open(db_path: &Path) -> anyhow::Result<Self> {
        // Restrict file mode 0600 on Unix; on Windows we rely on user-profile ACLs.
        #[cfg(unix)] {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new().create(true).write(true).mode(0o600).open(db_path)?;
        }
        let db = Connection::open(db_path)?;
        db.execute_batch("
            CREATE TABLE IF NOT EXISTS pinned_ipads (
                pubkey BLOB PRIMARY KEY,
                device_name TEXT NOT NULL,
                pair_id BLOB NOT NULL,
                added_unix INTEGER NOT NULL
            );")?;
        Ok(Self { db })
    }

    pub fn root_signing_key(&self) -> anyhow::Result<SigningKey> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?;
        match entry.get_password() {
            Ok(s) => Ok(SigningKey::from_bytes(&hex_to_32(&s)?)),
            Err(keyring::Error::NoEntry) => {
                let sk = SigningKey::generate(&mut rand::rngs::OsRng);
                entry.set_password(&hex::encode(sk.to_bytes()))?;
                Ok(sk)
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn pin(&self, pubkey: &VerifyingKey, name: &str, pair_id: [u8; 16]) -> anyhow::Result<()> {
        self.db.execute(
            "INSERT OR REPLACE INTO pinned_ipads VALUES (?,?,?,?)",
            params![pubkey.as_bytes().to_vec(), name, pair_id.to_vec(), now_unix()],
        )?;
        Ok(())
    }

    pub fn is_pinned(&self, pubkey: &VerifyingKey) -> anyhow::Result<bool> {
        let n: i64 = self.db.query_row(
            "SELECT COUNT(*) FROM pinned_ipads WHERE pubkey = ?",
            params![pubkey.as_bytes().to_vec()], |r| r.get(0))?;
        Ok(n > 0)
    }

    pub fn forget(&self, pubkey: &VerifyingKey) -> anyhow::Result<()> {
        self.db.execute("DELETE FROM pinned_ipads WHERE pubkey = ?", params![pubkey.as_bytes().to_vec()])?;
        Ok(())
    }
}

fn now_unix() -> i64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64 }
fn hex_to_32(s: &str) -> anyhow::Result<[u8; 32]> { let v = hex::decode(s)?; let a: [u8; 32] = v.try_into().map_err(|_| anyhow::anyhow!("bad len"))?; Ok(a) }
```

- [ ] **Step 3: Tests — temp DB, insert + query + forget**

```rust
#[test]
fn pin_query_forget_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let ks = Keystore::open(&dir.path().join("k.sqlite")).unwrap();
    let sk = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
    let vk = sk.verifying_key();
    ks.pin(&vk, "Aman's iPad", [7u8; 16]).unwrap();
    assert!(ks.is_pinned(&vk).unwrap());
    ks.forget(&vk).unwrap();
    assert!(!ks.is_pinned(&vk).unwrap());
}
```

- [ ] **Step 4: Commit**

```bash
git add host/crates/iextendd/src/keystore.rs host/crates/iextendd/Cargo.toml
git commit -m "feat(iextendd): keystore — OS-keyring root key + sqlite pinned iPads"
```

---

### Task 7: Host SPAKE2 server (`ix-rtc/src/pairing.rs`)

**Files:**
- Create: `host/crates/ix-rtc/src/pairing.rs` (and submodule `vectors.rs` from Task 3)
- Modify: `host/crates/ix-rtc/Cargo.toml` (add `spake2`, `aes-gcm`, `hkdf`, `sha2`, `hmac`)

- [ ] **Step 1: Define the state machine type**

```rust
pub enum PairServerState {
    Start,
    AwaitClient(spake2::Spake2<spake2::P256Group>),
    AwaitCertReq { k_pair: [u8; 32] },
    Done { ipad_pubkey: ed25519_dalek::VerifyingKey, pair_id: [u8; 16] },
}

pub struct PairServer {
    state: PairServerState,
    pin: zeroize::Zeroizing<[u8; 4]>,
    host_name: String,
    keystore: Arc<Keystore>,
    root_sk: ed25519_dalek::SigningKey,
}
```

- [ ] **Step 2: Implement `step` that advances on each `PairMsg`**

Methods on `PairServer`:
- `new(pin: [u8; 4], host_name: String, keystore: Arc<Keystore>) -> Self`
- `start_response(client_pstart: &PairMsg) -> Result<PairMsg, Error>`
  - parse client ephemeral
  - run SPAKE2 server side; derive `k_pair = HKDF-SHA256(spake_K, info=b"iextend-pair-v1")[..32]`
  - assemble `PRESPONSE` with server-confirm-MAC
- `cert_response(client_certreq: &PairMsg) -> Result<PairMsg, Error>`
  - AEAD-open the body with `k_pair`
  - sign cert CBOR with `root_sk`
  - pin the iPad pubkey
  - AEAD-seal the response

- [ ] **Step 3: Three-way unit test**

A single test creates a `PairServer` with a fixed PIN and feeds it canned client messages from a vector file. Asserts: no `PERR`, end state `Done`, keystore now contains the iPad pubkey.

- [ ] **Step 4: Brute-force-detection test**

Feed a `PSTART` whose ephemeral pubkey is *valid for a different PIN*. Server's confirm-MAC verification must fail and emit `PERR{reason: 0x10 = MAC_MISMATCH}`. Repeat 20 attempts; assert that after 20 mismatches within 60s the server emits `PERR{reason: 0x11 = RATE_LIMIT}` and refuses further attempts for 5 minutes.

- [ ] **Step 5: Commit**

```bash
git add host/crates/ix-rtc/src/pairing.rs host/crates/ix-rtc/Cargo.toml
git commit -m "feat(ix-rtc): SPAKE2 server — handshake, AEAD cert-wrap, brute-force lockout"
```

---

### Task 8: Host TCP listener — `iextendd::pair_listener`

**Files:**
- Create: `host/crates/iextendd/src/pair_listener.rs`
- Modify: `host/crates/iextendd/src/main.rs` (spawn the listener)

- [ ] **Step 1: Listener task**

```rust
pub async fn run(bind: std::net::SocketAddr, keystore: Arc<Keystore>, pin_chan: mpsc::Receiver<[u8; 4]>) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(bind).await?;
    while let Ok((mut stream, peer)) = listener.accept().await {
        let pin = match pin_chan.try_recv() { Ok(p) => p, Err(_) => continue }; // no PIN active → reject
        let ks  = keystore.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_one(&mut stream, pin, ks).await {
                tracing::warn!(?peer, ?e, "pair attempt failed");
                let _ = ix_pair_wire::PairMsg { kind: ix_pair_wire::Kind::PErr, body: vec![0x20] }.write_to(&mut stream);
            }
        });
    }
    Ok(())
}

async fn handle_one(stream: &mut tokio::net::TcpStream, pin: [u8; 4], ks: Arc<Keystore>) -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    // bridge tokio<->blocking PairMsg::read_from via buffering
    // ... (full implementation: read PSTART, build PairServer, run state machine to Done, close)
    todo!("see Task 7 for state machine details")
}
```

Replace the `todo!` with the explicit two-step flow that reads `PSTART`, writes `PRESPONSE`, reads `PCERT_REQ`, writes `PCERT_OK`, then closes the connection. Use a 30-second wall-clock deadline (`tokio::time::timeout`); on expiry send `PERR{0x21 = TIMEOUT}` and close.

- [ ] **Step 2: Bind to `127.0.0.1:5353` and `::1:5353` only when a PIN is active**

Default-deny posture: if no PIN window is open, the daemon refuses new connections (sends `PERR{0x22 = NO_ACTIVE_PIN}` and closes). PIN windows are 60 seconds; opening one is gated by the tray UI button, never automatic.

- [ ] **Step 3: Commit**

```bash
git add host/crates/iextendd/src/pair_listener.rs host/crates/iextendd/src/main.rs
git commit -m "feat(iextendd): TCP :5353 pair listener with PIN gating + 30s deadline"
```

---

### Task 9: Tray PIN screen (`iextend-tray::pair_screen`)

**Files:**
- Create: `host/crates/iextend-tray/src/pair_screen.rs`
- Modify: `host/crates/iextend-tray/src/main.rs` (add a "Pair iPad" button + tab)

- [ ] **Step 1: PIN-display screen**

```rust
pub struct PairScreen {
    pin: Option<[u8; 4]>,
    expires_at: Option<std::time::Instant>,
    pin_chan: mpsc::Sender<[u8; 4]>,
}

impl PairScreen {
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        if let (Some(pin), Some(deadline)) = (self.pin, self.expires_at) {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            ui.heading(format!("{}{}{}{}",
                pin[0] as char, pin[1] as char, pin[2] as char, pin[3] as char));
            ui.label(format!("Expires in {}s — type this on your iPad.", remaining.as_secs()));
            if remaining.is_zero() { self.pin = None; self.expires_at = None; }
        } else if ui.button("Pair iPad").clicked() {
            let p = generate_pin();
            self.pin = Some(p);
            self.expires_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(60));
            let _ = self.pin_chan.try_send(p);
        }
    }
}

fn generate_pin() -> [u8; 4] {
    use rand::Rng;
    let n: u16 = rand::thread_rng().gen_range(0..10000);
    let s = format!("{n:04}");
    [s.as_bytes()[0], s.as_bytes()[1], s.as_bytes()[2], s.as_bytes()[3]]
}
```

- [ ] **Step 2: Wire `pin_chan` into `iextendd` via the local gRPC socket**

The tray sends the active PIN to the daemon over its existing localhost gRPC channel. The daemon's `pair_listener` reads from the same channel.

- [ ] **Step 3: Snapshot test for the screen**

`egui_kittest` snapshot of the screen with a fixed PIN `4729` and `30s` remaining; assert it matches the committed PNG (regen on intentional UI changes).

- [ ] **Step 4: Commit**

```bash
git add host/crates/iextend-tray/src/pair_screen.rs host/crates/iextend-tray/src/main.rs
git commit -m "feat(iextend-tray): PIN-display screen + 60s countdown + daemon channel"
```

---

### Task 10: iPad SPAKE2 client (`PairingFlow.swift`)

**Files:**
- Modify (fill stub): `ipad/iExtendKit/Sources/iExtendKit/Connection/PairingFlow.swift`
- Create: `ipad/iExtendKit/Sources/iExtendKit/Connection/Keychain.swift`

- [ ] **Step 1: `Keychain.swift`**

```swift
import Foundation
import Security

enum KeychainError: Error { case status(OSStatus) }

public enum HostKeychain {
    private static let svc = "iextend.host-pubkey"

    public static func save(hostPub: Data, hostName: String) throws {
        let q: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
                                kSecAttrService as String: svc,
                                kSecAttrAccount as String: hostName]
        SecItemDelete(q as CFDictionary)
        var add = q
        add[kSecValueData as String] = hostPub
        let s = SecItemAdd(add as CFDictionary, nil)
        guard s == errSecSuccess else { throw KeychainError.status(s) }
    }

    public static func load(hostName: String) -> Data? {
        let q: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
                                kSecAttrService as String: svc,
                                kSecAttrAccount as String: hostName,
                                kSecReturnData as String: true,
                                kSecMatchLimit as String: kSecMatchLimitOne]
        var out: AnyObject?
        return SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess ? out as? Data : nil
    }
}
```

- [ ] **Step 2: `PairingFlow.swift`**

```swift
import Crypto
import Foundation
import Network

public actor PairingFlow {
    public enum PairError: Error { case wrongPIN, timeout, mac, aead, transport }

    public func pair(host: NWEndpoint, hostName: String, pin: String) async throws {
        precondition(pin.count == 4 && pin.allSatisfy(\.isNumber))
        let conn = NWConnection(to: host, using: .tcp)
        try await conn.startAndWait(timeout: .seconds(30))

        // 1. PSTART
        let client = SPAKE2.Client(password: pin.data(using: .ascii)!)
        let pStartBody = client.start()                       // ephemeral pubkey
        try await conn.sendMsg(.init(kind: .pStart, body: pStartBody))

        // 2. PRESPONSE
        let pRes = try await conn.recvMsg()
        guard pRes.kind == .pResponse else { throw PairError.transport }
        let kPair = try client.finalize(peer: pRes.body)      // throws on MAC mismatch

        // 3. PCERT_REQ
        let myKey = Curve25519.Signing.PrivateKey()
        let req = try aeadSeal(key: kPair, plaintext: certRequestPayload(myKey: myKey))
        try await conn.sendMsg(.init(kind: .pCertReq, body: req))

        // 4. PCERT_OK
        let pOk = try await conn.recvMsg()
        guard pOk.kind == .pCertOk else { throw PairError.transport }
        let ptext = try aeadOpen(key: kPair, ciphertext: pOk.body)
        let (cert, hostPub, pairId) = try parseCertOk(ptext)

        // verify host signature on cert
        try verify(cert: cert, hostPub: hostPub)
        try HostKeychain.save(hostPub: hostPub, hostName: hostName)
        UserDefaults.standard.set(pairId, forKey: "ix.pairId.\(hostName)")
        conn.cancel()
    }
}
```

- [ ] **Step 3: Wrong-PIN test**

Spin up a Rust pair server with PIN `1234`; iPad client tries `9999`; assert the error is `.wrongPIN`. Don't catch generic; the `mac` case must surface as `.wrongPIN` to the UI.

- [ ] **Step 4: Commit**

```bash
git add ipad/iExtendKit/Sources/iExtendKit/Connection/{PairingFlow,Keychain}.swift \
        ipad/iExtendKit/Tests/iExtendKitTests/PairingFlowTests.swift
git commit -m "feat(ipad): PairingFlow — SPAKE2 client + AEAD cert-wrap + Keychain pin"
```

---

### Task 11: `PinEntryView.swift` — UI behavior

**Files:**
- Modify (fill stub): `ipad/iExtendUI/Sources/iExtendUI/Onboarding/PinEntryView.swift`

- [ ] **Step 1: Behavior**

```swift
public struct PinEntryView: View {
    @State private var digits: [String] = ["","","",""]
    @State private var errorText: String?
    @State private var submitting = false
    let host: DiscoveredHost
    let pairing: PairingFlow

    public var body: some View {
        VStack(spacing: 20) {
            // 4 PIN cells (already drafted in Plan 6) — focus walks left to right
            // ...
            if let e = errorText { Text(e).foregroundStyle(.red) }
            Button("Connect") { Task { await submit() } }
                .disabled(digits.contains(where: \.isEmpty) || submitting)
        }
    }

    func submit() async {
        submitting = true
        defer { submitting = false }
        let pin = digits.joined()
        do {
            try await pairing.pair(host: host.endpoint, hostName: host.name, pin: pin)
            // navigate to ConnectingView (already in nav stack)
        } catch PairingFlow.PairError.wrongPIN {
            errorText = "That PIN doesn't match the one shown on your PC."
            digits = ["","","",""]
        } catch PairingFlow.PairError.timeout {
            errorText = "Took too long. Click 'Pair iPad' on your PC again."
        } catch {
            errorText = "Couldn't pair. \(String(describing: error))"
        }
    }
}
```

- [ ] **Step 2: ViewInspector tests for the four states**

`empty / typing / submitting / wrongPIN`. Snapshot the rendered view in dark mode (already required by Plan 6 styling).

- [ ] **Step 3: Commit**

```bash
git add ipad/iExtendUI/Sources/iExtendUI/Onboarding/PinEntryView.swift \
        ipad/iExtendUI/Tests/iExtendUITests/PinEntryViewTests.swift
git commit -m "feat(ipad): PinEntryView submit/error/typing states wired to PairingFlow"
```

---

### Task 12: Steady-state cert-pinned reconnect

**Files:**
- Create: `host/crates/ix-rtc/src/reconnect.rs`
- Create: `ipad/iExtendKit/Sources/iExtendKit/Connection/Reconnect.swift`

- [ ] **Step 1: Host — accept-and-validate path**

When `ix-rtc` accepts a new WebRTC offer, the iPad's DTLS certificate is exposed via `RTCPeerConnection::remote_certificate()`. Compare the cert's pubkey against `Keystore::is_pinned`; reject the offer with SDP error code 403 if not pinned. Wire this as a hook in `PeerConnection::on_offer`.

- [ ] **Step 2: iPad — present cert path**

When iPad creates a `RTCPeerConnection`, it constructs an `RTCCertificate` from the **device pair-id-bound key** (the Ed25519 key we generated in Task 10). DTLS uses this cert; the host validates it against the pinned list.

- [ ] **Step 3: End-to-end auto-reconnect test**

After a successful pairing, simulate a Wi-Fi flap: drop the WebRTC connection, then `ix-rtc` should re-handshake and end up `Live` again within 1 s with **no PIN prompt and no UI interaction**.

- [ ] **Step 4: Forget device test**

User taps "Forget device" on host; on iPad's next reconnect, host rejects (cert no longer pinned); iPad's UI surfaces "This PC unpaired you. Re-pair?" rather than spinning silently.

- [ ] **Step 5: Commit**

```bash
git add host/crates/ix-rtc/src/reconnect.rs \
        ipad/iExtendKit/Sources/iExtendKit/Connection/Reconnect.swift
git commit -m "feat: cert-pinned DTLS reconnect on both sides; forget-device flow"
```

---

### Task 13: Replay protection on DataChannels

**Files:**
- Create: `host/crates/ix-rtc/src/replay.rs`
- Create: `ipad/iExtendKit/Sources/iExtendKit/Connection/Replay.swift`

- [ ] **Step 1: Window data structure (both sides)**

Per-channel state: `last_seq: u64`, `bitmap: [u64; 16]` (1024 bits). On receive, packet's seq is checked against the window; ahead-of-window slides the window; in-window checks the bit; behind-window rejects.

```rust
pub struct ReplayWindow { last_seq: u64, bitmap: [u64; 16] }

impl ReplayWindow {
    pub fn check_and_set(&mut self, seq: u64) -> bool {
        const W: u64 = 1024;
        if seq > self.last_seq {
            let shift = seq - self.last_seq;
            if shift >= W { self.bitmap = [0; 16]; }
            else { self.shift_left(shift as usize); }
            self.last_seq = seq;
            self.set_bit(0);
            true
        } else {
            let off = (self.last_seq - seq) as usize;
            if off >= W as usize { return false; }                 // too old
            if self.get_bit(off) { return false; }                  // replay
            self.set_bit(off);
            true
        }
    }
    // ... shift_left, get_bit, set_bit
}
```

- [ ] **Step 2: Tests covering: in-order accept, replay reject, out-of-order accept-once, far-past reject**

Property-based: random sequence of seq numbers; assert that for any input set, each seq is accepted at most once.

- [ ] **Step 3: Wire it in**

Wrap each `DataChannel::on_message` with the corresponding window. The `input` channel and the `control` channel each get their own window. Failures increment a metric; do not log per-packet (would be too noisy under attack).

- [ ] **Step 4: Commit**

```bash
git add host/crates/ix-rtc/src/replay.rs \
        ipad/iExtendKit/Sources/iExtendKit/Connection/Replay.swift
git commit -m "feat: 1024-slot sliding-window seq replay protection (host+iPad)"
```

---

### Task 14: Cross-language interop test harness

**Files:**
- Create: `tests/interop/Cargo.toml`
- Create: `tests/interop/src/server_main.rs`
- Create: `tests/interop/ipad-sim-driver/run.sh`
- Create: `tests/interop/README.md`

- [ ] **Step 1: Rust server harness**

`server_main.rs` spawns a `pair_listener`, sets a fixed PIN `4729`, prints its mDNS service name to stdout, and exits when the first pairing succeeds (or after 60s).

- [ ] **Step 2: iPad simulator driver**

`run.sh` wraps `xcrun simctl boot 'iPad Pro (11-inch)'`, builds `iExtendKit` for the simulator, runs an XCTest fixture that:
  1. Browses for the advertised host
  2. Submits PIN `4729`
  3. Asserts the keychain now contains a host pubkey

- [ ] **Step 3: The negative test**

Same harness but iPad submits PIN `9999`; expects `.wrongPIN`. Both server and client log clean (no panics, no leaked file handles).

- [ ] **Step 4: Document running locally**

`tests/interop/README.md` covers requirements: macOS dev box, Xcode 16+, Rust 1.79+, `avahi-daemon` not needed (mDNSResponder built into macOS). CI runs this on the macOS runner only.

- [ ] **Step 5: Commit**

```bash
git add tests/interop
git commit -m "test(interop): Rust↔Swift SPAKE2 + cert-exchange end-to-end harness"
```

---

### Task 15: Plan-7 README polish + final tag

**Files:**
- Modify: `README.md` (flip Plan 7 checkbox)

- [ ] **Step 1: Tick the box**

```markdown
- [x] Plan 7: SPAKE2 pairing flow (host + iPad)
```

- [ ] **Step 2: Run the full host test suite**

```bash
cd /home/tops/Projects/iExtend/host
cargo test --workspace
```

Expected: all tests green; the SPAKE2 RFC vector test must show explicitly in the output.

- [ ] **Step 3: Run the iPad unit tests**

```bash
cd /home/tops/Projects/iExtend/ipad
xcodebuild test -scheme iExtendKit -destination 'platform=iOS Simulator,name=iPad Pro (11-inch),OS=latest'
```

Expected: green; SPAKE2 vector test prints `K = …` and matches.

- [ ] **Step 4: Run the interop harness once locally (or document why CI-only)**

- [ ] **Step 5: Commit and tag**

```bash
git add README.md
git commit -m "docs: tick Plan 7 status — SPAKE2 pairing complete"
git tag -a plan-7-complete -m "Plan 7 of 10 complete: SPAKE2 PAKE pairing, mDNS discovery, cert-pinned reconnect, replay protection"
```

---

## Done criteria

1. A factory-fresh iPad and a factory-fresh laptop, both on the same Wi-Fi, complete first-pair via 4-digit PIN in under 30 s end-to-end (PIN typed → live data channels open).
2. After pair, the iPad reconnects on every subsequent launch with no UI interaction.
3. Wrong PIN returns a specific, user-readable error on iPad inside 5 s.
4. After 20 wrong-PIN attempts within 60 s, the host enters a 5-minute lockout and emits `PERR{0x11}` immediately on subsequent attempts.
5. "Forget device" on either side triggers a clean re-pair flow on next attempt; no silent reconnects.
6. RFC 9382 §C.1 SPAKE2 vector matches in both Rust and Swift CI.
7. Cross-language interop harness pairs successfully macOS-host ↔ iPad-simulator.
8. `cargo test --workspace` and `xcodebuild test` both green; tag `plan-7-complete` on the head commit.

## Out of scope for this plan

- Biometric (Face ID) gate for auto-connect — opt-in v1.1, separate plan.
- TURN / NAT traversal — pairing is LAN-only by design; the spec rules out cloud relays.
- Recovery from corrupted keystore — manual reinstall is acceptable for v1.
- Multi-iPad pinning UI — the host CLI / tray supports the data model, but a "manage devices" UI is deferred to a polish plan.

## Notes for Plan 8 (input forwarding)

After Plan 7, both sides have authenticated, encrypted DataChannels with replay protection. Plan 8 can build the input wire on top without re-doing security. The `input` and `control` DataChannels are already configured with their own `ReplayWindow` instances; Plan 8 just defines the payload schemas.
