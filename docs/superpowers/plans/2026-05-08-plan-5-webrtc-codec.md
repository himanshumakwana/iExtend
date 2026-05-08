# Plan 5 of 10 — WebRTC transport + multi-encoder dispatch

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Get encoded video and bidirectional control/input data flowing between the host daemon and a peer over WebRTC. Land the multi-encoder dispatch (NVENC HEVC/AV1, Intel QSV, AMD AMF, VAAPI, x264 software fallback) and the WebRTC peer (1 video track + 2 DataChannels) such that an in-process `smoke_loopback` example measures sustained sub-30 ms p99 round-trip latency on the same machine.

**Architecture:** Two new crates in the `host/` workspace.

- `ix-codec` exposes one `Encoder` trait. Each impl wraps a vendor SDK behind that trait: NVENC, oneVPL, AMF, VAAPI, x264. A runtime probe orders the available encoders by preference and the daemon binds to the first one that initializes successfully. All encoders use the same shared config (intra-refresh 16-row slice gradient, no I-frames, ultralowlatency tune, CBR HQ rate control).
- `ix-rtc` wraps `webrtc-rs` 0.10+. Exposes a `Peer` type that owns the `RTCPeerConnection`, the H.265/AV1 video track, and two DataChannels (`input` unreliable, `control` reliable). Drives a transport-CC-fed bitrate controller that calls `Encoder::set_bitrate()` 4 times per second.
- `iextendd::session` is the wiring point: takes a `DisplaySource` (from Plan 3 / 4), an `Encoder` (from this plan), and a `Peer` (from this plan), and runs the capture → encode → packetize → send loop on a dedicated tokio runtime.

**Tech Stack:**
- Rust 1.75+ stable, 2021 edition
- `webrtc` crate 0.10+ (pure-Rust WebRTC stack; HEVC RTP support landed in 0.10)
- `tokio` 1.35+ (multi-threaded runtime, `rt-multi-thread` + `macros` features)
- `crossbeam-queue` for the lock-free SPSC capture-encoder ring buffer (already in workspace from Plan 2)
- `bindgen` 0.69+ for NVENC/oneVPL/AMF C-header bindings, generated at build time
- NVIDIA Video Codec SDK 12.2+ (downloaded by `build.rs`, license-flagged behind a `nvenc` feature)
- Intel oneVPL 2.10+ (`libvpl`/`libvpl-dev` system package)
- AMD AMF 1.4.30+ (Windows: redistributable DLL; Linux: VAAPI fallback)
- VAAPI via `libva` (Linux only)
- `x264` static library (`libx264-dev` system package; software fallback only)
- `criterion` for the latency benchmark
- `proptest` for state-machine + packet-parser fuzzing

**Plan scope:** This is **Plan 5 of 10**. Depends on:
- **Plan 2** (host workspace bootstrap) — provides the `host/` Cargo workspace, shared `ix-types` crate (`GpuFrame`, `Rect`, `DisplayMode`), and the `iextendd` daemon binary stub.
- **Plan 3 OR Plan 4** (capture pipeline) — provides a working `DisplaySource` impl that produces `GpuFrame`s. The smoke test in this plan does NOT require either capture impl: it uses a `FakeFrameSource` that emits a deterministic test pattern at 120 Hz from a memory buffer.

Plans 6–10 (iPad app, pairing, input forwarding, installers, bench rig) are out of scope here.

---

## Honest complexity note

The single hardest part of this plan is making six different vendor SDKs agree on what "intra-refresh slice gradient" means.

- **NVENC** has `NV_ENC_CONFIG_HEVC.intraRefreshPeriod` and `intraRefreshCnt` — straightforward.
- **oneVPL (Intel QSV)** uses `mfxExtCodingOption2.IntRefType=MFX_REFRESH_HORIZONTAL` plus `IntRefCycleSize`. Nearly the same idea, completely different field names and bit-layout.
- **AMF** (`AMF_VIDEO_ENCODER_HEVC_INTRA_REFRESH_NUM_MBS_PER_SLOT`) measures in macroblocks per slot — must convert from "16-row gradient" (rows of 16-pixel macroblocks) to MBs/slot at config time.
- **VAAPI** has `VAEncMiscParameterRIR` (rolling intra refresh) on Intel/AMD; NVIDIA's proprietary driver doesn't expose it through VAAPI even though the hardware supports it — that's why we keep NVENC as a separate impl on Linux too.
- **x264** has `x264_param_t.i_intra_refresh` (boolean) plus `x264_param_t.i_keyint_max=INFINITE` to suppress periodic I-frames; gradient direction is implied.

Budget ~1 day per encoder once the trait and tests are stable. The trait absorbs all this — callers see `Encoder::encode(...)` and don't care.

The second-hardest part is the bitrate controller. Naïve "if RTT > X drop bitrate by Y" loops oscillate. Use a proportional-only controller with deadband (no action if abs(error) < 1.5 Mbps for 200 ms) and a hard ceiling per direction (no more than ±25% per second). Plan a half-day of soak-test tuning at the end.

---

## File Structure

```
host/
├── Cargo.toml                              # workspace; add ix-codec, ix-rtc as members
└── crates/
    ├── ix-codec/
    │   ├── Cargo.toml                      # NEW
    │   ├── build.rs                        # NEW (bindgen for NVENC/oneVPL/AMF/x264)
    │   ├── src/
    │   │   ├── lib.rs                      # NEW (re-exports)
    │   │   ├── trait.rs                    # NEW (Encoder trait, EncodedSlice, PeerCaps, Negotiated)
    │   │   ├── probe.rs                    # NEW (runtime encoder availability probe)
    │   │   ├── nvenc_hevc.rs               # NEW
    │   │   ├── nvenc_av1.rs                # NEW
    │   │   ├── qsv_hevc.rs                 # NEW
    │   │   ├── amf_hevc.rs                 # NEW
    │   │   ├── vaapi_hevc.rs               # NEW
    │   │   ├── x264_sw.rs                  # NEW
    │   │   └── shared_config.rs            # NEW (the "ultralowlatency + intra-refresh" config builder)
    │   └── tests/
    │       ├── trait_contract.rs           # NEW (tests every impl honors the trait the same way)
    │       └── selection_logic.rs          # NEW (probe ordering + capability negotiation)
    ├── ix-rtc/
    │   ├── Cargo.toml                      # NEW
    │   ├── src/
    │   │   ├── lib.rs                      # NEW
    │   │   ├── peer.rs                     # NEW (Peer type — wraps RTCPeerConnection)
    │   │   ├── signaling.rs                # NEW (SDP offer/answer + cert pinning hook)
    │   │   ├── channels.rs                 # NEW (input + control DataChannel typed wrappers)
    │   │   ├── video_track.rs              # NEW (HEVC/AV1 RTP packetizer)
    │   │   ├── bitrate_controller.rs       # NEW (transport-CC → bitrate target)
    │   │   ├── heartbeat.rs                # NEW (250ms cadence, 4-missed disconnect)
    │   │   └── codec_pref.rs               # NEW (SDP capability ordering — this is where M4-iPad gating lives)
    │   └── tests/
    │       ├── sdp_negotiation.rs          # NEW (codec ordering, M4-iPad gating, fallback chain)
    │       ├── bitrate_step_response.rs    # NEW (synthetic transport-CC packet loss → expected bitrate target)
    │       └── heartbeat_state.rs          # NEW (4 missed = disconnect; 1 missed → recovers)
    └── iextendd/
        ├── src/
        │   └── session.rs                  # NEW (capture → encode → transport wiring)
        └── examples/
            └── smoke_loopback.rs           # NEW (in-process latency benchmark, asserts p99 < 30 ms)
```

**File-size targets:** No file exceeds 350 lines. The `Encoder` trait file is ~120 lines; each encoder impl 250–350 lines (most of that is unsafe FFI wrangling in initialization — the steady-state encode hot path is short). `Peer` is ~280 lines; everything else under 200.

**Why this decomposition:** the Encoder trait absorbs vendor-SDK weirdness so `ix-rtc` and `iextendd::session` never touch a C header. `ix-rtc` keeps the WebRTC concerns (SDP, RTP, DataChannels, congestion control) separate from codec concerns. `session.rs` is the only file that knows about all three (capture, encode, transport) and it stays small because each subsystem is already self-contained.

---

## Workspace setup

The `host/` workspace was bootstrapped by Plan 2 with crates `iextendd`, `iextend-tray`, `ix-types`, `ix-discover`, `ix-display-windows`, `ix-display-linux`, `ix-input`. This plan adds `ix-codec` and `ix-rtc` as members. All tasks below assume the working directory is `host/` unless stated otherwise.

---

### Task 1: Add `ix-codec` and `ix-rtc` to the workspace

**Files:**
- Modify: `host/Cargo.toml`
- Create: `host/crates/ix-codec/Cargo.toml`
- Create: `host/crates/ix-rtc/Cargo.toml`
- Create: `host/crates/ix-codec/src/lib.rs` (placeholder)
- Create: `host/crates/ix-rtc/src/lib.rs` (placeholder)

- [ ] **Step 1: Add the new crates to the workspace members list**

Open `host/Cargo.toml` and add `"crates/ix-codec"` and `"crates/ix-rtc"` to the `members` array. Keep the array sorted alphabetically.

- [ ] **Step 2: Write `host/crates/ix-codec/Cargo.toml`**

```toml
[package]
name = "ix-codec"
version = "0.0.1"
edition = "2021"
license = "Apache-2.0"
publish = false

[dependencies]
ix-types = { path = "../ix-types" }
thiserror = "1"
tracing = "0.1"
bytes = "1"

[build-dependencies]
bindgen = "0.69"
cc = "1"

[dev-dependencies]
proptest = "1"
tempfile = "3"

[features]
default = []
nvenc = []
qsv = []
amf = []
vaapi = []
x264-sw = []
all-encoders = ["nvenc", "qsv", "amf", "vaapi", "x264-sw"]
```

The features default OFF. Each encoder is opt-in; the daemon enables what makes sense for the build target via the umbrella `all-encoders` feature.

- [ ] **Step 3: Write `host/crates/ix-rtc/Cargo.toml`**

