# Input Forwarding & Cursor Reprojection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** End-to-end input forwarding from iPad → host with full Apple Pencil pressure/tilt fidelity, plus the iPad-side cursor & pencil-tip *reprojection* that hides the wire-latency tail and gets the perceived input feel down to ~2 ms on M2/M4 iPad Pro. After this plan, drawing in Photoshop on a paired iPad feels glassy.

**Architecture:** Three cooperating layers. (A) iPad captures every UITouch / UIPencilInteraction at hardware rate, serializes to a 32-byte fixed-size packet, sends over the unreliable `input` DataChannel. (B) Host parses, fans out to OS-specific stylus / touch / keyboard injectors — a kernel `vhf.sys` on Windows, `/dev/uinput` on Linux. (C) Host emits a per-frame `cursor` control message + tags the encoder to render the host cursor as a known luma signature; iPad masks that signature post-decode and re-renders the cursor + pencil tip on a SwiftUI overlay at the *reprojected* position based on the most recent input sample and measured RTT.

**Tech Stack:**
- Rust 1.78+ (host: `ix-input` crate, encoder hint plumbing in `iextendd`)
- Swift 5.10 / SwiftUI / UIKit / Metal (iPad: `iExtendInput`, `iExtendKit/Render`)
- Windows Driver Kit (WDK) 10.0.26100+ for `vhf.sys`
- Linux: `evdev` + `uinput` (kernel built-in everywhere we care about)
- C++/MSVC for Wintab shim DLL
- Existing Plan 2 / Plan 5 / Plan 6 scaffolding (workspace, control DataChannel, MetalRenderer stub)

**Plan scope:** This is **Plan 8 of 10** for the iExtend project. Spec sections covered: §6 Input forwarding (all of 6.1–6.4) and the cursor-related parts of §8.2 (Metal render pipeline). Out of scope: Pairing (Plan 7), installers / codesigning (Plan 9), bench rig (Plan 10).

**Honest expectations:**

- Without §6.4 cursor reprojection, the product feels "good" — about 22 ms perceived input latency, on par with Steam Link.
- With §6.4, perceived input latency drops to ~2 ms because we cheat the eye on the cursor / tip while the actual video frame is still 14 ms behind. This is the difference between "decent screen-sharing" and "Sidecar-class drawing tablet."
- **Don't cut §6.4.** It's the whole point of choosing iPad over a generic tablet.

**Plan depends on:**
- **Plan 2** (Rust workspace exists with `host/crates/ix-input` stub crate, control + input DataChannel plumbing types in `ix-rtc`)
- **Plan 5** (WebRTC peer connection + control + input DataChannels are open and routable end-to-end with a 32-byte test packet)
- **Plan 6** (Swift package `iExtendInput` exists with stub `EventCapture` class wired into the iPad app's `UIWindow`; `MetalRenderer.swift` performs basic decoded-frame blit to a `CAMetalLayer`)

**Plan parallels:** Task 9 in this plan (vhf-stylus driver) reuses the EV cert + WHQL pipeline established in Plan 3 for the IddCx display driver. Same signing key, same submission flow; only the WDK project skeleton differs.

---

## File Structure

This plan creates / modifies the following:

```
iExtend/
├── host/
│   ├── crates/
│   │   ├── ix-input/
│   │   │   ├── Cargo.toml                     # MODIFY: add cfg-gated deps
│   │   │   ├── tests/wire_vectors.rs          # CREATE: shared cross-platform vectors
│   │   │   ├── tests/wire_vectors.json        # CREATE: shared with Swift
│   │   │   └── src/
│   │   │       ├── lib.rs                     # CREATE: public API + dispatch
│   │   │       ├── wire.rs                    # CREATE: 32-byte packet codec
│   │   │       ├── windows.rs                 # CREATE: vhf user-mode partner
│   │   │       ├── linux.rs                   # CREATE: /dev/uinput injector
│   │   │       └── q16.rs                     # CREATE: fixed-point helpers
│   │   └── iextendd/src/
│   │       └── cursor_protocol.rs             # CREATE: per-frame cursor msg + encoder hint
│   ├── drivers/windows/vhf-stylus/
│   │   ├── vhf-stylus.vcxproj                 # CREATE: WDK project
│   │   ├── inf/vhf-stylus.inx                 # CREATE: install descriptor
│   │   ├── src/driver.c                       # CREATE: ~150 lines, single HID descriptor
│   │   └── src/hid_report.h                   # CREATE: ~50 lines, byte-level layout
│   ├── shims/windows/wintab-shim/
│   │   ├── Wintab32.dll                       # built artifact
│   │   ├── wintab-shim.vcxproj                # CREATE
│   │   └── src/wintab_shim.cpp                # CREATE: ~400 lines, Pointer Input → Wintab
│   └── crates/iextendd/Cargo.toml             # MODIFY: depend on ix-input
├── ipad/
│   ├── iExtendInput/Sources/iExtendInput/
│   │   ├── EventCapture.swift                 # COMPLETE (Plan 6 left a stub)
│   │   ├── PacketEncoder.swift                # CREATE: matches Rust wire.rs
│   │   ├── PacketEncoderTests.swift           # CREATE: tests/iExtendInputTests/
│   │   └── PencilSampleSource.swift           # CREATE: PencilKit + UIPencilInteraction
│   ├── iExtendKit/Sources/iExtendKit/Render/
│   │   ├── MetalRenderer.swift                # MODIFY: add cursor-mask + reproject pass
│   │   ├── CursorMaskShader.metal             # CREATE: ~80 lines compute shader
│   │   └── ReprojectMath.swift                # CREATE: position prediction from RTT
│   ├── iExtendUI/Sources/iExtendUI/Live/
│   │   └── PencilHud.swift                    # CREATE: SwiftUI overlay for tip indicator
│   └── tests/
│       ├── PacketEncoderTests.swift           # cross-test against shared vectors
│       └── ReprojectMathTests.swift           # CREATE: ~120 lines unit tests
└── docs/superpowers/plans/
    └── 2026-05-08-plan-8-input-forwarding.md  # this file
```

**Authoring discipline:** every file gets one clear responsibility. `wire.rs` and `PacketEncoder.swift` are mirror images intentionally — one is the source of truth in vectors; both must round-trip the shared JSON test corpus byte-for-byte.

---

## Wire format reference

All tasks below assume this 32-byte packet (spec §6.1):

```
Offset  Size  Field            Notes
─────── ───── ──────────────── ────────────────────────────────────────
0       1     kind             enum (see below)
1       8     time_us          u64 LE — iPad mach_absolute_time in µs
9       4     seq              u32 LE — monotonic per-channel
13      1     flags            bit 0 = predicted, bit 1 = coalesced,
                               bits 2–7 reserved (must be zero)
14      18    payload          kind-specific (see below)

kind values:
  0x01 TOUCH_BEGIN     0x02 TOUCH_MOVE     0x03 TOUCH_END
  0x10 PENCIL_BEGIN    0x11 PENCIL_MOVE    0x12 PENCIL_END
  0x20 KEY_DOWN        0x21 KEY_UP         0x22 MODIFIER

PENCIL_* payload (18 bytes):
  Offset  Size  Field         Notes
  ──────  ────  ───────────── ──────────────────────────
  0       4     x_q16         iPad pixels, fixed-point
  4       4     y_q16
  8       2     pressure_q16  0..=1.0
  10      2     tilt_q16      altitude angle, 0..=π/2
  12      2     azimuth_q16   azimuth, 0..=2π
  14      2     twist_q16     Pencil Pro only, else 0
  16      1     buttons       bit 0 = barrel
  17      1     hover         1 = hover, 0 = contact

TOUCH_* payload (18 bytes):
  Offset  Size  Field
  ──────  ────  ───────────────
  0       4     x_q16
  4       4     y_q16
  8       2     radius_major_q16
  10      2     radius_minor_q16
  12      2     force_q16
  14      4     reserved (zero)

KEY_* payload (18 bytes):
  Offset  Size  Field
  ──────  ────  ───────────────
  0       2     usage_page    HID
  2       2     usage         HID
  4       2     modifiers     bitmask
  6       12    reserved (zero)
```

Q16 fixed-point: signed 32-bit integer, denominator 65 536. So 1.0 = 0x00010000, π = 0x0003243F (close).

Packet endianness: **little-endian** to match WebRTC's binary message convention. Swift uses `withUnsafeBytes` + `loadUnaligned`. Rust uses `byteorder` or manual shifts.

---

### Task 1: `ix-input` wire codec + Q16 helpers (Rust)

**Files:**
- Create: `host/crates/ix-input/src/q16.rs`
- Create: `host/crates/ix-input/src/wire.rs`
- Modify: `host/crates/ix-input/src/lib.rs`
- Create: `host/crates/ix-input/tests/wire_vectors.json`
- Create: `host/crates/ix-input/tests/wire_vectors.rs`

- [ ] **Step 1: Write the failing test for Q16 round-trip**

Append to `host/crates/ix-input/src/q16.rs`:

```rust
// q16.rs — Q16 fixed-point helpers (i32 / 65_536).
//
// We use Q16 (not f32) on the wire so that small pencil deltas survive
// FP rounding when the host scales them to virtual-monitor pixels.
//
// Encode: q16_from_f32(1.5) = 0x0001_8000
// Decode: q16_to_f32(0x0001_8000) = 1.5

const SHIFT: i32 = 16;

#[inline]
pub fn q16_from_f32(v: f32) -> i32 {
    (v * 65_536.0).round() as i32
}

#[inline]
pub fn q16_to_f32(v: i32) -> f32 {
    (v as f32) / 65_536.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_unit() {
        assert_eq!(q16_to_f32(q16_from_f32(1.0)), 1.0);
    }

    #[test]
    fn roundtrip_pi() {
        let pi = std::f32::consts::PI;
        let q = q16_from_f32(pi);
        let back = q16_to_f32(q);
        assert!((pi - back).abs() < 1.0 / 65_536.0);
    }

    #[test]
    fn negatives_survive() {
        assert_eq!(q16_to_f32(q16_from_f32(-2.5)), -2.5);
    }
}
```

- [ ] **Step 2: Run; verify failure**

```bash
cd host/crates/ix-input
cargo test --lib q16
```

Expected: 3 tests fail with "module q16 not found" — that's because `lib.rs` hasn't declared it.

- [ ] **Step 3: Wire it into lib.rs**

Add to `host/crates/ix-input/src/lib.rs`:

```rust
pub mod q16;
pub mod wire;
```

Re-run `cargo test --lib q16`. Expected: 3 passed.

- [ ] **Step 4: Write the wire codec test**

Create `host/crates/ix-input/tests/wire_vectors.rs`:

```rust
use ix_input::wire::{Packet, Kind, PencilPayload};

#[test]
fn pencil_move_round_trip() {
    let p = Packet {
        kind: Kind::PencilMove,
        time_us: 1_700_000_000_000_000,
        seq: 42,
        flags: 0b01, // predicted
        payload: PencilPayload {
            x: 768.5, y: 1024.25,
            pressure: 0.5, tilt: 1.2, azimuth: 3.0, twist: 0.0,
            barrel: false, hover: false,
        }.into_bytes(),
    };
    let bytes = p.to_bytes();
    assert_eq!(bytes.len(), 32);
    let back = Packet::from_bytes(&bytes).unwrap();
    assert_eq!(back.kind, Kind::PencilMove);
    assert_eq!(back.seq, 42);
    assert_eq!(back.flags, 0b01);
    let pl = PencilPayload::from_bytes(&back.payload);
    assert!((pl.x - 768.5).abs() < 1.0 / 65_536.0);
    assert!((pl.pressure - 0.5).abs() < 1.0 / 65_536.0);
}

#[test]
fn rejects_short_buffer() {
    assert!(Packet::from_bytes(&[0u8; 31]).is_err());
}

#[test]
fn rejects_unknown_kind() {
    let mut bytes = [0u8; 32];
    bytes[0] = 0xFF;
    assert!(Packet::from_bytes(&bytes).is_err());
}
```

- [ ] **Step 5: Run, verify failure**

```bash
cargo test --test wire_vectors
```

Expected: compile errors about `Packet`, `Kind`, `PencilPayload` not existing.

- [ ] **Step 6: Implement the wire codec**

Create `host/crates/ix-input/src/wire.rs` with `Packet`, `Kind`, `PencilPayload`, `TouchPayload`, `KeyPayload`, plus `to_bytes` / `from_bytes`. Pack/unpack u32/u64 LE manually — no external crate needed.

```rust
use crate::q16::{q16_from_f32, q16_to_f32};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    TouchBegin  = 0x01,
    TouchMove   = 0x02,
    TouchEnd    = 0x03,
    PencilBegin = 0x10,
    PencilMove  = 0x11,
    PencilEnd   = 0x12,
    KeyDown     = 0x20,
    KeyUp       = 0x21,
    Modifier    = 0x22,
}

impl Kind {
    pub fn from_u8(v: u8) -> Option<Kind> {
        match v {
            0x01 => Some(Kind::TouchBegin),
            0x02 => Some(Kind::TouchMove),
            0x03 => Some(Kind::TouchEnd),
            0x10 => Some(Kind::PencilBegin),
            0x11 => Some(Kind::PencilMove),
            0x12 => Some(Kind::PencilEnd),
            0x20 => Some(Kind::KeyDown),
            0x21 => Some(Kind::KeyUp),
            0x22 => Some(Kind::Modifier),
            _    => None,
        }
    }
}

pub const PACKET_LEN: usize = 32;

#[derive(Debug, Clone)]
pub struct Packet {
    pub kind: Kind,
    pub time_us: u64,
    pub seq: u32,
    pub flags: u8,
    pub payload: [u8; 18],
}

#[derive(Debug)]
pub enum DecodeError { ShortBuffer, UnknownKind, ReservedBitsSet }

impl Packet {
    pub fn to_bytes(&self) -> [u8; PACKET_LEN] {
        let mut out = [0u8; PACKET_LEN];
        out[0] = self.kind as u8;
        out[1..9].copy_from_slice(&self.time_us.to_le_bytes());
        out[9..13].copy_from_slice(&self.seq.to_le_bytes());
        out[13] = self.flags;
        out[14..32].copy_from_slice(&self.payload);
        out
    }

    pub fn from_bytes(b: &[u8]) -> Result<Self, DecodeError> {
        if b.len() < PACKET_LEN { return Err(DecodeError::ShortBuffer); }
        let kind = Kind::from_u8(b[0]).ok_or(DecodeError::UnknownKind)?;
        if b[13] & 0b1111_1100 != 0 { return Err(DecodeError::ReservedBitsSet); }
        let time_us = u64::from_le_bytes(b[1..9].try_into().unwrap());
        let seq     = u32::from_le_bytes(b[9..13].try_into().unwrap());
        let flags   = b[13];
        let mut payload = [0u8; 18];
        payload.copy_from_slice(&b[14..32]);
        Ok(Self { kind, time_us, seq, flags, payload })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PencilPayload {
    pub x: f32, pub y: f32,
    pub pressure: f32, pub tilt: f32,
    pub azimuth: f32, pub twist: f32,
    pub barrel: bool, pub hover: bool,
}

impl PencilPayload {
    pub fn into_bytes(self) -> [u8; 18] {
        let mut b = [0u8; 18];
        b[0..4 ].copy_from_slice(&q16_from_f32(self.x       ).to_le_bytes());
        b[4..8 ].copy_from_slice(&q16_from_f32(self.y       ).to_le_bytes());
        b[8..10].copy_from_slice(&(q16_from_f32(self.pressure) as i16).to_le_bytes());
        b[10..12].copy_from_slice(&(q16_from_f32(self.tilt   ) as i16).to_le_bytes());
        b[12..14].copy_from_slice(&(q16_from_f32(self.azimuth) as i16).to_le_bytes());
        b[14..16].copy_from_slice(&(q16_from_f32(self.twist  ) as i16).to_le_bytes());
        b[16] = if self.barrel { 1 } else { 0 };
        b[17] = if self.hover  { 1 } else { 0 };
        b
    }
    pub fn from_bytes(b: &[u8; 18]) -> Self {
        Self {
            x:        q16_to_f32(i32::from_le_bytes(b[0..4 ].try_into().unwrap())),
            y:        q16_to_f32(i32::from_le_bytes(b[4..8 ].try_into().unwrap())),
            pressure: q16_to_f32(i16::from_le_bytes(b[8..10].try_into().unwrap()) as i32),
            tilt:     q16_to_f32(i16::from_le_bytes(b[10..12].try_into().unwrap()) as i32),
            azimuth:  q16_to_f32(i16::from_le_bytes(b[12..14].try_into().unwrap()) as i32),
            twist:    q16_to_f32(i16::from_le_bytes(b[14..16].try_into().unwrap()) as i32),
            barrel:   b[16] != 0,
            hover:    b[17] != 0,
        }
    }
}

// TouchPayload + KeyPayload are analogous; mirror the wire-format reference at
// the top of the plan. ~80 more lines.
```

Implement TouchPayload and KeyPayload with the same shape. Run `cargo test --test wire_vectors` — expected: 3 passed.

- [ ] **Step 7: Generate the shared cross-platform vector file**

Create `host/crates/ix-input/tests/wire_vectors.json`:

```json
[
  {
    "name": "pencil_move_pi_pressure",
    "kind": 17,
    "time_us": 1700000000000000,
    "seq": 42,
    "flags": 1,
    "pencil": {
      "x": 768.5, "y": 1024.25,
      "pressure": 0.5, "tilt": 1.5707963,
      "azimuth": 3.14159, "twist": 0.0,
      "barrel": false, "hover": false
    },
    "expected_bytes": "11008064b1ad9760000000002a0000000080800000805007007fc7000a000000000000"
  },
  {
    "name": "key_down_a",
    "kind": 32,
    "time_us": 0,
    "seq": 1,
    "flags": 0,
    "key": { "usage_page": 7, "usage": 4, "modifiers": 2 },
    "expected_bytes": "20000000000000000000010000000007000400020000000000000000000000000000"
  }
]
```

(Bytes hex above are illustrative — recompute via `cargo test -- --nocapture` once the test below runs.)

- [ ] **Step 8: Write the cross-platform vector test**

In `host/crates/ix-input/tests/wire_vectors.rs`, add:

```rust
#[test]
fn shared_vector_corpus_round_trips() {
    let raw = std::fs::read_to_string("tests/wire_vectors.json").unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    for entry in v.as_array().unwrap() {
        let bytes_hex = entry["expected_bytes"].as_str().unwrap();
        let bytes: Vec<u8> = (0..bytes_hex.len()).step_by(2)
            .map(|i| u8::from_str_radix(&bytes_hex[i..i+2], 16).unwrap())
            .collect();
        let p = ix_input::wire::Packet::from_bytes(&bytes).unwrap();
        let re = p.to_bytes();
        assert_eq!(&re[..], &bytes[..], "round trip changed bytes for {}", entry["name"]);
    }
}
```

Add `serde_json = "1"` to `[dev-dependencies]` in Cargo.toml. Run `cargo test`. If `expected_bytes` was a guess and the test fails, regenerate the corpus from the actual encoder output (see commit message below). The Swift test in Task 4 reads the *same* JSON file.

- [ ] **Step 9: Commit**

```bash
git add host/crates/ix-input/
git commit -m "feat(ix-input): 32-byte wire format + Q16 codec + shared vectors"
```

---

### Task 2: Swift `PacketEncoder` mirroring `wire.rs`

**Files:**
- Create: `ipad/iExtendInput/Sources/iExtendInput/PacketEncoder.swift`
- Create: `ipad/iExtendInput/Tests/iExtendInputTests/PacketEncoderTests.swift`
- Symlink (or copy build-time) `host/crates/ix-input/tests/wire_vectors.json` into `ipad/iExtendInput/Tests/iExtendInputTests/Fixtures/`

- [ ] **Step 1: Bring the shared corpus into the iPad test bundle**

Add a build phase to the iExtendInput Swift package that copies the wire vector corpus:

```swift
// Package.swift addition for iExtendInput
.testTarget(
    name: "iExtendInputTests",
    dependencies: ["iExtendInput"],
    resources: [.copy("Fixtures/wire_vectors.json")]
)
```

Place a real copy at `ipad/iExtendInput/Tests/iExtendInputTests/Fixtures/wire_vectors.json` (NOT a symlink; Xcode's resource pipeline rejects symlinks). A pre-build script keeps it in sync:

```bash
cp host/crates/ix-input/tests/wire_vectors.json \
   ipad/iExtendInput/Tests/iExtendInputTests/Fixtures/wire_vectors.json
```

Wire that into a `Makefile` target `make sync-fixtures` and add it to the iPad CI pre-build step.

- [ ] **Step 2: Write the failing Swift test**

Create `ipad/iExtendInput/Tests/iExtendInputTests/PacketEncoderTests.swift`:

```swift
import XCTest
@testable import iExtendInput

final class PacketEncoderTests: XCTestCase {
    func testSharedVectorCorpusRoundTrips() throws {
        let url = Bundle.module.url(forResource: "wire_vectors", withExtension: "json")!
        let data = try Data(contentsOf: url)
        let entries = try JSONSerialization.jsonObject(with: data) as! [[String: Any]]
        for entry in entries {
            let hex = entry["expected_bytes"] as! String
            let bytes = hex.hexBytes()
            let pkt = try Packet(bytes: bytes)
            let re  = pkt.encoded()
            XCTAssertEqual(Array(re), Array(bytes),
                           "round trip changed bytes for \(entry["name"] ?? "?")")
        }
    }

    func testPencilEncodeMatchesRust() {
        var p = Packet()
        p.kind   = .pencilMove
        p.timeUs = 1_700_000_000_000_000
        p.seq    = 42
        p.flags  = 0b01
        p.setPencil(x: 768.5, y: 1024.25,
                    pressure: 0.5, tilt: .pi/2,
                    azimuth: 3.14159, twist: 0,
                    barrel: false, hover: false)
        let out = p.encoded()
        XCTAssertEqual(out.count, 32)
        let back = try! Packet(bytes: out)
        XCTAssertEqual(back.seq, 42)
        XCTAssertEqual(back.kind, .pencilMove)
    }
}
```

Run `swift test`. Expected: compile failure — `Packet` doesn't exist yet.

- [ ] **Step 3: Implement `PacketEncoder.swift`**

Create `ipad/iExtendInput/Sources/iExtendInput/PacketEncoder.swift`:

```swift
import Foundation

public enum PacketKind: UInt8 {
    case touchBegin  = 0x01, touchMove = 0x02, touchEnd  = 0x03
    case pencilBegin = 0x10, pencilMove = 0x11, pencilEnd = 0x12
    case keyDown     = 0x20, keyUp     = 0x21, modifier  = 0x22
}

public struct Packet {
    public static let length = 32

    public var kind:    PacketKind = .touchMove
    public var timeUs:  UInt64 = 0
    public var seq:     UInt32 = 0
    public var flags:   UInt8  = 0
    public var payload: [UInt8] = Array(repeating: 0, count: 18)

    public init() {}

    public init(bytes: [UInt8]) throws {
        guard bytes.count >= Packet.length else { throw DecodeError.shortBuffer }
        guard let k = PacketKind(rawValue: bytes[0]) else { throw DecodeError.unknownKind }
        guard bytes[13] & 0b1111_1100 == 0 else { throw DecodeError.reservedBitsSet }
        kind   = k
        timeUs = bytes.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: 1, as: UInt64.self) }.littleEndian
        seq    = bytes.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: 9, as: UInt32.self) }.littleEndian
        flags  = bytes[13]
        payload = Array(bytes[14..<32])
    }

    public enum DecodeError: Error { case shortBuffer, unknownKind, reservedBitsSet }

    public func encoded() -> [UInt8] {
        var out = [UInt8](repeating: 0, count: 32)
        out[0] = kind.rawValue
        withUnsafeBytes(of: timeUs.littleEndian) { src in
            for (i, b) in src.enumerated() { out[1 + i] = b }
        }
        withUnsafeBytes(of: seq.littleEndian) { src in
            for (i, b) in src.enumerated() { out[9 + i] = b }
        }
        out[13] = flags
        for (i, b) in payload.enumerated() { out[14 + i] = b }
        return out
    }

    public mutating func setPencil(
        x: Float, y: Float,
        pressure: Float, tilt: Float,
        azimuth: Float, twist: Float,
        barrel: Bool, hover: Bool
    ) {
        var p = [UInt8](repeating: 0, count: 18)
        Q16.writeI32(&p, offset: 0, value: x)
        Q16.writeI32(&p, offset: 4, value: y)
        Q16.writeI16(&p, offset: 8,  value: pressure)
        Q16.writeI16(&p, offset: 10, value: tilt)
        Q16.writeI16(&p, offset: 12, value: azimuth)
        Q16.writeI16(&p, offset: 14, value: twist)
        p[16] = barrel ? 1 : 0
        p[17] = hover  ? 1 : 0
        payload = p
    }
}

enum Q16 {
    static func writeI32(_ buf: inout [UInt8], offset: Int, value: Float) {
        let q = Int32((value * 65_536.0).rounded()).littleEndian
        withUnsafeBytes(of: q) { src in
            for (i, b) in src.enumerated() { buf[offset + i] = b }
        }
    }
    static func writeI16(_ buf: inout [UInt8], offset: Int, value: Float) {
        let q = Int16(clamping: Int((value * 65_536.0).rounded())).littleEndian
        withUnsafeBytes(of: q) { src in
            for (i, b) in src.enumerated() { buf[offset + i] = b }
        }
    }
}

extension String {
    func hexBytes() -> [UInt8] {
        var out: [UInt8] = []
        var i = startIndex
        while i < endIndex {
            let j = index(i, offsetBy: 2)
            out.append(UInt8(self[i..<j], radix: 16)!)
            i = j
        }
        return out
    }
}
```

- [ ] **Step 4: Run, verify pass**

```bash
cd ipad/iExtendInput
make sync-fixtures
swift test
```

Expected: both tests pass.

- [ ] **Step 5: Commit**

```bash
git add ipad/iExtendInput/ Makefile
git commit -m "feat(iExtendInput): PacketEncoder mirrors ix-input wire format"
```

---

### Task 3: iPad `EventCapture` — UITouch handling

**Files:**
- Modify: `ipad/iExtendInput/Sources/iExtendInput/EventCapture.swift` (Plan 6 left a stub)

The Plan 6 stub looks like:

```swift
public final class EventCapture {
    public var onPacket: ((Packet) -> Void)?
    public func attach(to window: UIWindow) { /* TODO Plan 8 */ }
}
```

This task fills it in for `UITouch` only. Pencil + keyboard are Tasks 4 + 5.

- [ ] **Step 1: Write the test**

Create `ipad/iExtendInput/Tests/iExtendInputTests/EventCaptureTests.swift`:

```swift
import XCTest
import UIKit
@testable import iExtendInput

@MainActor
final class EventCaptureTests: XCTestCase {
    func testTouchBeginEmitsPacket() {
        let cap = EventCapture()
        var seen: [Packet] = []
        cap.onPacket = { seen.append($0) }

        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 1366, height: 1024))
        cap.attach(to: window)

        // Simulate a UIWindow.sendEvent with a synthetic touch
        let touch = SyntheticTouch(phase: .began, location: CGPoint(x: 100, y: 200), pressure: 0)
        cap.didReceive(touches: [touch])

        XCTAssertEqual(seen.count, 1)
        XCTAssertEqual(seen[0].kind, .touchBegin)
    }
}
```

`SyntheticTouch` is a mock conforming to a small `TouchLike` protocol that `EventCapture` expects (so we don't need to subclass UITouch which is impossible). Defining the protocol is part of step 2.

- [ ] **Step 2: Implement EventCapture (touch only)**

Replace `ipad/iExtendInput/Sources/iExtendInput/EventCapture.swift`:

```swift
import UIKit

public protocol TouchLike: AnyObject {
    var phase: UITouch.Phase { get }
    var location: CGPoint { get }
    var force: CGFloat { get }
    var majorRadius: CGFloat { get }
}

public final class EventCapture {
    public var onPacket: ((Packet) -> Void)?
    public weak var window: UIWindow?
    private var seq: UInt32 = 0

    public init() {}

    public func attach(to window: UIWindow) {
        self.window = window
        // Real wiring uses UIWindow.sendEvent override or a UIInteraction.
        // The test path calls didReceive(touches:) directly.
    }

    public func didReceive(touches: [TouchLike]) {
        for t in touches {
            var p = Packet()
            p.kind = mapPhase(t.phase, isPencil: false)
            p.timeUs = nowMicros()
            p.seq = nextSeq()
            p.flags = 0
            p.setTouch(x: Float(t.location.x), y: Float(t.location.y),
                       radiusMajor: Float(t.majorRadius), radiusMinor: 0,
                       force: Float(t.force))
            onPacket?(p)
        }
    }

    private func mapPhase(_ phase: UITouch.Phase, isPencil: Bool) -> PacketKind {
        switch phase {
        case .began:               return isPencil ? .pencilBegin : .touchBegin
        case .moved, .stationary:  return isPencil ? .pencilMove  : .touchMove
        case .ended, .cancelled:   return isPencil ? .pencilEnd   : .touchEnd
        @unknown default:          return isPencil ? .pencilMove  : .touchMove
        }
    }

    private func nextSeq() -> UInt32 { seq &+= 1; return seq }
    private func nowMicros() -> UInt64 {
        var info = mach_timebase_info_data_t()
        mach_timebase_info(&info)
        let abs = mach_absolute_time()
        return abs * UInt64(info.numer) / (UInt64(info.denom) * 1_000)
    }
}

extension Packet {
    public mutating func setTouch(
        x: Float, y: Float,
        radiusMajor: Float, radiusMinor: Float, force: Float
    ) {
        var p = [UInt8](repeating: 0, count: 18)
        Q16.writeI32(&p, offset: 0, value: x)
        Q16.writeI32(&p, offset: 4, value: y)
        Q16.writeI16(&p, offset: 8,  value: radiusMajor)
        Q16.writeI16(&p, offset: 10, value: radiusMinor)
        Q16.writeI16(&p, offset: 12, value: force)
        payload = p
    }
}
```

- [ ] **Step 3: Wire production code into UIWindow**

In the live iPad app (Plan 6's `LiveScreenView`), add a `UIWindow` subclass:

```swift
final class IExtendWindow: UIWindow {
    let capture = EventCapture()
    override func sendEvent(_ event: UIEvent) {
        if let touches = event.allTouches {
            capture.didReceive(touches: touches.map(RealTouch.init))
        }
        super.sendEvent(event)
    }
}

private final class RealTouch: TouchLike {
    let phase: UITouch.Phase
    let location: CGPoint
    let force: CGFloat
    let majorRadius: CGFloat
    init(_ t: UITouch) {
        phase = t.phase
        location = t.preciseLocation(in: nil)
        force = t.force
        majorRadius = t.majorRadius
    }
}
```

- [ ] **Step 4: Run tests, commit**

```bash
swift test
git add ipad/iExtendInput/
git commit -m "feat(iExtendInput): EventCapture handles UITouch"
```

---

### Task 4: iPad `PencilSampleSource` — UIPencilInteraction + predicted touches

**Files:**
- Create: `ipad/iExtendInput/Sources/iExtendInput/PencilSampleSource.swift`
- Modify: `ipad/iExtendInput/Sources/iExtendInput/EventCapture.swift`

- [ ] **Step 1: Write tests covering predicted-touch flagging**

Add to `EventCaptureTests.swift`:

```swift
func testPencilPredictedTouchEmitsPredictedFlag() {
    let cap = EventCapture()
    var seen: [Packet] = []
    cap.onPacket = { seen.append($0) }
    cap.attach(to: UIWindow())

    let real = SyntheticPencil(phase: .moved, location: .init(x: 100, y: 200),
                                force: 0.5, tilt: 1.2, azimuth: 3.0,
                                predicted: false)
    let pred = SyntheticPencil(phase: .moved, location: .init(x: 110, y: 210),
                                force: 0.55, tilt: 1.2, azimuth: 3.0,
                                predicted: true)
    cap.didReceive(pencils: [real, pred])

    XCTAssertEqual(seen.count, 2)
    XCTAssertEqual(seen[0].flags & 1, 0, "real sample is not predicted")
    XCTAssertEqual(seen[1].flags & 1, 1, "predicted sample carries the flag")
}

func testPencilProSamplesTwistOnM2() {
    let cap = EventCapture()
    var seen: [Packet] = []
    cap.onPacket = { seen.append($0) }
    cap.attach(to: UIWindow())
    let s = SyntheticPencil(phase: .moved, location: .zero, force: 0.5,
                             tilt: 0, azimuth: 0, predicted: false, twist: 1.0)
    cap.didReceive(pencils: [s])
    // decode payload, verify twist round-trip survived Q16
    XCTAssertEqual(seen.count, 1)
}
```

- [ ] **Step 2: Implement**

Create `ipad/iExtendInput/Sources/iExtendInput/PencilSampleSource.swift`:

```swift
import UIKit

public protocol PencilLike: TouchLike {
    var altitudeAngle: CGFloat { get }   // tilt
    var azimuthAngle:  CGFloat { get }
    var twistRoll:     CGFloat { get }   // Pencil Pro only; 0 otherwise
    var hover:         Bool    { get }
    var predicted:     Bool    { get }
    var barrelButton:  Bool    { get }
}
```

Extend `EventCapture`:

```swift
public extension EventCapture {
    func didReceive(pencils: [PencilLike]) {
        for p in pencils {
            var pkt = Packet()
            pkt.kind   = mapPhase(p.phase, isPencil: true)
            pkt.timeUs = nowMicros()
            pkt.seq    = nextSeq()
            pkt.flags  = (p.predicted ? 0b01 : 0)
            pkt.setPencil(
                x: Float(p.location.x), y: Float(p.location.y),
                pressure: Float(p.force),
                tilt: Float(.pi / 2 - p.altitudeAngle),
                azimuth: Float(p.azimuthAngle),
                twist: Float(p.twistRoll),
                barrel: p.barrelButton,
                hover: p.hover
            )
            onPacket?(pkt)
        }
    }
}
```

Plumb the production path: in `IExtendWindow.sendEvent`, separate touches by `t.type == .pencil`. Pull `event.predictedTouches(for:)` for predicted samples; flag them in the same call. Pencil Pro's `twistRoll` requires `UIPencilHoverPose` (iPadOS 17.5+); guard with availability:

```swift
let twist: CGFloat
if #available(iOS 17.5, *), let pose = t.preciseTwistRoll() {
    twist = pose
} else {
    twist = 0
}
```

- [ ] **Step 3: Verify, commit**

```bash
swift test
git add ipad/iExtendInput/
git commit -m "feat(iExtendInput): pencil samples + predicted touches + Pencil Pro twist"
```

---

### Task 5: iPad keyboard capture

**Files:**
- Modify: `ipad/iExtendInput/Sources/iExtendInput/EventCapture.swift`
- Modify: `ipad/iExtendInput/Tests/iExtendInputTests/EventCaptureTests.swift`

Goal: capture `UIPress` events (the iPadOS hardware-keyboard pipeline) and serialize HID usage codes. iPad on-screen-keyboard taps come through as UIPress on iPadOS 18+ once we set `becomeFirstResponder` on a hidden text input view; for older OS we route via a transparent `UITextView` that captures `keyCommands`.

- [ ] **Step 1: Test**

```swift
func testKeyDownEmitsUsageAndModifiers() {
    let cap = EventCapture()
    var seen: [Packet] = []
    cap.onPacket = { seen.append($0) }
    let press = SyntheticPress(phase: .began, key: SyntheticKey(usagePage: 7, usage: 4, mods: 2))
    cap.didReceive(presses: [press])
    XCTAssertEqual(seen.first?.kind, .keyDown)
}
```

- [ ] **Step 2: Implement**

```swift
public protocol PressLike { var phase: UIPress.Phase { get }; var key: KeyData { get } }
public struct KeyData { public let usagePage: UInt16; public let usage: UInt16; public let mods: UInt16 }

public extension EventCapture {
    func didReceive(presses: [PressLike]) {
        for p in presses {
            var pkt = Packet()
            pkt.kind = (p.phase == .ended ? .keyUp : .keyDown)
            pkt.timeUs = nowMicros()
            pkt.seq    = nextSeq()
            pkt.flags  = 0
            pkt.setKey(usagePage: p.key.usagePage, usage: p.key.usage, mods: p.key.mods)
            onPacket?(pkt)
        }
    }
}

extension Packet {
    public mutating func setKey(usagePage: UInt16, usage: UInt16, mods: UInt16) {
        var p = [UInt8](repeating: 0, count: 18)
        withUnsafeBytes(of: usagePage.littleEndian) { for (i, b) in $0.enumerated() { p[0 + i] = b } }
        withUnsafeBytes(of: usage.littleEndian)     { for (i, b) in $0.enumerated() { p[2 + i] = b } }
        withUnsafeBytes(of: mods.littleEndian)      { for (i, b) in $0.enumerated() { p[4 + i] = b } }
        payload = p
    }
}
```

- [ ] **Step 3: Run, commit**

```bash
swift test
git add ipad/iExtendInput/
git commit -m "feat(iExtendInput): keyboard capture (HID usage + modifiers)"
```

---

### Task 6: Host wire decoder + dispatch

**Files:**
- Modify: `host/crates/ix-input/src/lib.rs`

Wire the `input` DataChannel sink into the per-OS injectors. The transport layer (Plan 5) hands us `&[u8]`; we hand the parsed Packet to a trait object.

- [ ] **Step 1: Test**

`host/crates/ix-input/src/lib.rs`:

```rust
#[cfg(test)]
mod dispatch_tests {
    use super::*;
    use std::sync::Mutex;

    struct Recorder(Mutex<Vec<wire::Packet>>);
    impl Injector for Recorder {
        fn inject(&self, p: &wire::Packet) { self.0.lock().unwrap().push(p.clone()); }
    }

    #[test]
    fn dispatch_routes_to_injector() {
        let r = std::sync::Arc::new(Recorder(Mutex::new(vec![])));
        let mut d = Dispatcher::new(r.clone());
        let pkt = wire::Packet { /* ... pencil move ... */ };
        d.handle(&pkt.to_bytes()).unwrap();
        assert_eq!(r.0.lock().unwrap().len(), 1);
    }
}
```

- [ ] **Step 2: Implement**

```rust
pub trait Injector: Send + Sync {
    fn inject(&self, p: &wire::Packet);
}

pub struct Dispatcher<I: Injector> {
    injector: std::sync::Arc<I>,
    last_seq_per_kind: std::collections::HashMap<u8, u32>,
}

impl<I: Injector> Dispatcher<I> {
    pub fn new(i: std::sync::Arc<I>) -> Self {
        Self { injector: i, last_seq_per_kind: Default::default() }
    }

    pub fn handle(&mut self, bytes: &[u8]) -> Result<(), wire::DecodeError> {
        let p = wire::Packet::from_bytes(bytes)?;
        // Drop replay/out-of-order. Unreliable channel reorders; we accept
        // *forward* progress only.
        let key = p.kind as u8;
        let last = self.last_seq_per_kind.get(&key).copied().unwrap_or(0);
        if p.seq > last || p.seq == 0 {
            self.last_seq_per_kind.insert(key, p.seq);
            self.injector.inject(&p);
        }
        Ok(())
    }
}
```

- [ ] **Step 3: Commit**

```bash
git add host/crates/ix-input/
git commit -m "feat(ix-input): Dispatcher + Injector trait + seq-based dedup"
```

---

### Task 7: Linux uinput injector — touch + stylus + keyboard

**Files:**
- Create: `host/crates/ix-input/src/linux.rs`

- [ ] **Step 1: Test against a real `/dev/uinput`**

CI runners must expose `/dev/uinput` (root or `udev` rule). Test:

```rust
#[cfg(target_os = "linux")]
#[test]
#[ignore] // requires /dev/uinput; run with `cargo test -- --ignored`
fn linux_uinput_creates_stylus_and_emits_pressure() {
    let inj = LinuxInjector::new().expect("uinput available");
    let mut p = wire::Packet { /* pencil_move, pressure 0.7 */ };
    inj.inject(&p);
    // Verify via libevdev that ABS_PRESSURE moved to the expected raw value.
}
```

- [ ] **Step 2: Implement**

```rust
use std::fs::OpenOptions;
use std::os::fd::AsRawFd;
use std::os::raw::c_int;

pub struct LinuxInjector {
    stylus_fd: c_int,
    touch_fd:  c_int,
    kbd_fd:    c_int,
}

impl LinuxInjector {
    pub fn new() -> std::io::Result<Self> {
        Ok(Self {
            stylus_fd: open_uinput_stylus()?,
            touch_fd:  open_uinput_touch()?,
            kbd_fd:    open_uinput_keyboard()?,
        })
    }
}

fn open_uinput_stylus() -> std::io::Result<c_int> {
    let f = OpenOptions::new().write(true).open("/dev/uinput")?;
    let fd = f.as_raw_fd();
    std::mem::forget(f); // we hold the fd for the lifetime of the process
    // ioctl UI_SET_EVBIT, UI_SET_ABSBIT, UI_DEV_SETUP, UI_DEV_CREATE...
    // axes: ABS_X, ABS_Y, ABS_PRESSURE, ABS_TILT_X, ABS_TILT_Y, ABS_DISTANCE
    // tools: BTN_TOOL_PEN, BTN_TOUCH, BTN_STYLUS (barrel)
    Ok(fd)
}

// open_uinput_touch — multi-touch protocol B (ABS_MT_*)
// open_uinput_keyboard — KEY_*

impl Injector for LinuxInjector {
    fn inject(&self, p: &wire::Packet) {
        match p.kind {
            wire::Kind::PencilBegin | wire::Kind::PencilMove | wire::Kind::PencilEnd => {
                let pl = wire::PencilPayload::from_bytes(&p.payload.try_into().unwrap());
                emit(self.stylus_fd, EV_ABS, ABS_X, pl.x as i32);
                emit(self.stylus_fd, EV_ABS, ABS_Y, pl.y as i32);
                emit(self.stylus_fd, EV_ABS, ABS_PRESSURE, (pl.pressure * 8192.0) as i32);
                // tilt mapping: altitude → ABS_TILT_X/Y as -90..=90
                emit(self.stylus_fd, EV_ABS, ABS_TILT_X, ((pl.tilt.cos() * pl.azimuth.cos()) * 90.0) as i32);
                emit(self.stylus_fd, EV_ABS, ABS_TILT_Y, ((pl.tilt.cos() * pl.azimuth.sin()) * 90.0) as i32);
                emit(self.stylus_fd, EV_KEY, BTN_TOOL_PEN, if pl.hover { 1 } else { 1 });
                emit(self.stylus_fd, EV_KEY, BTN_TOUCH, if pl.hover { 0 } else { 1 });
                emit(self.stylus_fd, EV_KEY, BTN_STYLUS, if pl.barrel { 1 } else { 0 });
                emit(self.stylus_fd, EV_SYN, SYN_REPORT, 0);
            }
            wire::Kind::TouchBegin | wire::Kind::TouchMove | wire::Kind::TouchEnd => { /* MT-B */ }
            wire::Kind::KeyDown => { emit(self.kbd_fd, EV_KEY, key_from(p), 1); emit(self.kbd_fd, EV_SYN, SYN_REPORT, 0); }
            wire::Kind::KeyUp   => { emit(self.kbd_fd, EV_KEY, key_from(p), 0); emit(self.kbd_fd, EV_SYN, SYN_REPORT, 0); }
            wire::Kind::Modifier => { /* update kept-modifiers shadow */ }
        }
    }
}
```

- [ ] **Step 3: Live-verify on a Linux box**

```bash
cargo test --target x86_64-unknown-linux-gnu --features linux -- --ignored
xinput list  # should show three new "iExtend ..." virtual devices
```

Open Krita, draw with a fake pencil event source — strokes should have pressure variation matching what you piped in.

- [ ] **Step 4: Commit**

```bash
git add host/crates/ix-input/src/linux.rs
git commit -m "feat(ix-input): Linux uinput injector — stylus, touch, keyboard"
```

---

### Task 8: Windows `vhf-stylus` kernel driver

**Files:**
- Create: `host/drivers/windows/vhf-stylus/{vhf-stylus.vcxproj,inf/vhf-stylus.inx,src/driver.c,src/hid_report.h}`

**This is a kernel driver.** It reuses the EV cert + WHQL pipeline established for the IddCx driver in Plan 3. Same signing key, same submission flow; only the WDK project skeleton differs.

- [ ] **Step 1: WDK project skeleton**

Copy the IddCx project's `.vcxproj` from Plan 3, change the target type to `KMDF VHF Driver` and the OutputType to `Driver`. Build configuration: `Release | x64`.

- [ ] **Step 2: HID report descriptor**

`host/drivers/windows/vhf-stylus/src/hid_report.h`:

```c
// 18-byte HID input report:
// - X (i32, logical 0..32767)
// - Y (i32, logical 0..32767)
// - Pressure (u16, logical 0..1024)
// - Tilt-X (i16, logical -9000..9000 = -90.00..+90.00°)
// - Tilt-Y (i16, logical -9000..9000)
// - Buttons (u8, bit 0 = tip, bit 1 = barrel, bit 2 = hover)
// - Padding (u8)

#define HID_REPORT_DESC_SIZE 96
extern const UCHAR HidReportDescriptor[HID_REPORT_DESC_SIZE];
```

`src/hid_report.c`:

```c
#include <ntddk.h>
#include "hid_report.h"

const UCHAR HidReportDescriptor[HID_REPORT_DESC_SIZE] = {
    0x05, 0x0D,        // Usage Page (Digitizer)
    0x09, 0x02,        // Usage (Pen)
    0xA1, 0x01,        // Collection (Application)
    0x09, 0x20,        //   Usage (Stylus)
    0xA1, 0x00,        //   Collection (Physical)
    /* X, Y, pressure, tilt, buttons — see Microsoft sample digitizer
       and Hagiwara HID 1.11 §16.7. */
    /* ... ~80 more bytes ... */
    0xC0,              //   End Collection
    0xC0,              // End Collection
};
```

- [ ] **Step 3: Driver entry**

`host/drivers/windows/vhf-stylus/src/driver.c`:

```c
#include <ntddk.h>
#include <wdf.h>
#include <vhf.h>
#include "hid_report.h"

NTSTATUS DriverEntry(PDRIVER_OBJECT DriverObject, PUNICODE_STRING RegistryPath) {
    WDF_DRIVER_CONFIG config;
    WDF_DRIVER_CONFIG_INIT(&config, EvtDeviceAdd);
    return WdfDriverCreate(DriverObject, RegistryPath, WDF_NO_OBJECT_ATTRIBUTES, &config, WDF_NO_HANDLE);
}

NTSTATUS EvtDeviceAdd(WDFDRIVER Driver, PWDFDEVICE_INIT DeviceInit) {
    /* allocate device, init VHF config with HidReportDescriptor,
       register VhfSubmitReport callback that drains a per-device queue. */
    return STATUS_SUCCESS;
}
```

- [ ] **Step 4: User-mode partner**

`host/crates/ix-input/src/windows.rs`:

```rust
use std::os::windows::raw::HANDLE;

pub struct WindowsInjector {
    device: HANDLE, // \\.\iExtendStylus
}

impl WindowsInjector {
    pub fn new() -> std::io::Result<Self> { /* CreateFileW on \\.\iExtendStylus */ }
}

impl Injector for WindowsInjector {
    fn inject(&self, p: &wire::Packet) {
        // Format an 18-byte HID report; DeviceIoControl(IOCTL_VHF_SUBMIT_REPORT)
    }
}
```

- [ ] **Step 5: Sign & install**

Same EV cert + WHQL flow as Plan 3. Local dev requires `bcdedit /set testsigning on`.

- [ ] **Step 6: Live test**

Install driver, run host, drive synthetic pencil events from a test harness. Open Krita on Windows and draw — pressure curve should look smooth (not stairstep).

- [ ] **Step 7: Commit**

```bash
git add host/drivers/windows/vhf-stylus/ host/crates/ix-input/src/windows.rs
git commit -m "feat(host/win): vhf-stylus kernel driver + ix-input windows partner"
```

---

### Task 9: Wintab compatibility shim DLL

**Files:**
- Create: `host/shims/windows/wintab-shim/{wintab-shim.vcxproj,src/wintab_shim.cpp}`

A small drop-in `Wintab32.dll` that legacy apps (older Photoshop, Wacom-era apps) load. We translate Windows Pointer Input events (which our kernel driver emits via Windows Ink) into Wintab API responses. No Wacom dependency.

- [ ] **Step 1: Project skeleton**

DLL output `Wintab32.dll`. Plan 9's installer drops it into `%PROGRAMFILES%/iExtend/Wintab32.dll` and prepends that to the relevant search paths.

- [ ] **Step 2: Stub the Wintab entry points**

```cpp
// src/wintab_shim.cpp
#include <windows.h>
extern "C" {

__declspec(dllexport) HCTX WTOpenA(HWND hwnd, LPLOGCONTEXTA lpLogCtx, BOOL fEnable) {
    /* register a window message subscription on hwnd for WM_POINTER* */
    /* return an opaque HCTX */
}

__declspec(dllexport) BOOL WTPacket(HCTX hctx, UINT serial, LPVOID lpPkt) {
    /* pop the next pointer event, fill PACKET (legacy Wintab struct) */
}

__declspec(dllexport) BOOL WTClose(HCTX hctx) { /* unregister */ }
/* ... ~15 more entry points ... */

}
```

- [ ] **Step 3: Test with a Wintab demo app**

Use the Wacom WTtest sample (no Wacom hardware needed for the test). Pipe synthetic pencil events through our kernel driver; WTtest should display strokes with pressure.

- [ ] **Step 4: Commit**

```bash
git add host/shims/windows/wintab-shim/
git commit -m "feat(host/win): Wintab32.dll shim — Pointer Input → Wintab"
```

---

### Task 10: Cursor protocol — host emits per-frame `cursor` message

**Files:**
- Create: `host/crates/iextendd/src/cursor_protocol.rs`
- Modify: `host/crates/iextendd/src/main.rs`
- Modify: `host/crates/ix-codec/src/lib.rs` (Plan 5)

The host needs to (a) emit cursor position updates on the **control** DataChannel each capture tick, and (b) hint the encoder to render the cursor as a known-signature sprite the iPad can mask.

- [ ] **Step 1: Define the control message**

`host/crates/iextendd/src/cursor_protocol.rs`:

```rust
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ControlMsg {
    #[serde(rename = "cursor")]
    Cursor {
        x: f32, y: f32,
        sprite_id: u32,
        hotspot_x: f32, hotspot_y: f32,
        ts_us: u64,
    },
    /* other variants from Plan 5 */
}

pub struct CursorEmitter {
    last_pos: (f32, f32),
    last_sprite: u32,
    seq: u32,
}

impl CursorEmitter {
    pub fn new() -> Self { Self { last_pos: (0.0, 0.0), last_sprite: 0, seq: 0 } }

    /// Sample current OS cursor (GetCursorPos on Win, /sys on Wayland);
    /// emit ControlMsg::Cursor on each call. Cheap (<5 µs) — call per capture tick.
    pub fn tick(&mut self) -> Option<ControlMsg> {
        let (x, y) = sample_cursor();
        let sprite = current_sprite_id();
        if (x, y) != self.last_pos || sprite != self.last_sprite {
            self.last_pos = (x, y);
            self.last_sprite = sprite;
            self.seq += 1;
            Some(ControlMsg::Cursor {
                x, y, sprite_id: sprite,
                hotspot_x: 0.0, hotspot_y: 0.0,
                ts_us: now_micros(),
            })
        } else {
            None
        }
    }
}

#[cfg(windows)] fn sample_cursor() -> (f32, f32) { /* GetCursorPos */ }
#[cfg(target_os = "linux")] fn sample_cursor() -> (f32, f32) { /* Wayland: ext-image-capture; X11: XQueryPointer */ }
fn current_sprite_id() -> u32 { /* from OS cursor handle */ 0 }
fn now_micros() -> u64 { /* clock_gettime CLOCK_MONOTONIC_RAW */ 0 }
```

- [ ] **Step 2: Encoder hint API**

In `ix-codec`, add a method on `Encoder`:

```rust
pub trait Encoder {
    /* existing... */

    /// Hint the encoder to render a known-signature cursor sprite at (x, y)
    /// during the next encode call. The sprite is two-tone: fully-saturated
    /// magenta core (#FF00FF) with a 1-pixel cyan border (#00FFFF). The iPad
    /// recognizes this 2-color pattern in the decoded frame and masks it out.
    ///
    /// On vendors that support it (NVENC, VAAPI), we use the encoder's ROI /
    /// QP-map to force this region intra-coded so the signature survives
    /// inter-prediction unscrambled.
    fn set_cursor_overlay(&mut self, sprite_id: u32, x: f32, y: f32);
}
```

- [ ] **Step 3: Wire it into iextendd's main capture loop**

```rust
// in iextendd/src/main.rs capture loop
loop {
    let frame = capture.next_frame()?;
    if let Some(msg) = cursor_emitter.tick() {
        control_channel.send(serde_json::to_string(&msg)?)?;
    }
    encoder.set_cursor_overlay(current_sprite_id(), cursor_x, cursor_y);
    encoder.encode(&frame)?;
}
```

- [ ] **Step 4: Commit**

```bash
git add host/crates/
git commit -m "feat(iextendd): emit cursor msgs + encoder cursor-overlay hint"
```

---

### Task 11: `CursorMaskShader.metal` — iPad-side post-decode mask

**Files:**
- Create: `ipad/iExtendKit/Sources/iExtendKit/Render/CursorMaskShader.metal`

- [ ] **Step 1: Write the Metal compute kernel**

```metal
#include <metal_stdlib>
using namespace metal;

// Per-frame uniforms
struct Uniforms {
    float2 cursor_pos;     // host coords, normalized 0..=1
    float2 cursor_size;    // sprite size, normalized
    uint32_t sprite_id;
    float threshold;       // luma signature tolerance
};

// Recognize the magenta-core / cyan-border signature and replace with
// the median of the surrounding (non-signature) pixels.
kernel void mask_cursor(
    texture2d<half, access::read>       in_tex  [[ texture(0) ]],
    texture2d<half, access::write>      out_tex [[ texture(1) ]],
    constant Uniforms&                   u       [[ buffer(0)  ]],
    uint2                                gid     [[ thread_position_in_grid ]]
) {
    half4 c = in_tex.read(gid);

    // Magenta signature: R high, G low, B high, near 1.0/0.0/1.0.
    bool is_magenta = (c.r > 0.95h) && (c.g < 0.10h) && (c.b > 0.95h);
    // Cyan border: R low, G high, B high.
    bool is_cyan    = (c.r < 0.10h) && (c.g > 0.90h) && (c.b > 0.90h);

    if (is_magenta || is_cyan) {
        // Sample 4 corners of a 9x9 box for a quick median estimate.
        half3 a = in_tex.read(gid + uint2(  4,   4)).rgb;
        half3 b = in_tex.read(gid + uint2( -4,   4)).rgb;
        half3 d = in_tex.read(gid + uint2(  4,  -4)).rgb;
        half3 e = in_tex.read(gid + uint2( -4,  -4)).rgb;
        half3 m = (a + b + d + e) * 0.25h;
        out_tex.write(half4(m, c.a), gid);
    } else {
        out_tex.write(c, gid);
    }
}
```

- [ ] **Step 2: Encoder pre-flight test**

Encode a synthetic frame with the magenta+cyan signature in a known location, decode it, run the shader, verify the signature is gone in the output texture (magenta count drops to <5% of original).

- [ ] **Step 3: Commit**

```bash
git add ipad/iExtendKit/
git commit -m "feat(iExtendKit/Render): CursorMaskShader.metal — post-decode mask"
```

---

### Task 12: Reproject math + iPad cursor overlay

**Files:**
- Create: `ipad/iExtendKit/Sources/iExtendKit/Render/ReprojectMath.swift`
- Create: `ipad/iExtendKit/Tests/iExtendKitTests/ReprojectMathTests.swift`
- Modify: `ipad/iExtendKit/Sources/iExtendKit/Render/MetalRenderer.swift`
- Modify: `ipad/iExtendUI/Sources/iExtendUI/Live/LiveScreenView.swift`

- [ ] **Step 1: Write the test**

```swift
import XCTest
@testable import iExtendKit

final class ReprojectMathTests: XCTestCase {
    func testStaticCursorReprojectsToInputPos() {
        // RTT 14 ms, last input was 4 ms ago at (110, 200), host says (100, 200).
        // Predicted = input + (host_velocity * (rtt - input_age)) + (input - host).
        // For zero host velocity & zero history, predicted = input.
        let p = Reproject.predict(
            hostPos: SIMD2(100, 200),
            history: [(timeUs: 0, pos: SIMD2(110, 200))],
            nowUs: 4_000,
            rttUs: 14_000
        )
        XCTAssertEqual(p.x, 110, accuracy: 0.5)
    }

    func testMovingCursorExtrapolatesForward() {
        // Input @ t=0 was (100, 200), @ t=8 was (108, 200) → +1 px/ms.
        // Now t=12, host says cursor at (100, 200). Predict at t=12 + half_rtt(7).
        // We expect ~(112+7, 200).
        let p = Reproject.predict(
            hostPos: SIMD2(100, 200),
            history: [(timeUs: 0, pos: SIMD2(100, 200)),
                      (timeUs: 8_000, pos: SIMD2(108, 200))],
            nowUs: 12_000,
            rttUs: 14_000
        )
        XCTAssertEqual(p.x, 119, accuracy: 1.0)
    }
}
```

- [ ] **Step 2: Implement**

```swift
import simd

public enum Reproject {
    public struct Sample { public let timeUs: UInt64; public let pos: SIMD2<Float> }

    /// Predict where the cursor should be at "now + half_rtt", given:
    /// - hostPos: where the host last said the cursor was (lags by ~rtt/2)
    /// - history: recent local input samples (newest last)
    /// - nowUs / rttUs: clocks
    public static func predict(
        hostPos: SIMD2<Float>,
        history: [Sample],
        nowUs: UInt64,
        rttUs: UInt64
    ) -> SIMD2<Float> {
        guard let last = history.last else { return hostPos }
        // Velocity from the last two samples.
        let v: SIMD2<Float>
        if history.count >= 2 {
            let prev = history[history.count - 2]
            let dt = Float(last.timeUs - prev.timeUs) / 1_000_000.0
            v = (last.pos - prev.pos) / max(dt, 1e-4)
        } else {
            v = .zero
        }
        let halfRttSec = Float(rttUs) / 2_000_000.0
        let predicted = last.pos + v * halfRttSec
        // Blend: trust local input near the present; trust host as t → ∞.
        // Decay constant 30 ms.
        let ageSec = Float(nowUs - last.timeUs) / 1_000_000.0
        let alpha = exp(-ageSec / 0.030)
        return predicted * alpha + hostPos * (1 - alpha)
    }
}
```

- [ ] **Step 3: Wire the overlay**

In `MetalRenderer.swift`, add:

```swift
public final class MetalRenderer {
    /* existing fields */
    private var cursorPipeline: MTLComputePipelineState?
    private var inputHistory: [Reproject.Sample] = []

    public func ingestInputSample(_ s: Reproject.Sample) {
        inputHistory.append(s)
        if inputHistory.count > 32 { inputHistory.removeFirst() }
    }

    public func render(frame: CVPixelBuffer, hostCursor: SIMD2<Float>, rttUs: UInt64) {
        /* existing decode + blit */

        // 1) Mask: run CursorMaskShader on the decoded MTLTexture.
        // 2) Reproject: compute the iPad-side cursor position.
        let pos = Reproject.predict(
            hostPos: hostCursor,
            history: inputHistory,
            nowUs: nowMicros(),
            rttUs: rttUs
        )
        // 3) Composite the iPad cursor sprite at `pos` on the SwiftUI overlay
        //    via NotificationCenter or an @Observable model — the SwiftUI
        //    view in LiveScreenView reads it.
        cursorOverlay.setPosition(CGPoint(x: CGFloat(pos.x), y: CGFloat(pos.y)))
    }
}
```

In `LiveScreenView.swift`:

```swift
struct LiveScreenView: View {
    @State var cursorPos: CGPoint = .zero
    var body: some View {
        ZStack {
            MetalLayerView(/* ... */)
            CursorSpriteView()
                .position(cursorPos)
                .allowsHitTesting(false)
        }
    }
}
```

- [ ] **Step 4: Run tests, commit**

```bash
swift test
git add ipad/iExtendKit/ ipad/iExtendUI/
git commit -m "feat(iExtendKit/Render): cursor reprojection + SwiftUI overlay"
```

---

### Task 13: Pencil tip HUD overlay

**Files:**
- Create: `ipad/iExtendUI/Sources/iExtendUI/Live/PencilHud.swift`
- Modify: `ipad/iExtendUI/Sources/iExtendUI/Live/LiveScreenView.swift`

The pencil tip indicator is the *small dot* most apps draw under the pencil. We tell the host not to render one (via a control flag), and draw it ourselves at the predicted position.

- [ ] **Step 1: Implement the SwiftUI overlay**

```swift
import SwiftUI

struct PencilHud: View {
    let position: CGPoint
    let pressure: CGFloat   // 0..=1
    let visible:  Bool
    var body: some View {
        Circle()
            .fill(.white.opacity(0.85))
            .frame(width: 8 + 16 * pressure, height: 8 + 16 * pressure)
            .blur(radius: 1)
            .position(position)
            .opacity(visible ? 1 : 0)
            .allowsHitTesting(false)
    }
}
```

- [ ] **Step 2: Drive position via the same Reproject.predict path**

The pencil HUD reads `EventCapture.lastPencilSample` and reprojects with the same math as the cursor. Because it's local-only (no round-trip), perceived input → photon latency for the HUD is **bounded by the iPad's display latch** (~2–4 ms on M2/M4 ProMotion).

- [ ] **Step 3: Tell the host not to render its tip**

Plan 5's control protocol gets a new variant:

```rust
ControlMsg::SetHostCursorVisible { tip: bool }
```

iPad sends `{ tip: false }` whenever the live screen is fronted; sends `{ tip: true }` on background.

- [ ] **Step 4: Commit**

```bash
git add ipad/iExtendUI/ host/
git commit -m "feat(iExtendUI/Live): PencilHud — locally-rendered tip indicator"
```

---

### Task 14: Synthetic input-to-photon latency CI test

**Files:**
- Create: `host/crates/ix-input/tests/loopback_latency.rs`
- Modify: `.github/workflows/ci.yml`

A pure-software latency test: WebRTC loopback (host runs both ends), iPad simulator pipe.

- [ ] **Step 1: Test design**

Send a `PENCIL_MOVE` packet with a high-resolution timestamp from a fake iPad client. Host injects via the `LinuxInjector` (CI runner is Linux). A second loopback display reads the resulting position via `evdev` and timestamps it again. Compute delta. Pass if `< 5 ms` (since this is loopback, no wire latency).

- [ ] **Step 2: Implement and wire into CI**

```yaml
# .github/workflows/ci.yml addition
- name: synthetic input loopback latency
  run: cargo test --test loopback_latency --release
  if: runner.os == 'Linux'
```

- [ ] **Step 3: Commit**

```bash
git add host/crates/ix-input/tests/loopback_latency.rs .github/
git commit -m "test(ci): synthetic input-to-photon loopback latency"
```

---

### Task 15: Optical bench-rig latency procedure (manual / weekly)

**Files:**
- Create: `docs/runbooks/2026-05-08-input-latency-bench.md`

This is a **manual test rig** scoped to Plan 10 to fully automate, but the procedure is documented here so anyone can run it ad-hoc:

```markdown
# Input-to-photon latency — optical procedure

## Setup
- iPhone 15 Pro (or any 240 fps slo-mo camera) on a tripod, framing
  the iPad screen + the host PC monitor side-by-side
- iPad and host PC both connected to the same Wi-Fi 6E AP, 5 GHz
- Run `iextend-bench` (a CLI in `host/crates/iextend-bench`) in measure mode

## Procedure
1. Start the bench app on host. It draws a clock face that increments
   a frame counter at 120 Hz, baked into the captured frame.
2. Start a paired iExtend session.
3. Tap the iPad screen with an Apple Pencil at a known, repeatable spot.
4. Camera captures both screens at 240 fps for 5 seconds.
5. OCR the host clock counter at the moment of touch and at the moment
   the cursor appears on the iPad. Delta in frames * (1000/240) = latency ms.

## Bar
- p50 ≤ 14 ms Wi-Fi 6E
- p99 ≤ 30 ms Wi-Fi 6E
- p50 ≤ 10 ms USB-C
- p99 ≤ 18 ms USB-C
```

- [ ] **Step 1: Commit**

```bash
git add docs/runbooks/
git commit -m "docs(runbook): optical input-to-photon latency procedure"
```

---

### Task 16: Final integration + tag

**Files:** none (final commit + tag)

- [ ] **Step 1: Run all tests across host + iPad**

```bash
cd host && cargo test --release
cd ../ipad && swift test
```

Both must pass.

- [ ] **Step 2: Manual smoke**

Pair a real iPad with a real Win11 host, open Photoshop, draw with Apple Pencil. Subjective bar:

- Cursor is glued to the pencil tip — no visible lag during fast strokes
- Pressure response feels linear; no stairstepping
- Barrel-button click registers as right-button context menu in Photoshop
- Switching to Krita on Linux box produces identical feel
- Disconnecting Wi-Fi mid-stroke gracefully ends the stroke (no runaway line)

- [ ] **Step 3: Tag**

```bash
git tag -a plan-8-complete -m "Plan 8 of 10 complete: input forwarding + cursor reprojection"
```

---

## Done criteria

1. All 6 cross-corpus wire vectors round-trip identically in Rust and Swift.
2. CI green: unit tests + Linux uinput integration + WebRTC loopback latency under 5 ms.
3. Real-hardware smoke pass: M2/M4 iPad → Win11/Photoshop and Ubuntu/Krita, with pressure/tilt visible in strokes and barrel button mapped.
4. Optical bench measures p50 ≤ 14 ms Wi-Fi 6E, p50 ≤ 10 ms USB-C.
5. Cursor reprojection on M-series iPads makes pencil HUD update within 2 ms of the touch (display-latch bounded).
6. Code review: each new file has one clear responsibility; no file > 400 lines except generated ones.
7. Tag `plan-8-complete` on head.

## Out of scope

- Pairing/security (Plan 7 — required dependency)
- Installer that drops `Wintab32.dll` and `vhf-stylus.sys` onto the host (Plan 9)
- macOS host (deferred indefinitely; Apple has Sidecar)
- Audio routing (Plan 11+, not in v1)

## Risks & mitigations

| Risk | Mitigation |
|---|---|
| Cursor signature gets eaten by aggressive HEVC inter-prediction | Encoder hint forces the cursor ROI to intra-coded P-slices; verified in Task 11 step 2. |
| Apple Pencil Pro twist API changes between iPadOS versions | Availability gate `#available(iOS 17.5, *)`; fall back to twist=0. |
| Linux uinput requires root or udev rule | Installer (Plan 9) drops a `60-iextend-uinput.rules` udev rule granting `iextend` group access to `/dev/uinput`. |
| Wintab shim conflicts with installed Wacom driver | Detect `Wacom_Tablet.exe` running; iExtend's shim chain-loads the original Wintab32.dll if present, only injecting our pen events when no Wacom hardware is attached. |
| Reprojection overshoots on rapid direction reversals | History buffer caps at 32 samples; weighted velocity estimate uses last 4 only; clamp prediction to a 30-pixel ball around `last.pos`. |

---

End of Plan 8.