```toml
[package]
name = "ix-rtc"
version = "0.0.1"
edition = "2021"
license = "Apache-2.0"
publish = false

[dependencies]
ix-codec = { path = "../ix-codec" }
ix-types = { path = "../ix-types" }
webrtc = "0.10"
tokio = { version = "1.35", features = ["rt-multi-thread", "macros", "sync", "time"] }
thiserror = "1"
tracing = "0.1"
bytes = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[dev-dependencies]
tokio = { version = "1.35", features = ["rt-multi-thread", "macros", "sync", "time", "test-util"] }
proptest = "1"
```

- [ ] **Step 4: Write placeholder `lib.rs` files**

`host/crates/ix-codec/src/lib.rs`:

```rust
//! ix-codec — encoder trait and platform-specific impls.
//!
//! See `crate::trait_::Encoder` for the public API.

pub mod r#trait;
pub mod probe;
pub mod shared_config;

pub use r#trait::*;
pub use probe::probe_available_encoders;
```

`host/crates/ix-rtc/src/lib.rs`:

```rust
//! ix-rtc — WebRTC peer connection (1 video track + 2 DataChannels).

pub mod peer;
pub mod channels;
pub mod video_track;
pub mod codec_pref;
pub mod bitrate_controller;
pub mod heartbeat;
pub mod signaling;

pub use peer::Peer;
```

- [ ] **Step 5: Verify the workspace compiles**

```bash
cd host
cargo check -p ix-codec -p ix-rtc
```

Expected: `error[E0432]: unresolved import` for the missing modules. **That's fine for this task** — modules get filled in by Tasks 2–13. The workspace structure is the deliverable for Task 1.

- [ ] **Step 6: Commit**

```bash
git add host/Cargo.toml host/crates/ix-codec host/crates/ix-rtc
git commit -m "chore(host): add ix-codec and ix-rtc workspace members"
```

---

### Task 2: Define the `Encoder` trait and supporting types

**Files:**
- Create: `host/crates/ix-codec/src/trait.rs`
- Modify: `host/crates/ix-codec/src/lib.rs`

This task defines the public surface that every encoder impl implements. Test-driven: write the contract first.

- [ ] **Step 1: Write the trait-contract test (failing)**

Create `host/crates/ix-codec/tests/trait_contract.rs`:

```rust
use ix_codec::{Encoder, EncodedSlice, EncoderKind, Negotiated, PeerCaps, Profile, ColorSpace};
use ix_types::{GpuFrame, Rect};

/// A do-nothing encoder used to verify the trait shape compiles and
/// the typestate of every method behaves predictably.
struct NullEncoder { kind: EncoderKind, bitrate_kbps: u32, force_kf: bool }

impl Encoder for NullEncoder {
    fn kind(&self) -> EncoderKind { self.kind }
    fn negotiate(&mut self, _peer: &PeerCaps) -> Negotiated {
        Negotiated { profile: Profile::HevcMain10, color: ColorSpace::Bt2020Pq }
    }
    fn encode(&mut self, _src: &GpuFrame, _dirty: &[Rect]) -> Result<EncodedSlice, ix_codec::Error> {
        let mut data = vec![0u8; 16];
        if self.force_kf { data[0] = 0x42; self.force_kf = false; }
        Ok(EncodedSlice { data, is_keyframe: data[0] == 0x42, pts_us: 0, slice_index: 0 })
    }
    fn force_keyframe(&mut self) { self.force_kf = true; }
    fn set_bitrate(&mut self, kbps: u32) { self.bitrate_kbps = kbps; }
}

#[test]
fn force_keyframe_takes_effect_on_next_encode() {
    let mut e = NullEncoder { kind: EncoderKind::X264SoftwareUlllSw, bitrate_kbps: 8000, force_kf: false };
    let frame = GpuFrame::dummy(1920, 1080);
    let slice = e.encode(&frame, &[]).unwrap();
    assert!(!slice.is_keyframe);
    e.force_keyframe();
    let slice = e.encode(&frame, &[]).unwrap();
    assert!(slice.is_keyframe);
    let slice = e.encode(&frame, &[]).unwrap();
    assert!(!slice.is_keyframe, "force_keyframe is one-shot");
}

#[test]
fn set_bitrate_is_idempotent() {
    let mut e = NullEncoder { kind: EncoderKind::X264SoftwareUlllSw, bitrate_kbps: 8000, force_kf: false };
    e.set_bitrate(40_000);
    e.set_bitrate(40_000);
    assert_eq!(e.bitrate_kbps, 40_000);
}

#[test]
fn negotiate_with_av1_capable_peer_offers_av1_when_kind_supports_it() {
    let mut e = NullEncoder { kind: EncoderKind::NvencAv1, bitrate_kbps: 8000, force_kf: false };
    let n = e.negotiate(&PeerCaps { av1_decode: true, hevc_decode: true, max_resolution: (3840, 2160), peer_kind: ix_codec::PeerKind::IpadProM4 });
    assert!(matches!(n.profile, Profile::HevcMain10 | Profile::Av1Main10));
}
```

Run: `cargo test -p ix-codec --test trait_contract`. Expected: compile errors (types don't exist).

- [ ] **Step 2: Define the trait and types**

Create `host/crates/ix-codec/src/trait.rs`:

```rust
//! Public encoder trait + value types.

use ix_types::{GpuFrame, Rect};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("encoder is not available on this host: {0}")]
    NotAvailable(String),
    #[error("encoder initialization failed: {0}")]
    Init(String),
    #[error("encode failed: {0}")]
    EncodeFailed(String),
    #[error("set_bitrate out of range: requested {requested_kbps}, allowed {min_kbps}–{max_kbps}")]
    BitrateOutOfRange { requested_kbps: u32, min_kbps: u32, max_kbps: u32 },
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum EncoderKind {
    NvencAv1,
    NvencHevc,
    QsvHevc,
    AmfHevc,
    VaapiHevc,
    X264SoftwareUlllSw,
}

impl EncoderKind {
    pub fn priority(self) -> u8 {
        match self {
            EncoderKind::NvencAv1   => 0, // best — but only if peer is M4
            EncoderKind::NvencHevc  => 1,
            EncoderKind::QsvHevc    => 1,
            EncoderKind::AmfHevc    => 1,
            EncoderKind::VaapiHevc  => 2,
            EncoderKind::X264SoftwareUlllSw => 99,
        }
    }
    pub fn is_software(self) -> bool { matches!(self, EncoderKind::X264SoftwareUlllSw) }
}

#[derive(Debug, Copy, Clone)]
pub enum Profile {
    HevcMain10,    // BT.2020 PQ HDR or BT.709 SDR
    Av1Main10,     // M4 iPad only on the decode side
    H264UlllFallback,
}

#[derive(Debug, Copy, Clone)]
pub enum ColorSpace {
    Bt2020Pq,      // HDR
    Bt709Sdr,
}

#[derive(Debug, Copy, Clone)]
pub enum PeerKind { IpadProM4, IpadProM2OrM1, IpadAirM, IpadAseries, Unknown }

#[derive(Debug, Clone)]
pub struct PeerCaps {
    pub av1_decode: bool,
    pub hevc_decode: bool,
    pub max_resolution: (u32, u32),
    pub peer_kind: PeerKind,
}

#[derive(Debug, Clone)]
pub struct Negotiated {
    pub profile: Profile,
    pub color: ColorSpace,
}

#[derive(Debug, Clone)]
pub struct EncodedSlice {
    pub data: Vec<u8>,         // raw NAL units (HEVC) or OBUs (AV1) — packetizer in ix-rtc handles RTP framing
    pub is_keyframe: bool,
    pub pts_us: i64,           // microseconds since session start
    pub slice_index: u32,      // 0..N for intra-refresh slice rotation; useful for telemetry
}

pub trait Encoder: Send {
    fn kind(&self) -> EncoderKind;
    fn negotiate(&mut self, peer: &PeerCaps) -> Negotiated;
    fn encode(&mut self, src: &GpuFrame, dirty: &[Rect]) -> Result<EncodedSlice, Error>;
    fn force_keyframe(&mut self);
    fn set_bitrate(&mut self, kbps: u32);
}
```

- [ ] **Step 3: Re-export from `lib.rs`**

`host/crates/ix-codec/src/lib.rs` already does `pub use r#trait::*;` from Task 1. Verify the re-exports compile:

```bash
cargo build -p ix-codec
```

Expected: clean build (probe and shared_config modules are still empty stubs but they don't reference missing types).

- [ ] **Step 4: Run the contract test, expect pass**

```bash
cargo test -p ix-codec --test trait_contract
```

Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add host/crates/ix-codec/src host/crates/ix-codec/tests/trait_contract.rs
git commit -m "feat(ix-codec): define Encoder trait and value types"
```

---

### Task 3: Runtime encoder probe (TDD)

**Files:**
- Create: `host/crates/ix-codec/src/probe.rs`
- Create: `host/crates/ix-codec/tests/selection_logic.rs`

The probe inspects the host once at daemon start: which encoder libraries are present, which GPU is installed, what the kernel says about `/dev/dri/renderD*` (Linux), what `nvenc.dll` / `mfx*.dll` resolve to (Windows). Returns `Vec<EncoderKind>` ordered by priority.

The probe is also where M4-iPad gating happens: even if the host has hardware AV1 encode, we don't add `EncoderKind::NvencAv1` to the candidate list unless the eventual peer announces M4-class decode in the SDP offer answer. This is enforced at session-build time, not probe time — the probe is host-only.

- [ ] **Step 1: Write the failing selection test**

Create `host/crates/ix-codec/tests/selection_logic.rs`:

```rust
use ix_codec::probe::{Probe, ProbeOutcome};
use ix_codec::{EncoderKind, PeerCaps, PeerKind};

#[test]
fn priority_order_is_stable_across_calls() {
    let outcome = Probe::synthetic_for_test(&[
        EncoderKind::NvencHevc,
        EncoderKind::QsvHevc,
        EncoderKind::X264SoftwareUlllSw,
    ]);
    let a: Vec<_> = outcome.iter().copied().collect();
    let outcome2 = Probe::synthetic_for_test(&[
        EncoderKind::NvencHevc,
        EncoderKind::QsvHevc,
        EncoderKind::X264SoftwareUlllSw,
    ]);
    let b: Vec<_> = outcome2.iter().copied().collect();
    assert_eq!(a, b);
}

#[test]
fn av1_candidate_only_offered_to_m4_peers() {
    let outcome = Probe::synthetic_for_test(&[
        EncoderKind::NvencAv1,
        EncoderKind::NvencHevc,
    ]);
    let m4 = PeerCaps { av1_decode: true, hevc_decode: true, max_resolution: (3840, 2160), peer_kind: PeerKind::IpadProM4 };
    let m2 = PeerCaps { av1_decode: false, hevc_decode: true, max_resolution: (2732, 2048), peer_kind: PeerKind::IpadProM2OrM1 };

    let for_m4 = outcome.candidates_for(&m4);
    let for_m2 = outcome.candidates_for(&m2);

    assert_eq!(for_m4.first(), Some(&EncoderKind::NvencAv1));
    assert!(for_m2.iter().all(|k| !matches!(k, EncoderKind::NvencAv1)));
}

#[test]
fn software_fallback_emits_warning_flag() {
    let outcome = Probe::synthetic_for_test(&[EncoderKind::X264SoftwareUlllSw]);
    assert!(outcome.software_fallback_only(),
        "if every available encoder is software, the daemon should warn the user about battery");
}
```

Run: `cargo test -p ix-codec --test selection_logic`. Expected: compile errors.

- [ ] **Step 2: Implement `probe.rs`**

Create `host/crates/ix-codec/src/probe.rs`:

```rust
//! Runtime encoder availability probe.
//!
//! `Probe::detect()` runs at daemon startup and returns the host-side list
//! of available encoder kinds. `ProbeOutcome::candidates_for(peer)` filters
//! that list against per-peer capabilities (notably the M4-iPad gate for AV1).

use crate::{EncoderKind, PeerCaps, PeerKind};

#[derive(Debug, Clone)]
pub struct ProbeOutcome {
    available: Vec<EncoderKind>,
}

impl ProbeOutcome {
    pub fn iter(&self) -> std::slice::Iter<EncoderKind> { self.available.iter() }
    pub fn software_fallback_only(&self) -> bool {
        !self.available.is_empty() && self.available.iter().all(|k| k.is_software())
    }
    /// Filter against a peer's decode caps. AV1 only goes to M4 iPads.
    pub fn candidates_for(&self, peer: &PeerCaps) -> Vec<EncoderKind> {
        let mut out: Vec<EncoderKind> = self.available.iter().copied()
            .filter(|k| match k {
                EncoderKind::NvencAv1 => peer.av1_decode && matches!(peer.peer_kind, PeerKind::IpadProM4),
                _ => peer.hevc_decode || k.is_software(),
            })
            .collect();
        out.sort_by_key(|k| k.priority());
        out
    }
}

pub struct Probe;

impl Probe {
    /// Test seam: skip OS detection, accept the candidate list as given.
    pub fn synthetic_for_test(candidates: &[EncoderKind]) -> ProbeOutcome {
        ProbeOutcome { available: candidates.to_vec() }
    }

    /// Real probe — runs at daemon startup. Cheap (<5 ms): just checks
    /// for the presence of vendor SDK shared libraries and DRI device nodes.
    pub fn detect() -> ProbeOutcome {
        let mut available = Vec::new();

        #[cfg(all(feature = "nvenc", target_os = "windows"))]
        if Self::nvenc_present_windows() { available.push(EncoderKind::NvencHevc); if Self::nvenc_av1_supported() { available.push(EncoderKind::NvencAv1); } }

        #[cfg(all(feature = "nvenc", target_os = "linux"))]
        if Self::nvenc_present_linux() { available.push(EncoderKind::NvencHevc); if Self::nvenc_av1_supported() { available.push(EncoderKind::NvencAv1); } }

        #[cfg(feature = "qsv")]
        if Self::qsv_present() { available.push(EncoderKind::QsvHevc); }

        #[cfg(all(feature = "amf", target_os = "windows"))]
        if Self::amf_present_windows() { available.push(EncoderKind::AmfHevc); }

        #[cfg(all(feature = "vaapi", target_os = "linux"))]
        if Self::vaapi_present_linux() { available.push(EncoderKind::VaapiHevc); }

        #[cfg(feature = "x264-sw")]
        available.push(EncoderKind::X264SoftwareUlllSw);

        ProbeOutcome { available }
    }

    // === platform probes — implemented in encoder-specific modules ===

    #[cfg(all(feature = "nvenc", target_os = "windows"))]
    fn nvenc_present_windows() -> bool { crate::nvenc_hevc::probe_windows() }
    #[cfg(all(feature = "nvenc", target_os = "linux"))]
    fn nvenc_present_linux() -> bool { crate::nvenc_hevc::probe_linux() }
    #[cfg(feature = "nvenc")]
    fn nvenc_av1_supported() -> bool { crate::nvenc_av1::probe_av1_capability() }
    #[cfg(feature = "qsv")]
    fn qsv_present() -> bool { crate::qsv_hevc::probe() }
    #[cfg(all(feature = "amf", target_os = "windows"))]
    fn amf_present_windows() -> bool { crate::amf_hevc::probe_windows() }
    #[cfg(all(feature = "vaapi", target_os = "linux"))]
    fn vaapi_present_linux() -> bool { crate::vaapi_hevc::probe_linux() }
}

pub fn probe_available_encoders() -> ProbeOutcome { Probe::detect() }
```

- [ ] **Step 3: Run the test, expect pass**

```bash
cargo test -p ix-codec --test selection_logic
```

Expected: 3 tests pass. (No feature flags enabled means `Probe::detect()` returns empty — but `synthetic_for_test` is what the tests use.)

- [ ] **Step 4: Commit**

```bash
git add host/crates/ix-codec/src/probe.rs host/crates/ix-codec/tests/selection_logic.rs
git commit -m "feat(ix-codec): runtime encoder probe with M4-iPad AV1 gating"
```

---

### Task 4: Shared encoder configuration (the "ultralowlatency + intra-refresh" preset)

**Files:**
- Create: `host/crates/ix-codec/src/shared_config.rs`

A single struct that captures the spec's encoder-config requirements (§5.2). Each encoder impl reads this struct and translates it to its vendor SDK at init time.

- [ ] **Step 1: Define `SharedConfig`**

Create `host/crates/ix-codec/src/shared_config.rs`:

```rust
//! Spec-derived shared encoder config (see spec §5.2).

use crate::{ColorSpace, Profile};

#[derive(Debug, Clone)]
pub struct SharedConfig {
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,         // 120 for max-push, 60 fallback
    pub fps_den: u32,         // always 1 in our use
    pub initial_bitrate_kbps: u32,
    pub min_bitrate_kbps: u32,        // 6000 for 1080p120 per §5.2
    pub max_bitrate_kbps: u32,        // 80_000
    pub profile: Profile,
    pub color: ColorSpace,
    pub intra_refresh_rows: u32,      // 16 — see spec §5.2 "16-row slice gradient"
}

impl SharedConfig {
    pub fn default_1080p120() -> Self {
        Self {
            width: 1920, height: 1200,         // host's virtual monitor mode at 16:10
            fps_num: 120, fps_den: 1,
            initial_bitrate_kbps: 25_000,
            min_bitrate_kbps: 6_000,
            max_bitrate_kbps: 80_000,
            profile: Profile::HevcMain10,
            color: ColorSpace::Bt2020Pq,
            intra_refresh_rows: 16,
        }
    }
    pub fn default_60fps_fallback(mut self) -> Self {
        self.fps_num = 60; self.min_bitrate_kbps = 4_000; self
    }
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo build -p ix-codec
```

- [ ] **Step 3: Commit**

```bash
git add host/crates/ix-codec/src/shared_config.rs
git commit -m "feat(ix-codec): shared encoder config (intra-refresh + ultralowlatency)"
```

---

### Task 5: Implement `NvencHevc`

**Files:**
- Create: `host/crates/ix-codec/build.rs`
- Create: `host/crates/ix-codec/src/nvenc_hevc.rs`
- Create: `host/crates/ix-codec/tests/nvenc_hevc_smoke.rs`

NVENC is the most-used encoder; do this one first to validate the trait. The SDK is downloaded out-of-band by the engineer running the build (NVIDIA forbids redistribution); `build.rs` fails with a clear message if the headers are absent.

- [ ] **Step 1: Write `build.rs`**

Create `host/crates/ix-codec/build.rs`:

```rust
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=NVIDIA_VIDEO_CODEC_SDK_DIR");

    #[cfg(feature = "nvenc")]
    nvenc_bindgen();
    #[cfg(feature = "qsv")]
    qsv_bindgen();
    #[cfg(feature = "amf")]
    amf_bindgen();
    #[cfg(feature = "x264-sw")]
    x264_link();
    #[cfg(feature = "vaapi")]
    vaapi_link();
}

#[cfg(feature = "nvenc")]
fn nvenc_bindgen() {
    let sdk_dir = std::env::var("NVIDIA_VIDEO_CODEC_SDK_DIR")
        .expect("set NVIDIA_VIDEO_CODEC_SDK_DIR to a downloaded copy of NVIDIA Video Codec SDK 12.2+ (https://developer.nvidia.com/nvidia-video-codec-sdk)");
    let header = format!("{}/Interface/nvEncodeAPI.h", sdk_dir);
    let bindings = bindgen::Builder::default()
        .header(&header)
        .allowlist_function("NvEncode.*")
        .allowlist_type("NV_ENC_.*")
        .allowlist_var("NV_ENC_.*")
        .generate().expect("nvenc bindgen failed");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    bindings.write_to_file(out.join("nvenc_bindings.rs")).unwrap();

    if cfg!(target_os = "windows") {
        println!("cargo:rustc-link-lib=dylib=nvencodeapi");
    } else {
        println!("cargo:rustc-link-lib=dylib=nvidia-encode");
    }
}

#[cfg(feature = "qsv")]
fn qsv_bindgen() {
    let bindings = bindgen::Builder::default()
        .header_contents("vpl.h", "#include <vpl/mfxvideo.h>\n#include <vpl/mfxstructures.h>\n")
        .allowlist_function("MFX.*").allowlist_type("mfx.*").allowlist_var("MFX_.*")
        .generate().expect("oneVPL bindgen failed (install libvpl-dev)");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    bindings.write_to_file(out.join("qsv_bindings.rs")).unwrap();
    println!("cargo:rustc-link-lib=dylib=vpl");
}

#[cfg(feature = "amf")]
fn amf_bindgen() {
    // AMF ships a public header tree; user installs the AMF SDK and points $AMF_HOME at it.
    let amf_home = std::env::var("AMF_HOME")
        .expect("set AMF_HOME to a downloaded copy of AMD AMF SDK (https://github.com/GPUOpen-LibrariesAndSDKs/AMF)");
    let bindings = bindgen::Builder::default()
        .header(format!("{}/amf/public/include/core/Result.h", amf_home))
        .header(format!("{}/amf/public/include/components/VideoEncoderHEVC.h", amf_home))
        .allowlist_function("AMF.*").allowlist_type("AMF.*").allowlist_var("AMF.*")
        .generate().expect("AMF bindgen failed");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    bindings.write_to_file(out.join("amf_bindings.rs")).unwrap();
}

#[cfg(feature = "x264-sw")]
fn x264_link() { println!("cargo:rustc-link-lib=dylib=x264"); }

#[cfg(feature = "vaapi")]
fn vaapi_link() { println!("cargo:rustc-link-lib=dylib=va"); println!("cargo:rustc-link-lib=dylib=va-drm"); }
```

- [ ] **Step 2: Write the `NvencHevc` impl**

Create `host/crates/ix-codec/src/nvenc_hevc.rs`. The full file is ~330 lines and follows this structure:

```rust
//! NVENC HEVC encoder (NVIDIA Video Codec SDK 12.2+).
//!
//! Wraps the C API behind the `Encoder` trait. Init opens an encode session,
//! configures it with intra-refresh + ultralowlatency tune, and binds the
//! input as DXGI shared texture (Windows) or CUDA device pointer (Linux).
//!
//! The per-frame hot path is `encode()`: presents the input texture, fires
//! `NvEncEncodePicture`, drains output bitstream, returns the slice.

#![cfg(feature = "nvenc")]
#![allow(non_snake_case, non_upper_case_globals, non_camel_case_types, dead_code)]

include!(concat!(env!("OUT_DIR"), "/nvenc_bindings.rs"));

use crate::shared_config::SharedConfig;
use crate::{ColorSpace, EncodedSlice, Encoder, EncoderKind, Error, Negotiated, PeerCaps, Profile};
use ix_types::{GpuFrame, Rect};
use std::ffi::c_void;
use std::ptr;

pub struct NvencHevc {
    session: *mut c_void,
    cfg: SharedConfig,
    pts_us_origin: std::time::Instant,
    next_slice_index: u32,
    pending_keyframe: bool,
    api: NV_ENCODE_API_FUNCTION_LIST,
}

unsafe impl Send for NvencHevc {} // session is single-threaded; we ensure it via &mut self

pub fn probe_windows() -> bool { unsafe { LoadLibraryA_safe(b"nvEncodeAPI64.dll\0") } }
pub fn probe_linux()   -> bool { std::path::Path::new("/usr/lib/x86_64-linux-gnu/libnvidia-encode.so.1").exists() }

impl NvencHevc {
    pub fn new(cfg: SharedConfig) -> Result<Self, Error> {
        // 1. Load nvEncodeAPI.dll / .so via the static dispatch table the SDK gives us.
        // 2. Open encode session: NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS, deviceType=CUDA or DIRECTX
        //    Lifetime: Drop calls NvEncDestroyEncoder(session).
        // 3. Configure: NV_ENC_INITIALIZE_PARAMS with:
        //      - encodeGUID = NV_ENC_CODEC_HEVC_GUID
        //      - presetGUID = NV_ENC_PRESET_LOW_LATENCY_HQ_GUID + tuningInfo=NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY
        //      - encodeWidth/Height from cfg
        //      - frameRateNum=cfg.fps_num, frameRateDen=1
        //      - encodeConfig.gopLength = NVENC_INFINITE_GOPLENGTH
        //      - encodeConfig.frameIntervalP = 1 (no B frames)
        //      - rcParams.rateControlMode = NV_ENC_PARAMS_RC_CBR_HQ
        //      - rcParams.averageBitRate = cfg.initial_bitrate_kbps * 1000
        //      - hevcConfig.intraRefreshPeriod = ceil(height / cfg.intra_refresh_rows)
        //      - hevcConfig.intraRefreshCnt = cfg.intra_refresh_rows
        //      - hevcConfig.pixelBitDepthMinus8 = 2 (Main10)
        //      - hevcConfig.outputColorPrimaries = NV_ENC_COLOR_PRIMARIES_BT2020
        //      - hevcConfig.outputTransferCharacteristics = NV_ENC_TRANSFER_CHARACTERISTIC_SMPTE2084 (PQ)
        //      - hevcConfig.outputColorMatrix = NV_ENC_MATRIX_COEFFICIENTS_BT2020_NCL
        //      - tier = NV_ENC_TIER_HEVC_HIGH (HDR Main10 requires High tier)
        // Return Self.
        unimplemented!("see comments above; ~120 lines of unsafe FFI")
    }
    fn translate_dirty(&self, dirty: &[Rect]) -> Vec<NV_ENC_INPUT_RESOURCE_OPENGL_TEX> {
        // NVENC supports per-frame ROI but not arbitrary dirty rects directly.
        // We use ROI to bias quantization toward updated regions (set ROI deltaQP=-3).
        // ~30 lines.
        unimplemented!()
    }
}

impl Encoder for NvencHevc {
    fn kind(&self) -> EncoderKind { EncoderKind::NvencHevc }

    fn negotiate(&mut self, peer: &PeerCaps) -> Negotiated {
        // We're HEVC-only here; AV1 is its own impl.
        Negotiated {
            profile: Profile::HevcMain10,
            color: if peer.peer_kind_supports_hdr() { ColorSpace::Bt2020Pq } else { ColorSpace::Bt709Sdr },
        }
    }

    fn encode(&mut self, src: &GpuFrame, dirty: &[Rect]) -> Result<EncodedSlice, Error> {
        // 1. Map input texture into the encoder via NvEncRegisterResource (idempotent — cache by GpuFrame's handle).
        // 2. Build NV_ENC_PIC_PARAMS:
        //      - inputBuffer = mapped resource
        //      - outputBitstream = preallocated output buffer
        //      - pictureType = self.pending_keyframe ? IDR : P
        //      - encodePicFlags = INTRA_REFRESH if cfg.intra_refresh_rows > 0
        //      - inputTimeStamp = pts in 90 kHz (multiply pts_us by 0.09)
        //      - if !dirty.is_empty(): build ROI map favoring dirty rects
        // 3. NvEncEncodePicture(session, &pic_params)
        // 4. NvEncLockBitstream → copy into Vec<u8> → NvEncUnlockBitstream
        // 5. Return EncodedSlice { is_keyframe: pending_keyframe, slice_index: self.next_slice_index, ... }
        // 6. self.pending_keyframe = false; self.next_slice_index = (self.next_slice_index + 1) % refresh_period;
        let _ = (src, dirty);
        unimplemented!("~80 lines on the hot path")
    }

    fn force_keyframe(&mut self) { self.pending_keyframe = true; }

    fn set_bitrate(&mut self, kbps: u32) {
        if kbps < self.cfg.min_bitrate_kbps || kbps > self.cfg.max_bitrate_kbps { return; }
        // NvEncReconfigureEncoder with rcParams.averageBitRate updated.
        // Reconfigure is cheap (<1 ms) and doesn't drop frames.
        unimplemented!("~20 lines")
    }
}

impl Drop for NvencHevc {
    fn drop(&mut self) {
        if !self.session.is_null() {
            unsafe { (self.api.nvEncDestroyEncoder.unwrap())(self.session); }
            self.session = ptr::null_mut();
        }
    }
}

unsafe fn LoadLibraryA_safe(_: &[u8]) -> bool { true /* stub */ }
```

The full implementation is mostly mechanical FFI: NVIDIA's SDK ships a `NvEncoder` C++ sample (Samples/NvEncoder/) that maps 1:1 onto this Rust impl. The trait surface is small enough that the bulk of work is in `new()` (init) and `encode()` (hot path).

- [ ] **Step 3: Write a smoke test**

Create `host/crates/ix-codec/tests/nvenc_hevc_smoke.rs`:

```rust
#![cfg(feature = "nvenc")]

use ix_codec::{Encoder, EncoderKind, shared_config::SharedConfig};
use ix_codec::nvenc_hevc::NvencHevc;
use ix_types::GpuFrame;

#[test]
fn nvenc_hevc_encodes_120_frames_under_500_ms() {
    if !ix_codec::probe::Probe::detect().iter().any(|k| matches!(k, EncoderKind::NvencHevc)) {
        eprintln!("skipping: no NVENC on this host");
        return;
    }
    let mut enc = NvencHevc::new(SharedConfig::default_1080p120()).expect("nvenc init");
    let frame = GpuFrame::dummy(1920, 1200);
    let start = std::time::Instant::now();
    for _ in 0..120 {
        enc.encode(&frame, &[]).expect("encode");
    }
    let elapsed = start.elapsed();
    assert!(elapsed.as_millis() < 500,
        "120 frames at 1080p120 should encode in <500 ms; got {} ms", elapsed.as_millis());
}
```

- [ ] **Step 4: Build and (optionally on real hardware) run**

```bash
cargo build -p ix-codec --features nvenc
cargo test -p ix-codec --features nvenc --test nvenc_hevc_smoke -- --include-ignored
```

On a host with NVENC, expect pass. Without, the test self-skips.

- [ ] **Step 5: Commit**

```bash
git add host/crates/ix-codec/build.rs host/crates/ix-codec/src/nvenc_hevc.rs host/crates/ix-codec/tests/nvenc_hevc_smoke.rs
git commit -m "feat(ix-codec): NVENC HEVC encoder with intra-refresh + ultralowlatency tune"
```

---

### Task 6: Implement `NvencAv1`

**Files:**
- Create: `host/crates/ix-codec/src/nvenc_av1.rs`

NvencAv1 is structurally identical to NvencHevc. The differences are:
- `encodeGUID = NV_ENC_CODEC_AV1_GUID` (RTX 40-series only — `NvEncodeAPI` returns `NV_ENC_ERR_UNSUPPORTED_DEVICE` on older cards; that's the signal `probe_av1_capability()` looks for).
- `av1Config` struct in place of `hevcConfig`. Same intra-refresh fields (`intraRefreshPeriod`, `intraRefreshCnt`).
- AV1 OBU framing in the bitstream output. The packetizer in `ix-rtc::video_track` will handle RFC 9335 packetization — encoder just emits OBUs.

- [ ] **Step 1: Implement (~280 lines, mirrors `nvenc_hevc.rs`)**

The shape is the same as Task 5. Differences spelled out above.

- [ ] **Step 2: Verify probe gating**

```bash
cargo test -p ix-codec --features nvenc --test selection_logic
```

Expected: `av1_candidate_only_offered_to_m4_peers` passes (already does from Task 3 — this confirms nothing regressed).

- [ ] **Step 3: Commit**

```bash
git add host/crates/ix-codec/src/nvenc_av1.rs
git commit -m "feat(ix-codec): NVENC AV1 encoder (RTX 40+ + M4-iPad gated)"
```

---

### Task 7: Implement `QsvHevc` (Intel Quick Sync via oneVPL)

**Files:**
- Create: `host/crates/ix-codec/src/qsv_hevc.rs`

Intel oneVPL is a successor to MSDK. The intra-refresh field is `mfxExtCodingOption2.IntRefType=MFX_REFRESH_HORIZONTAL` plus `IntRefCycleSize=intra_refresh_rows`.

- [ ] **Step 1: Implement (~310 lines)**

Three-stage init dance specific to oneVPL:
1. `MFXLoad()` → returns a `mfxLoader`.
2. `MFXCreateSession(loader, 0, &session)`.
3. Build `mfxVideoParam` with HEVC + intra-refresh + Main10, then `MFXVideoENCODE_Init(session, &params)`.

Hot-path encode:
1. `MFXVideoENCODE_EncodeFrameAsync(session, &ctrl, &surface_in, &bitstream_out, &sync_point)`.
2. `MFXVideoCORE_SyncOperation(session, sync_point, INFINITE)`.
3. Copy `bitstream_out.Data[0..DataLength]` into `EncodedSlice.data`.

- [ ] **Step 2: Smoke test (mirrors Task 5 Step 3)**

- [ ] **Step 3: Commit**

```bash
git add host/crates/ix-codec/src/qsv_hevc.rs host/crates/ix-codec/tests/qsv_hevc_smoke.rs
git commit -m "feat(ix-codec): Intel Quick Sync HEVC via oneVPL"
```

---

### Task 8: Implement `AmfHevc` and `VaapiHevc`

**Files:**
- Create: `host/crates/ix-codec/src/amf_hevc.rs`
- Create: `host/crates/ix-codec/src/vaapi_hevc.rs`

**AMF (Windows + Linux):** AMD's wrapper. Open question per the complexity note — `AMF_VIDEO_ENCODER_HEVC_INTRA_REFRESH_NUM_MBS_PER_SLOT` measures macroblocks. Conversion: `mbs_per_slot = ceil(height / 16) / intra_refresh_rows * ceil(width / 16)`. Example for 1920×1200 @ intra_refresh_rows=16: 75 MBs vertical / 16 = ~4.7 → 5 rows of 120 MBs/row = 600 MBs/slot.

**VAAPI (Linux only):** Intel and AMD on Linux can share this path; it's our portable Linux fallback. NVIDIA proprietary driver does NOT expose VAAPI rolling-intra-refresh, so we keep `VaapiHevc` AMD/Intel-only on Linux and rely on the dedicated NVENC path for NVIDIA-on-Linux.

- [ ] **Step 1: Implement amf_hevc.rs (~310 lines)**

- [ ] **Step 2: Implement vaapi_hevc.rs (~290 lines)**

VAAPI uses `vaCreateConfig` + `vaCreateContext` + `VAEncMiscParameterRIR`. Output via `vaMapBuffer` on coded buffer. Input is a `VASurfaceID` backed by a DMA-BUF imported from the capture pipeline (Plan 4 produces these).

- [ ] **Step 3: Smoke tests (one per encoder)**

- [ ] **Step 4: Commit**

```bash
git add host/crates/ix-codec/src/amf_hevc.rs host/crates/ix-codec/src/vaapi_hevc.rs host/crates/ix-codec/tests/amf_hevc_smoke.rs host/crates/ix-codec/tests/vaapi_hevc_smoke.rs
git commit -m "feat(ix-codec): AMD AMF + VAAPI HEVC encoders"
```

---

### Task 9: Implement `X264UltraLowLatency` (software fallback)

**Files:**
- Create: `host/crates/ix-codec/src/x264_sw.rs`

Software fallback. Used only when no hardware encoder is present. Emits a one-time tracing warning at session start so the daemon can surface "Software encoder — battery will drain fast" to the tray UI.

x264 config:
- `preset = "ultrafast"`
- `tune = "zerolatency"`
- `i_keyint_max = X264_KEYINT_MAX_INFINITE`
- `i_intra_refresh = 1`
- `b_repeat_headers = 1` (so each slice carries SPS/PPS — no separate keyframe needed)
- `i_threads = num_cpus::get_physical()` (oversubscribing hurts more than it helps in low-latency mode)

- [ ] **Step 1: Implement (~220 lines)**

```rust
//! x264 software fallback. Emits a battery-warning event on session start.

#![cfg(feature = "x264-sw")]
#![allow(non_snake_case)]

use crate::{Encoder, EncoderKind, EncodedSlice, Error, Negotiated, PeerCaps, Profile, ColorSpace};
use crate::shared_config::SharedConfig;
use ix_types::{GpuFrame, Rect};
use tracing::warn;

pub struct X264Sw { /* ... */ }

impl X264Sw {
    pub fn new(cfg: SharedConfig) -> Result<Self, Error> {
        warn!(target: "ix_codec::x264_sw",
            "Using software encoder — laptop battery will drain ~3-4× faster than with a hardware encoder. \
             Consider plugging in or using a hardware-encoder-capable GPU.");
        // x264_param_default_preset(&p, "ultrafast", "zerolatency");
        // ... configure ...
        // x264_encoder_open(&p)
        unimplemented!()
    }
}

impl Encoder for X264Sw {
    fn kind(&self) -> EncoderKind { EncoderKind::X264SoftwareUlllSw }
    fn negotiate(&mut self, _peer: &PeerCaps) -> Negotiated {
        // x264 emits H.264 (AVC), not HEVC — fallback Profile.
        Negotiated { profile: Profile::H264UlllFallback, color: ColorSpace::Bt709Sdr }
    }
    fn encode(&mut self, src: &GpuFrame, _dirty: &[Rect]) -> Result<EncodedSlice, Error> {
        // x264_picture_t with src.as_yuv420p() — software encoder doesn't get GPU textures
        // x264_encoder_encode → grab NAL array
        let _ = src; unimplemented!()
    }
    fn force_keyframe(&mut self) { /* set IDR on next encode */ }
    fn set_bitrate(&mut self, kbps: u32) { let _ = kbps; /* x264_encoder_reconfig */ }
}
```

- [ ] **Step 2: Smoke test**

The smoke test does NOT skip on hosts without hardware encoders — x264 always works.

- [ ] **Step 3: Commit**

```bash
git add host/crates/ix-codec/src/x264_sw.rs host/crates/ix-codec/tests/x264_sw_smoke.rs
git commit -m "feat(ix-codec): x264 software fallback with battery-drain warning"
```

---

### Task 10: Scaffold `ix-rtc::Peer` (peer connection lifecycle)

**Files:**
- Create: `host/crates/ix-rtc/src/peer.rs`

`Peer` owns the `RTCPeerConnection`, the `VideoTrack`, both DataChannels, and the bitrate controller. Construction is async because WebRTC handshake is async.

- [ ] **Step 1: Write the failing test**

Create `host/crates/ix-rtc/tests/peer_construction.rs`:

```rust
use ix_rtc::Peer;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn peer_construction_happy_path() {
    let peer = Peer::builder().await.expect("peer build").build().await.expect("peer ready");
    assert!(peer.is_idle());
    drop(peer); // ensures clean shutdown without panic
}
```

- [ ] **Step 2: Implement `peer.rs` (~280 lines)**

```rust
//! WebRTC peer connection — owns the video track and 2 DataChannels.

use crate::bitrate_controller::BitrateController;
use crate::channels::{ControlChannel, InputChannel};
use crate::heartbeat::Heartbeat;
use crate::video_track::VideoSink;
use ix_codec::{Encoder, PeerCaps};
use std::sync::Arc;
use tokio::sync::Mutex;
use webrtc::api::APIBuilder;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::ice_transport::ice_server::RTCIceServer;

pub struct Peer {
    rtc: Arc<RTCPeerConnection>,
    pub video: VideoSink,
    pub input: InputChannel,
    pub control: ControlChannel,
    bitrate: Arc<Mutex<BitrateController>>,
    heartbeat: Arc<Mutex<Heartbeat>>,
    encoder: Arc<Mutex<Box<dyn Encoder>>>,
}

pub struct PeerBuilder { /* … */ }

impl Peer {
    pub fn builder() -> PeerBuilder { PeerBuilder::new() }
    pub fn is_idle(&self) -> bool { /* peer state == New */ unimplemented!() }
    pub async fn shutdown(self) -> Result<(), webrtc::Error> { /* close everything cleanly */ unimplemented!() }
}

impl PeerBuilder {
    fn new() -> Self { /* … */ unimplemented!() }
    pub async fn build(self) -> Result<Peer, webrtc::Error> {
        let api = APIBuilder::new().build();
        let cfg = RTCConfiguration {
            ice_servers: vec![],     // host-only candidates: STUN/TURN explicitly empty (LAN-only)
            ..Default::default()
        };
        let pc = api.new_peer_connection(cfg).await?;
        // ... wire video track + data channels ...
        unimplemented!()
    }
}
```

- [ ] **Step 3: Run test, expect pass**

```bash
cargo test -p ix-rtc --test peer_construction
```

- [ ] **Step 4: Commit**

```bash
git add host/crates/ix-rtc/src/peer.rs host/crates/ix-rtc/tests/peer_construction.rs
git commit -m "feat(ix-rtc): Peer scaffold with empty ICE server list (LAN-only)"
```

---

### Task 11: SDP codec preference and capability ordering

**Files:**
- Create: `host/crates/ix-rtc/src/codec_pref.rs`
- Create: `host/crates/ix-rtc/tests/sdp_negotiation.rs`

Where the runtime decision "which encoder do we open?" lives. Reads the probed `EncoderKind` list, intersects with the peer's SDP-advertised decode caps, and picks the highest-priority match.

- [ ] **Step 1: Write the test**

```rust
use ix_codec::{EncoderKind, PeerCaps, PeerKind};
use ix_rtc::codec_pref::{negotiate, NegotiationOutcome};

#[test]
fn m4_peer_with_av1_host_picks_av1() {
    let host = vec![EncoderKind::NvencAv1, EncoderKind::NvencHevc, EncoderKind::X264SoftwareUlllSw];
    let peer = PeerCaps { av1_decode: true, hevc_decode: true, max_resolution: (3840, 2160), peer_kind: PeerKind::IpadProM4 };
    assert_eq!(negotiate(&host, &peer).picked, EncoderKind::NvencAv1);
}

#[test]
fn m2_peer_falls_back_to_hevc() {
    let host = vec![EncoderKind::NvencAv1, EncoderKind::NvencHevc, EncoderKind::X264SoftwareUlllSw];
    let peer = PeerCaps { av1_decode: false, hevc_decode: true, max_resolution: (2732, 2048), peer_kind: PeerKind::IpadProM2OrM1 };
    assert_eq!(negotiate(&host, &peer).picked, EncoderKind::NvencHevc);
}

#[test]
fn no_overlap_returns_software_fallback() {
    let host = vec![EncoderKind::X264SoftwareUlllSw];
    let peer = PeerCaps { av1_decode: false, hevc_decode: false, max_resolution: (1920, 1080), peer_kind: PeerKind::IpadAseries };
    assert_eq!(negotiate(&host, &peer).picked, EncoderKind::X264SoftwareUlllSw);
}
```

- [ ] **Step 2: Implement (~120 lines)**

```rust
use ix_codec::{EncoderKind, PeerCaps, PeerKind};

#[derive(Debug, Clone)]
pub struct NegotiationOutcome {
    pub picked: EncoderKind,
    pub fallback_chain: Vec<EncoderKind>,
}

pub fn negotiate(host: &[EncoderKind], peer: &PeerCaps) -> NegotiationOutcome {
    let mut candidates: Vec<EncoderKind> = host.iter().copied()
        .filter(|k| match k {
            EncoderKind::NvencAv1 => peer.av1_decode && matches!(peer.peer_kind, PeerKind::IpadProM4),
            _ => peer.hevc_decode || k.is_software(),
        }).collect();
    candidates.sort_by_key(|k| k.priority());
    let picked = *candidates.first().unwrap_or(&EncoderKind::X264SoftwareUlllSw);
    NegotiationOutcome { picked, fallback_chain: candidates }
}
```

- [ ] **Step 3: Test pass + commit**

```bash
cargo test -p ix-rtc --test sdp_negotiation
git add host/crates/ix-rtc/src/codec_pref.rs host/crates/ix-rtc/tests/sdp_negotiation.rs
git commit -m "feat(ix-rtc): SDP codec preference with M4-iPad AV1 gating"
```

---

### Task 12: Video track + RTP packetizer

**Files:**
- Create: `host/crates/ix-rtc/src/video_track.rs`

Wraps a `webrtc-rs` `TrackLocalStaticSample`. The packetizer takes `EncodedSlice.data` (raw HEVC NALs or AV1 OBUs) and runs the corresponding RFC packetization (RFC 7798 for HEVC, RFC 9335 for AV1). `webrtc-rs` 0.10+ has built-in packetizers for both.

- [ ] **Step 1: Implement (~180 lines)**

```rust
use ix_codec::{EncodedSlice, EncoderKind, Profile};
use std::sync::Arc;
use std::time::Duration;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::media::Sample;

pub struct VideoSink {
    track: Arc<TrackLocalStaticSample>,
    profile: Profile,
}

impl VideoSink {
    pub fn new(profile: Profile) -> Self {
        let mime = match profile {
            Profile::HevcMain10 => "video/H265",
            Profile::Av1Main10 => "video/AV1",
            Profile::H264UlllFallback => "video/H264",
        };
        let track = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability { mime_type: mime.to_string(), ..Default::default() },
            "video".to_string(), "iextend".to_string(),
        ));
        Self { track, profile }
    }
    pub fn track(&self) -> Arc<TrackLocalStaticSample> { self.track.clone() }

    pub async fn write_slice(&self, slice: &EncodedSlice) -> Result<(), webrtc::Error> {
        let sample = Sample {
            data: bytes::Bytes::copy_from_slice(&slice.data),
            duration: Duration::from_micros(8_333),  // ~120 fps; bitrate-controller fixes if fps changes
            timestamp: std::time::SystemTime::UNIX_EPOCH + Duration::from_micros(slice.pts_us as u64),
            ..Default::default()
        };
        self.track.write_sample(&sample).await?;
        Ok(())
    }
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo build -p ix-rtc
```

- [ ] **Step 3: Commit**

```bash
git add host/crates/ix-rtc/src/video_track.rs
git commit -m "feat(ix-rtc): video track wrapping HEVC/AV1/H.264 sample writer"
```

---

### Task 13: DataChannels (input + control)

**Files:**
- Create: `host/crates/ix-rtc/src/channels.rs`

Two typed wrappers around `RTCDataChannel`. `input` is unreliable (`max_retransmits=0`, `ordered=false`); `control` is reliable+ordered.

- [ ] **Step 1: Implement (~190 lines)**

```rust
use std::sync::Arc;
use bytes::Bytes;
use webrtc::data_channel::data_channel_init::RTCDataChannelInit;
use webrtc::data_channel::RTCDataChannel;
use webrtc::peer_connection::RTCPeerConnection;
use serde::{Serialize, Deserialize};

pub struct InputChannel { dc: Arc<RTCDataChannel> }
pub struct ControlChannel { dc: Arc<RTCDataChannel> }

impl InputChannel {
    pub async fn create(pc: &RTCPeerConnection) -> Result<Self, webrtc::Error> {
        let dc = pc.create_data_channel("input", Some(RTCDataChannelInit {
            ordered: Some(false),
            max_retransmits: Some(0),
            ..Default::default()
        })).await?;
        Ok(Self { dc })
    }
    pub async fn send(&self, packet: &[u8; 32]) -> Result<(), webrtc::Error> {
        self.dc.send(&Bytes::copy_from_slice(packet)).await?; Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ControlMessage {
    Heartbeat { seq: u32, sent_us: i64 },
    HeartbeatAck { ack_seq: u32, recv_us: i64, send_us: i64 },
    SetMode { mode: String },
    Cursor { x_q16: i32, y_q16: i32, sprite_id: u32, hotspot: (i16, i16) },
    LatencyProbe { id: u32, sent_us: i64 },
    LatencyProbeAck { id: u32, recv_us: i64 },
    EndSession { reason: String },
    Background, Foreground,
}

impl ControlChannel {
    pub async fn create(pc: &RTCPeerConnection) -> Result<Self, webrtc::Error> {
        let dc = pc.create_data_channel("control", Some(RTCDataChannelInit {
            ordered: Some(true), ..Default::default()
        })).await?;
        Ok(Self { dc })
    }
    pub async fn send(&self, msg: &ControlMessage) -> Result<(), webrtc::Error> {
        let bytes = serde_json::to_vec(msg).map_err(|e| webrtc::Error::new(format!("serde: {e}")))?;
        self.dc.send(&Bytes::from(bytes)).await?; Ok(())
    }
    pub async fn on_message<F>(&self, mut on: F)
    where F: FnMut(ControlMessage) + Send + 'static {
        let dc = self.dc.clone();
        dc.on_message(Box::new(move |msg| {
            if let Ok(parsed) = serde_json::from_slice::<ControlMessage>(&msg.data) { on(parsed); }
            Box::pin(async {})
        })).await;
    }
}
```

- [ ] **Step 2: Test (channels are hard to test without a peer; defer end-to-end to Task 18)**

- [ ] **Step 3: Commit**

```bash
git add host/crates/ix-rtc/src/channels.rs
git commit -m "feat(ix-rtc): typed input + control DataChannels"
```

---

### Task 14: Heartbeat (250 ms cadence, 4-missed disconnect)

**Files:**
- Create: `host/crates/ix-rtc/src/heartbeat.rs`
- Create: `host/crates/ix-rtc/tests/heartbeat_state.rs`

The heartbeat is a state machine that fires `ControlMessage::Heartbeat` every 250 ms and tracks acks. After 4 consecutive timeouts (1 s) it raises a disconnect event. Spec §5.3 + §9.

- [ ] **Step 1: Test first**

```rust
use std::time::{Duration, Instant};
use ix_rtc::heartbeat::{Heartbeat, HeartbeatEvent};

#[test]
fn four_missed_acks_signals_disconnect() {
    let mut hb = Heartbeat::new(Duration::from_millis(250));
    let t0 = Instant::now();
    for i in 0..4 {
        let _ = hb.tick(t0 + Duration::from_millis(250 * (i + 1)));
        // (no ack arrives)
    }
    let evt = hb.tick(t0 + Duration::from_millis(250 * 5));
    assert_eq!(evt, Some(HeartbeatEvent::Disconnected));
}

#[test]
fn one_late_ack_is_recoverable() {
    let mut hb = Heartbeat::new(Duration::from_millis(250));
    let t0 = Instant::now();
    let seq = hb.tick(t0 + Duration::from_millis(250)).expect_send();
    // skip the next 2 cycles, then ack arrives late
    let _ = hb.tick(t0 + Duration::from_millis(500));
    let _ = hb.tick(t0 + Duration::from_millis(750));
    hb.on_ack(seq, t0 + Duration::from_millis(800));
    let evt = hb.tick(t0 + Duration::from_millis(1000));
    assert_ne!(evt, Some(HeartbeatEvent::Disconnected));
}
```

- [ ] **Step 2: Implement (~140 lines)**

- [ ] **Step 3: Run + commit**

```bash
cargo test -p ix-rtc --test heartbeat_state
git add host/crates/ix-rtc/src/heartbeat.rs host/crates/ix-rtc/tests/heartbeat_state.rs
git commit -m "feat(ix-rtc): heartbeat with 250 ms cadence and 4-missed disconnect"
```

---

### Task 15: Bitrate controller (transport-CC → encoder)

**Files:**
- Create: `host/crates/ix-rtc/src/bitrate_controller.rs`
- Create: `host/crates/ix-rtc/tests/bitrate_step_response.rs`

Subscribes to `webrtc-rs` transport-CC feedback events, runs a proportional controller with deadband, calls `Encoder::set_bitrate(kbps)`. See the complexity note at the top: this is the part most likely to need soak-test tuning.

- [ ] **Step 1: Test the step response**

```rust
use ix_rtc::bitrate_controller::BitrateController;
use std::time::Duration;

#[test]
fn loss_increase_drops_bitrate_within_one_second() {
    let mut bc = BitrateController::new(25_000, 6_000, 80_000);
    // Synthetic transport-CC samples: 5% loss, RTT stable.
    for _ in 0..10 {
        bc.on_feedback(loss_pct(5.0), rtt_ms(20.0), Duration::from_millis(100));
    }
    assert!(bc.target_kbps() < 25_000, "5% loss should bring bitrate down");
    assert!(bc.target_kbps() >= 6_000, "but not below the floor");
}

#[test]
fn deadband_suppresses_oscillation_under_clean_link() {
    let mut bc = BitrateController::new(25_000, 6_000, 80_000);
    let initial = bc.target_kbps();
    for _ in 0..100 {
        bc.on_feedback(loss_pct(0.05), rtt_ms(15.0 + (rand::random::<f32>() - 0.5)), Duration::from_millis(100));
    }
    assert!((bc.target_kbps() as i64 - initial as i64).abs() < 1500,
        "small RTT jitter inside deadband should not move bitrate by more than 1.5 Mbps");
}
```

- [ ] **Step 2: Implement (~180 lines)**

Proportional controller with:
- Error metric: weighted combo of loss% and RTT delta from baseline
- Deadband: ±1.5 Mbps for 200 ms
- Rate-of-change cap: ±25% per second
- Floor and ceiling from `SharedConfig`

- [ ] **Step 3: Run + commit**

```bash
cargo test -p ix-rtc --test bitrate_step_response
git add host/crates/ix-rtc/src/bitrate_controller.rs host/crates/ix-rtc/tests/bitrate_step_response.rs
git commit -m "feat(ix-rtc): proportional bitrate controller with deadband"
```

---

### Task 16: Signaling (SDP offer/answer + cert pinning hook)

**Files:**
- Create: `host/crates/ix-rtc/src/signaling.rs`

LAN-only: SDP swap goes through the local mDNS service record (Plan 7 owns mDNS). This file just exposes the signaling-state machine: `create_offer()`, `apply_answer(sdp)`, `create_answer_for(offer_sdp)`. The cert-pinning hook is a callback the daemon supplies — it's where `ix-discover` (Plan 7) plugs in once that lands.

- [ ] **Step 1: Implement (~150 lines)**

```rust
use std::sync::Arc;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;

pub struct Signaling {
    pc: Arc<RTCPeerConnection>,
    pin_check: Box<dyn Fn(&[u8]) -> bool + Send + Sync>,
}

impl Signaling {
    pub fn new(pc: Arc<RTCPeerConnection>, pin_check: impl Fn(&[u8]) -> bool + Send + Sync + 'static) -> Self {
        Self { pc, pin_check: Box::new(pin_check) }
    }

    pub async fn create_offer(&self) -> Result<RTCSessionDescription, webrtc::Error> {
        let offer = self.pc.create_offer(None).await?;
        self.pc.set_local_description(offer.clone()).await?;
        Ok(offer)
    }

    pub async fn apply_answer(&self, answer: RTCSessionDescription, peer_cert_der: &[u8]) -> Result<(), webrtc::Error> {
        if !(self.pin_check)(peer_cert_der) {
            return Err(webrtc::Error::new("peer cert not in pinned set".into()));
        }
        self.pc.set_remote_description(answer).await
    }

    pub async fn create_answer_for(&self, offer: RTCSessionDescription, peer_cert_der: &[u8]) -> Result<RTCSessionDescription, webrtc::Error> {
        if !(self.pin_check)(peer_cert_der) {
            return Err(webrtc::Error::new("peer cert not in pinned set".into()));
        }
        self.pc.set_remote_description(offer).await?;
        let answer = self.pc.create_answer(None).await?;
        self.pc.set_local_description(answer.clone()).await?;
        Ok(answer)
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add host/crates/ix-rtc/src/signaling.rs
git commit -m "feat(ix-rtc): signaling with cert-pinning hook"
```

---

### Task 17: `iextendd::session` — wire capture → encode → transport

**Files:**
- Create: `host/crates/iextendd/src/session.rs`
- Modify: `host/crates/iextendd/src/main.rs` (just to register the new module)

The wiring file. Spawns three tokio tasks:
1. Capture pump: pulls `GpuFrame` from `DisplaySource`, pushes to ring buffer.
2. Encode pump: pops from ring buffer, calls `Encoder::encode()`, pushes `EncodedSlice` to a tokio::mpsc.
3. Transport pump: pops `EncodedSlice` from mpsc, calls `VideoSink::write_slice()`. Also reads transport-CC feedback and calls `BitrateController::on_feedback()` → `Encoder::set_bitrate()` on a 250 ms tick.

- [ ] **Step 1: Implement (~250 lines)**

```rust
use crossbeam_queue::ArrayQueue;
use ix_codec::Encoder;
use ix_rtc::Peer;
use ix_types::{DisplaySource, GpuFrame};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

pub struct Session {
    capture: Arc<Mutex<Box<dyn DisplaySource>>>,
    encoder: Arc<Mutex<Box<dyn Encoder>>>,
    peer:    Arc<Peer>,
    queue:   Arc<ArrayQueue<GpuFrame>>,
}

impl Session {
    pub fn new(
        capture: Box<dyn DisplaySource>,
        encoder: Box<dyn Encoder>,
        peer:    Peer,
    ) -> Self {
        Self {
            capture: Arc::new(Mutex::new(capture)),
            encoder: Arc::new(Mutex::new(encoder)),
            peer:    Arc::new(peer),
            queue:   Arc::new(ArrayQueue::new(8)),  // 8-frame backlog → drops if encoder falls behind
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let cap_handle = self.spawn_capture().await;
        let enc_handle = self.spawn_encode().await;
        let bcr_handle = self.spawn_bitrate_controller().await;
        info!("session running");
        let _ = tokio::join!(cap_handle, enc_handle, bcr_handle);
        Ok(())
    }
    // each spawn_* method ~50 lines, all isolated.
    async fn spawn_capture(&self) -> tokio::task::JoinHandle<()> { unimplemented!() }
    async fn spawn_encode(&self)  -> tokio::task::JoinHandle<()> { unimplemented!() }
    async fn spawn_bitrate_controller(&self) -> tokio::task::JoinHandle<()> { unimplemented!() }
}
```

Wire into `main.rs`:

```rust
pub mod session;
```

- [ ] **Step 2: `cargo check -p iextendd`**

- [ ] **Step 3: Commit**

```bash
git add host/crates/iextendd/src/session.rs host/crates/iextendd/src/main.rs
git commit -m "feat(iextendd): session wires capture → encode → transport"
```

---

### Task 18: `smoke_loopback` example (in-process latency benchmark)

**Files:**
- Create: `host/crates/iextendd/examples/smoke_loopback.rs`

The integration test. Runs everything in-process: a `FakeFrameSource` that emits a deterministic 1080p120 timecode pattern, the negotiated encoder, an in-process WebRTC loopback (two peers in the same tokio runtime), and a frame round-tripper that reads the decoded frame back via the data channel and computes latency.

This binary is what we point at in CI to catch regressions. The spec's CI test (§10.1) is exactly this smoke binary.

- [ ] **Step 1: Implement (~280 lines)**

```rust
//! In-process WebRTC loopback latency benchmark.
//!
//! Two peers in one process: A produces frames + encodes + sends video,
//! B receives + decodes (well — counts samples; we don't actually decode in-process,
//! we just timestamp them on receipt) + sends back a "got slice N" message
//! on the control channel. A measures wall-clock RTT.
//!
//! Asserts: p99 < 30 ms over 1200 frames (10 s at 120 fps).

use ix_codec::{probe::Probe, Encoder, EncoderKind, PeerCaps, PeerKind, shared_config::SharedConfig};
use ix_rtc::Peer;
use ix_rtc::channels::ControlMessage;
use ix_types::GpuFrame;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt::init();

    // 1. Probe + pick encoder. Skip cleanly if no encoder is available.
    let probe = Probe::detect();
    if probe.iter().count() == 0 {
        eprintln!("smoke_loopback: no encoders available; skipping");
        return Ok(());
    }
    let peer_caps = PeerCaps {
        av1_decode: false,    // pretend we're an M2 — pick HEVC
        hevc_decode: true,
        max_resolution: (1920, 1200),
        peer_kind: PeerKind::IpadProM2OrM1,
    };
    let candidates = probe.candidates_for(&peer_caps);
    let kind = *candidates.first().expect("at least one candidate");
    println!("smoke_loopback: using encoder {kind:?}");

    let mut encoder: Box<dyn Encoder> = open_encoder(kind, SharedConfig::default_1080p120())?;

    // 2. Build two peers, wire their SDP exchange via in-process channels.
    let peer_a = Peer::builder().build().await?;
    let peer_b = Peer::builder().build().await?;
    in_process_sdp_exchange(&peer_a, &peer_b).await?;

    // 3. Round-trip channel: peer B sends a Heartbeat-like ack on receiving each video sample.
    let (latency_tx, mut latency_rx) = mpsc::unbounded_channel::<Duration>();
    peer_b.video.on_received_sample(move |slice_index, recv_at| {
        // ack via control channel
        let ack = ControlMessage::HeartbeatAck { ack_seq: slice_index, recv_us: 0, send_us: recv_at };
        // … send …
    });
    peer_a.control.on_message(move |msg| {
        if let ControlMessage::HeartbeatAck { ack_seq, .. } = msg {
            // look up send_at by ack_seq, push delta to latency_tx
        }
    }).await;

    // 4. Drive 1200 frames at 120 fps from the FakeFrameSource.
    let frame = GpuFrame::dummy(1920, 1200);
    let interval = Duration::from_micros(8_333);
    let mut next = Instant::now();
    for i in 0..1200u32 {
        tokio::time::sleep_until(next.into()).await;
        next += interval;

        let slice = encoder.encode(&frame, &[])?;
        peer_a.video.write_slice(&slice).await?;
    }

    // 5. Drain latency samples and assert p99 < 30 ms.
    let mut samples: Vec<Duration> = Vec::with_capacity(1200);
    while let Ok(d) = tokio::time::timeout(Duration::from_secs(2), latency_rx.recv()).await {
        if let Some(d) = d { samples.push(d); }
    }
    samples.sort();
    let p50 = samples[samples.len() / 2];
    let p99 = samples[(samples.len() * 99) / 100];
    println!("smoke_loopback: {} samples, p50={p50:?}, p99={p99:?}", samples.len());

    assert!(p99 < Duration::from_millis(30),
        "p99 round-trip latency must be < 30 ms (per spec §3); got {p99:?}");
    Ok(())
}

fn open_encoder(kind: EncoderKind, cfg: SharedConfig) -> Result<Box<dyn Encoder>, Box<dyn std::error::Error + Send + Sync>> {
    match kind {
        #[cfg(feature = "nvenc")] EncoderKind::NvencHevc => Ok(Box::new(ix_codec::nvenc_hevc::NvencHevc::new(cfg)?)),
        #[cfg(feature = "nvenc")] EncoderKind::NvencAv1  => Ok(Box::new(ix_codec::nvenc_av1::NvencAv1::new(cfg)?)),
        #[cfg(feature = "qsv")]   EncoderKind::QsvHevc   => Ok(Box::new(ix_codec::qsv_hevc::QsvHevc::new(cfg)?)),
        #[cfg(feature = "amf")]   EncoderKind::AmfHevc   => Ok(Box::new(ix_codec::amf_hevc::AmfHevc::new(cfg)?)),
        #[cfg(feature = "vaapi")] EncoderKind::VaapiHevc => Ok(Box::new(ix_codec::vaapi_hevc::VaapiHevc::new(cfg)?)),
        #[cfg(feature = "x264-sw")] EncoderKind::X264SoftwareUlllSw => Ok(Box::new(ix_codec::x264_sw::X264Sw::new(cfg)?)),
        _ => Err("encoder kind not built into this binary".into()),
    }
}

async fn in_process_sdp_exchange(_a: &Peer, _b: &Peer) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // a.create_offer() → b.create_answer_for(offer) → a.apply_answer(answer)
    // Cert pinning is a no-op (both peers use synthetic certs in this test).
    Ok(())
}
```

- [ ] **Step 2: Run on a real-encoder host**

```bash
cargo run -p iextendd --example smoke_loopback --release --features ix-codec/all-encoders
```

Expected output (on a host with NVENC):
```
smoke_loopback: using encoder NvencHevc
smoke_loopback: 1198 samples, p50=11ms, p99=24ms
```

- [ ] **Step 3: Add CI step that runs this with `--features x264-sw` so it always exercises something**

Edit `.github/workflows/ci.yml` (or equivalent) — Plan 2 should have set this up. Add a job:

```yaml
  smoke_loopback:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: sudo apt-get update && sudo apt-get install -y libx264-dev
      - run: cargo run -p iextendd --example smoke_loopback --release --features ix-codec/x264-sw
```

- [ ] **Step 4: Commit**

```bash
git add host/crates/iextendd/examples/smoke_loopback.rs .github/workflows/ci.yml
git commit -m "test(iextendd): in-process WebRTC loopback smoke benchmark (p99 < 30ms gate)"
```

---

### Task 19: Trait-contract sweep across all encoder impls

**Files:**
- Modify: `host/crates/ix-codec/tests/trait_contract.rs`

Re-run the trait-contract test against every real encoder (not the `NullEncoder` from Task 2). This catches inconsistencies — e.g., `force_keyframe` not actually emitting an IDR on the next encode, or `set_bitrate` swallowing out-of-range values silently in one impl but erroring in another.

- [ ] **Step 1: Add a parametric test**

Add to the existing `tests/trait_contract.rs`:

```rust
fn for_each_real_encoder(mut f: impl FnMut(Box<dyn Encoder>, EncoderKind)) {
    use ix_codec::shared_config::SharedConfig;
    let cfg = || SharedConfig::default_1080p120();
    #[cfg(feature = "nvenc")] if Probe::detect().iter().any(|k| matches!(k, EncoderKind::NvencHevc)) {
        f(Box::new(ix_codec::nvenc_hevc::NvencHevc::new(cfg()).unwrap()), EncoderKind::NvencHevc);
    }
    #[cfg(feature = "qsv")]   if Probe::detect().iter().any(|k| matches!(k, EncoderKind::QsvHevc)) {
        f(Box::new(ix_codec::qsv_hevc::QsvHevc::new(cfg()).unwrap()), EncoderKind::QsvHevc);
    }
    #[cfg(feature = "x264-sw")]
        f(Box::new(ix_codec::x264_sw::X264Sw::new(cfg()).unwrap()), EncoderKind::X264SoftwareUlllSw);
    // (etc. for amf, vaapi, nvenc-av1)
}

#[test]
fn force_keyframe_actually_emits_keyframe_on_real_impls() {
    let frame = GpuFrame::dummy(1920, 1200);
    for_each_real_encoder(|mut enc, kind| {
        let _ = enc.encode(&frame, &[]).unwrap();      // P
        enc.force_keyframe();
        let s = enc.encode(&frame, &[]).unwrap();
        assert!(s.is_keyframe, "{kind:?} did not emit keyframe after force_keyframe");
    });
}

#[test]
fn set_bitrate_within_range_does_not_error_on_real_impls() {
    for_each_real_encoder(|mut enc, _kind| {
        enc.set_bitrate(20_000);
        enc.set_bitrate(40_000);
        enc.set_bitrate(80_000);
    });
}
```

- [ ] **Step 2: Run on real hardware**

```bash
cargo test -p ix-codec --features all-encoders --test trait_contract -- --include-ignored
```

- [ ] **Step 3: Commit**

```bash
git add host/crates/ix-codec/tests/trait_contract.rs
git commit -m "test(ix-codec): cross-impl trait-contract sweep"
```

---

### Task 20: Tag the milestone

**Files:** none

- [ ] **Step 1: Verify everything is green**

```bash
cargo test -p ix-codec -p ix-rtc --features ix-codec/x264-sw
cargo run -p iextendd --example smoke_loopback --release --features ix-codec/x264-sw
```

Expected: all unit tests pass; smoke_loopback prints a p99 < 30 ms summary.

- [ ] **Step 2: Tag**

```bash
git tag -a plan-5-complete -m "Plan 5 of 10: WebRTC transport + multi-encoder dispatch shipped"
```

---

## Done criteria

All must be true to consider Plan 5 complete:

1. `host/crates/ix-codec/` exposes `Encoder` trait + 6 impls (NvencHevc, NvencAv1, QsvHevc, AmfHevc, VaapiHevc, X264Sw). Each is feature-gated.
2. `host/crates/ix-rtc/` exposes `Peer` (1 video track + 2 DataChannels), the bitrate controller, and the heartbeat.
3. `iextendd::session` wires capture → encode → transport.
4. `smoke_loopback` runs end-to-end and asserts p99 < 30 ms.
5. Every real encoder impl passes the trait-contract sweep.
6. CI runs `smoke_loopback` with the x264 fallback (so the gate fires on machines without GPUs).
7. Tag `plan-5-complete` is on the head of `main`.

## Out of scope (later plans)

- **Plan 3/4** — capture pipeline. This plan uses a `FakeFrameSource` for the smoke test; real DXGI / PipeWire capture is in those plans.
- **Plan 6** — iPad app. Real WebRTC peer at the other end. The smoke test fakes the iPad as another in-process `Peer`.
- **Plan 7** — pairing + mDNS discovery. The cert-pinning hook in `signaling.rs` is wired here, but the actual pinned-cert store is owned by Plan 7.
- **Plan 8** — input forwarding. The `input` DataChannel exists here but only carries test packets; real touch/Pencil packets are Plan 8.
- **Cursor reprojection (spec §6.4)** — the `cursor` control message is defined in `channels.rs::ControlMessage`, but the iPad-side reprojection shader is Plan 6, and the host-side cursor-luma-key encoder hint is Plan 8.
- **HDR pass-through end-to-end** — this plan supports HDR Main10 in the encoder config, but the iPad's PQ display latch is Plan 6.
- **Codec-fallback chain runtime switching** (AV1 → HEVC → H.264 → 60 fps → Failed) — `codec_pref::negotiate()` returns the full chain, but the actual mid-session switch (with its 80–150 ms blip) is wired in Plan 6 once both ends are running.

## Notes for whoever implements this

- **NVENC SDK is a download, not a crate.** Add it to a `tools/` directory on the build machine and set `NVIDIA_VIDEO_CODEC_SDK_DIR`. Do not commit the SDK.
- **AMF SDK same situation.** `AMF_HOME` env var.
- **Linux: install `libvpl-dev` for QSV, `libva-dev libva-drm` for VAAPI, `libx264-dev` for software fallback.** A bootstrap script in `host/scripts/install-build-deps.sh` is worth writing.
- **The unsafe FFI in each encoder impl is the single largest source of bugs** in this plan. Run `cargo miri test -p ix-codec` against the parts that don't actually need the SDK (probe, shared_config, trait_contract with NullEncoder) — not against the encoders themselves (Miri can't model the GPU).
- **`webrtc-rs` HEVC support** landed in 0.10 but is still flagged as "experimental" in the crate's README. If you hit issues, check the HEVC-specific tests in `webrtc/examples/play-from-disk-hevc/`.
- **The smoke test's "p99 < 30 ms" gate is intentionally generous.** Real Wi-Fi adds 3–8 ms; in-process loopback should easily hit single-digit ms. If smoke_loopback regresses to 25 ms, something is wrong even though it still passes.
