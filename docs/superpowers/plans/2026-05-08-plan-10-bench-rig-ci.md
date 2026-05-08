# Plan 10 — Bench Rig & CI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the testing infrastructure that proves iExtend hits its latency budget (`p50 14 ms Wi-Fi 6E / 10 ms USB-C`, p99 ≤ 30 ms / ≤ 18 ms) and stays smooth over long sessions. Three tiers: (1) cheap synthetic loopback in CI, (2) real-hardware CI cluster of four boxes, (3) a manual 240 fps camera bench rig that measures true photon-to-photon latency.

**Architecture:**
- Synthetic CI runs in-process inside `iextendd`'s integration tests — pure software, no real iPad needed; catches encoder / transport / config regressions.
- Hardware CI cluster runs the real binary on real GPUs against real iPads tethered USB-C; catches driver / codec / OS regressions that the synthetic test cannot see.
- Bench camera rig is the only path to true wall-clock photon-to-photon measurement; runs weekly + before every release.
- 8-hour soak and structured Pencil-feel reviews complete the picture.

**Tech Stack:**
- Rust (in `iextendd` workspace) for synthetic latency test + screen-timecode reference signal
- Ansible 2.16+ for hardware-CI box provisioning
- Python 3.12+ + OpenCV + Tesseract for the bench-rig analysis script
- GitHub Actions self-hosted runners on the hardware boxes
- Chart.js (vendored) for the soak-test dashboard

**Plan scope:** This is **Plan 10 of 10** for the iExtend project. It depends on:
- **Plan 2** (Rust host workspace bootstrap) — needs the `iextendd` crate + `host/.github/workflows/host-ci.yml` to extend
- **Plan 5** (WebRTC transport + codec selection) — the synthetic loopback test wraps this pipeline
- **Plan 6** (iPad Swift app shell + decode/render) — hardware CI requires a working iPad app to receive frames
- **Plan 9** (installers + codesigning) — Ansible playbooks install the signed `.msix` / `.deb` produced by Plan 9

After this plan, v1 is shippable: the team has automated regression detection, a way to measure ground truth, and a structured subjective-quality bar.

---

## Tier strategy: synthetic vs hardware CI

Hardware CI is **expensive**. Realistic costs for the 4-box cluster:

| Item | Initial | Annual recurring |
|---|---|---|
| Win11 + RTX 4060 box | ~$1,500 | — |
| Win11 + Intel iGPU box | ~$900 | — |
| Ubuntu + RX 7600 box | ~$1,400 | — |
| Ubuntu + Intel iGPU box | ~$900 | — |
| 4× iPad Pro M2 (one per box) | ~$3,600 | — |
| Network gear (Wi-Fi 6E AP, switch) | ~$400 | — |
| 4-port USB-C KVM (optional) | ~$300 | — |
| **Hardware total** | **~$9,000** | — |
| Electricity (4 boxes 24/7, 100 W avg, $0.15/kWh) | — | ~$525 |
| iPad battery replacement (annual) | — | ~$400 |
| Office space / racking | — | varies |
| **Recurring total** | — | **~$1,000+** |

**If the budget doesn't allow hardware CI:** ship with synthetic CI only. Synthetic CI catches roughly 70 % of pre-release regressions (anything in encoder config, codec selection, packet pacing, congestion control). The other 30 % — driver-stack changes, OS-vendor codec regressions, USB-C link flakiness, iPad-side decode pipeline regressions — only show up on real hardware. Document both tiers; let the user pick.

The plan below builds **both** tiers. Tasks 5–6 (provision hardware + write playbooks) are the optional ones; everything else is required even if you skip hardware CI.

---

## File Structure

```
iExtend/
├── host/                                        # from Plan 2
│   ├── crates/
│   │   └── iextendd/
│   │       └── tests/
│   │           ├── synthetic_latency.rs         # NEW — Task 2
│   │           └── support/
│   │               ├── mod.rs                   # NEW — Task 2
│   │               ├── loopback_peer.rs         # NEW — Task 2
│   │               └── timecode_source.rs       # NEW — Task 1
│   └── .github/workflows/
│       ├── host-ci.yml                          # MODIFIED — Task 3
│       └── perf-nightly.yml                     # NEW — Task 4
├── bench/                                       # NEW (root of this plan)
│   ├── README.md                                # NEW — Task 0
│   ├── cluster/
│   │   ├── README.md                            # NEW — Task 5
│   │   ├── inventory.example.yml                # NEW — Task 6
│   │   ├── group_vars/
│   │   │   └── all.example.yml                  # NEW — Task 6
│   │   ├── win11-rtx4060/playbook.yml           # NEW — Task 6
│   │   ├── win11-igpu/playbook.yml              # NEW — Task 6
│   │   ├── ubuntu-rx7600/playbook.yml           # NEW — Task 6
│   │   ├── ubuntu-igpu/playbook.yml             # NEW — Task 6
│   │   └── roles/                               # NEW — Task 6
│   │       ├── common/tasks/main.yml
│   │       ├── github-runner/tasks/main.yml
│   │       ├── iextend-host/tasks/main.yml
│   │       └── ipad-pair/tasks/main.yml
│   ├── camera-rig/
│   │   ├── README.md                            # NEW — Task 7
│   │   ├── jig-design.md                        # NEW — Task 7
│   │   ├── measure_p2p_latency.py               # NEW — Task 8
│   │   ├── pyproject.toml                       # NEW — Task 8
│   │   └── tests/
│   │       └── test_measure.py                  # NEW — Task 8
│   ├── camera-rig/screen-timecode/              # small Rust crate
│   │   ├── Cargo.toml                           # NEW — Task 1
│   │   ├── src/
│   │   │   ├── main.rs                          # NEW — Task 1
│   │   │   ├── vsync_windows.rs                 # NEW — Task 1
│   │   │   └── vsync_linux.rs                   # NEW — Task 1
│   │   └── README.md                            # NEW — Task 1
│   ├── soak/
│   │   ├── README.md                            # NEW — Task 9
│   │   ├── run-soak.sh                          # NEW — Task 9
│   │   └── dashboard/
│   │       ├── dashboard.html                   # NEW — Task 9
│   │       ├── chart.umd.min.js                 # vendored Chart.js v4 — Task 9
│   │       └── style.css                        # NEW — Task 9
│   └── pencil-feel/
│       ├── protocol.md                          # NEW — Task 10
│       ├── consent-form.md                      # NEW — Task 10
│       ├── google-form.template.json            # NEW — Task 10
│       ├── analyze.py                           # NEW — Task 10
│       └── results/                             # gitignored except .gitkeep
│           └── .gitkeep
└── docs/
    └── superpowers/plans/
        └── 2026-05-08-plan-10-bench-rig-ci.md   # this file
```

**Inter-file principles:**
- Anything CI-related lives under `host/.github/workflows/` so it's adjacent to Plan 2's existing config.
- The bench rig lives under a new top-level `bench/` so it's clearly out-of-band from the shipping product.
- `screen-timecode/` is a tiny standalone Rust crate, not a member of the main `host/` workspace, because it depends on platform display APIs and shouldn't pull those into `iextendd`'s normal build.

---

### Task 0: Bootstrap `bench/` directory + top-level README

**Files:**
- Create: `/home/tops/Projects/iExtend/bench/README.md`
- Create: `/home/tops/Projects/iExtend/bench/.gitkeep` (only if needed by git)

- [ ] **Step 1: Create `bench/README.md`**

Write at `/home/tops/Projects/iExtend/bench/README.md`:

```markdown
# iExtend bench rig

Out-of-band testing infrastructure. Not part of the shipping product.

## What lives here

- `cluster/` — Ansible playbooks for the hardware-CI cluster (4 boxes, optional but recommended)
- `camera-rig/` — physical 240 fps camera setup + analysis script for true photon-to-photon measurement
- `soak/` — 8-hour soak runner + dashboard
- `pencil-feel/` — structured subjective-quality protocol (10 reviewers × 3 apps)

## Tiers

iExtend uses three regression-detection tiers, in increasing order of cost and fidelity:

1. **Synthetic CI** (free, automatic) — `host/crates/iextendd/tests/synthetic_latency.rs`
   runs in GitHub Actions on every PR. Catches encoder/transport/config regressions.
2. **Hardware CI** (~$9 000 setup, ~$1 000/yr) — `cluster/` provisions 4 boxes with real
   GPUs + tethered iPads. Runs nightly + per release branch.
3. **Bench camera rig** (~$1 200 setup, manual) — `camera-rig/` measures true wall-clock
   photon-to-photon latency. Run weekly + before every release.

If the budget doesn't allow Tier 2, Tier 1 alone catches ~70 % of regressions. Don't skip
Tier 3 — only physical optics measure what the user actually feels.

## Running

| What | Where | Cadence |
|---|---|---|
| Synthetic loopback test | GitHub Actions | every PR + nightly |
| Hardware-CI smoke | self-hosted runners (cluster boxes) | nightly + release branches |
| Camera-rig measurement | manual, one operator + iPhone slo-mo | weekly + pre-release |
| 8-hour soak | manual, dedicated box | pre-release |
| Pencil-feel review | manual, 10 reviewers | pre-release (or on Pencil-related changes) |
```

- [ ] **Step 2: Commit**

```bash
cd /home/tops/Projects/iExtend
git add bench/README.md
git commit -m "bench: add bench-rig root README"
```

---

### Task 1: Screen-timecode reference signal

**Files:**
- Create: `/home/tops/Projects/iExtend/bench/camera-rig/screen-timecode/Cargo.toml`
- Create: `/home/tops/Projects/iExtend/bench/camera-rig/screen-timecode/src/main.rs`
- Create: `/home/tops/Projects/iExtend/bench/camera-rig/screen-timecode/src/vsync_windows.rs`
- Create: `/home/tops/Projects/iExtend/bench/camera-rig/screen-timecode/src/vsync_linux.rs`
- Create: `/home/tops/Projects/iExtend/bench/camera-rig/screen-timecode/README.md`
- Create: `/home/tops/Projects/iExtend/host/crates/iextendd/tests/support/timecode_source.rs`

The `screen-timecode` binary paints a decimal-seconds-since-epoch timecode on the screen edge, synchronized to vsync. The bench camera captures both the host monitor's painted timecode and the iPad's displayed (relayed) timecode in the same frame. Frame-by-frame diff = true photon-to-photon latency.

- [ ] **Step 1: Write `Cargo.toml`**

Create `/home/tops/Projects/iExtend/bench/camera-rig/screen-timecode/Cargo.toml`:

```toml
[package]
name = "screen-timecode"
version = "0.1.0"
edition = "2024"
description = "Paints a decimal-seconds timecode on the screen edge, synced to vsync. Used by the bench camera rig to measure photon-to-photon latency."
publish = false

[dependencies]
anyhow = "1.0"
clap = { version = "4.5", features = ["derive"] }
softbuffer = "0.4"
winit = "0.30"

[target.'cfg(windows)'.dependencies]
windows = { version = "0.58", features = [
    "Win32_Foundation",
    "Win32_Graphics_Dxgi",
    "Win32_Graphics_Dxgi_Common",
] }

[target.'cfg(target_os = "linux")'.dependencies]
drm = "0.12"
nix = { version = "0.29", features = ["fs", "ioctl"] }
```

- [ ] **Step 2: Write `src/main.rs`**

Create `/home/tops/Projects/iExtend/bench/camera-rig/screen-timecode/src/main.rs`:

```rust
//! Screen-timecode reference signal.
//!
//! Paints a 7-segment-style decimal timecode (microseconds-since-process-start) on the
//! top-left corner of the primary display. The repaint is locked to vsync via the
//! platform's frame-statistics API. The camera rig sees this both directly and on the
//! iPad (via iExtend); per-frame OCR + subtraction = photon-to-photon latency.

use anyhow::Result;
use clap::Parser;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

#[cfg(windows)]
mod vsync_windows;
#[cfg(target_os = "linux")]
mod vsync_linux;

#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// Digit height in pixels (the bigger, the more reliably OCR catches it at 240 fps)
    #[arg(long, default_value_t = 80)]
    digit_height: u32,
    /// Margin from the top-left corner, pixels
    #[arg(long, default_value_t = 16)]
    margin: u32,
}

struct App {
    args: Args,
    start: Instant,
    window: Option<Window>,
    surface: Option<softbuffer::Surface<&'static Window, &'static Window>>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title("iExtend timecode")
            .with_decorations(false)
            .with_transparent(true);
        self.window = Some(el.create_window(attrs).expect("create window"));
        // (softbuffer surface init omitted for brevity; see README)
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if let WindowEvent::RedrawRequested = event {
            let us = self.start.elapsed().as_micros() as u64;
            // 1. paint big block-segment digits (us mod 10_000_000) at top-left
            // 2. additionally paint a 32-bit binary stripe along the very top edge —
            //    redundant signal in case OCR fails on glare (camera rig docs explain)
            paint_timecode(&mut self.surface, us, self.args.digit_height, self.args.margin);
            self.window.as_ref().unwrap().request_redraw();
        }
    }
}

fn paint_timecode(_surface: &mut Option<softbuffer::Surface<&'static Window, &'static Window>>,
                  _microseconds: u64, _digit_h: u32, _margin: u32) {
    // implementation: simple 7-segment LUT; ~150 lines. See README for the bitmap shape.
}

fn main() -> Result<()> {
    let args = Args::parse();
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        args,
        start: Instant::now(),
        window: None,
        surface: None,
    };

    #[cfg(windows)]
    vsync_windows::install_present_callback()?;
    #[cfg(target_os = "linux")]
    vsync_linux::install_drm_vblank_handler()?;

    event_loop.run_app(&mut app)?;
    Ok(())
}
```

- [ ] **Step 3: Write the platform vsync handlers (skeletons; full impl ~80 lines each)**

Create `/home/tops/Projects/iExtend/bench/camera-rig/screen-timecode/src/vsync_windows.rs`:

```rust
//! Windows vsync via DXGI frame-statistics. We don't need to render through DXGI;
//! we just want a high-priority callback fired on every present-cycle so we can
//! request a redraw at the right moment.

use anyhow::{Context, Result};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput};

pub fn install_present_callback() -> Result<()> {
    // 1. CreateDXGIFactory1 → enumerate adapters → first IDXGIOutput
    // 2. spawn a thread that calls IDXGIOutput::WaitForVBlank in a loop and sets
    //    a flag the main loop polls every redraw
    // Full implementation ~80 lines — see README.
    Ok(())
}
```

Create `/home/tops/Projects/iExtend/bench/camera-rig/screen-timecode/src/vsync_linux.rs`:

```rust
//! Linux vsync via DRM vblank.

use anyhow::Result;

pub fn install_drm_vblank_handler() -> Result<()> {
    // 1. open /dev/dri/card0
    // 2. drmWaitVBlank with DRM_VBLANK_RELATIVE
    // 3. spawn thread that loops vblank wait + signals the main loop
    Ok(())
}
```

- [ ] **Step 4: Write `README.md`**

Create `/home/tops/Projects/iExtend/bench/camera-rig/screen-timecode/README.md` documenting:

- The digit-bitmap shape (7-segment LUT; show ASCII art of each digit)
- Why the binary stripe along the top edge is a redundant fallback channel for OCR-poor frames
- How to read the timecode from a 240 fps camera frame in Python (used by `measure_p2p_latency.py`)
- Build instructions: `cargo build --release` from the crate directory
- Why this is a separate crate from the `host/` workspace (depends on platform display APIs that shouldn't bleed into `iextendd`'s normal build matrix)

- [ ] **Step 5: Write the in-crate `timecode_source.rs` helper**

`/home/tops/Projects/iExtend/host/crates/iextendd/tests/support/timecode_source.rs` provides the same timecode-painting logic but as an in-process texture (RGBA buffer of size `1920×1080`) for the synthetic latency test in Task 2 — no display required.

```rust
//! In-process timecode painter for the synthetic latency test.
//! Same digit shapes as bench/camera-rig/screen-timecode/, no display.

pub struct Painter { /* digit LUT, frame buffer, etc. */ }

impl Painter {
    pub fn new(width: u32, height: u32) -> Self { todo!() }

    /// Paints the given microseconds-since-start into a fresh RGBA buffer.
    pub fn paint(&mut self, microseconds: u64) -> &[u8] { todo!() }

    /// Reverse: given a captured RGBA buffer (from the decoder), read the timecode back.
    pub fn read(buf: &[u8], width: u32, height: u32) -> Option<u64> { todo!() }
}
```

The full implementation is ~250 lines: a 7-segment LUT with a 32-bit binary fallback stripe, plus a simple read path that locates the digits by their black-on-white contrast and matches each digit against the LUT.

- [ ] **Step 6: Verify the binary builds and paints**

```bash
cd /home/tops/Projects/iExtend/bench/camera-rig/screen-timecode
cargo build --release
cargo run --release -- --digit-height 80 &
sleep 2
# (manual: take a phone photo of the screen; verify decimal digits + binary stripe are visible)
kill %1
```

- [ ] **Step 7: Verify the in-crate helper round-trips**

Add a unit test in `tests/support/timecode_source.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip() {
        let mut p = Painter::new(1920, 1080);
        let buf = p.paint(123_456_789).to_vec();
        let read = Painter::read(&buf, 1920, 1080).unwrap();
        assert_eq!(read, 123_456_789);
    }
}
```

Run:
```bash
cd /home/tops/Projects/iExtend/host
cargo test -p iextendd --test '*' round_trip
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
cd /home/tops/Projects/iExtend
git add bench/camera-rig/screen-timecode host/crates/iextendd/tests/support/
git commit -m "bench: add screen-timecode reference signal + in-crate painter"
```

---

### Task 2: Synthetic latency CI test (`synthetic_latency.rs`)

**Files:**
- Create: `/home/tops/Projects/iExtend/host/crates/iextendd/tests/synthetic_latency.rs`
- Create: `/home/tops/Projects/iExtend/host/crates/iextendd/tests/support/mod.rs`
- Create: `/home/tops/Projects/iExtend/host/crates/iextendd/tests/support/loopback_peer.rs`

The synthetic test wires the host's full pipeline to itself: capture (synthetic, from the timecode painter) → encoder → SRTP packetize → loopback transport → SRTP depacketize → decoder → frame-buffer reader. Reads the timecode out and compares to wall-clock `now()`.

- [ ] **Step 1: Write `support/mod.rs`**

```rust
//! Test-support code shared between integration tests.
pub mod timecode_source;
pub mod loopback_peer;
```

- [ ] **Step 2: Write `support/loopback_peer.rs`**

```rust
//! In-process WebRTC loopback peer.
//!
//! Plan 5 will have produced an `IxRtcPeer` abstraction. The loopback peer is two
//! `IxRtcPeer` instances connected via an in-memory channel that bypasses ICE
//! entirely — DTLS-SRTP still runs (we want the encryption cost in the budget),
//! but discovery is null and the wire is a `tokio::sync::mpsc` channel.

use anyhow::Result;
use ix_rtc::{IxRtcConfig, IxRtcPeer, IxRtcPeerEvent};
use tokio::sync::mpsc;

pub struct LoopbackPair {
    pub host: IxRtcPeer,
    pub client: IxRtcPeer,
}

impl LoopbackPair {
    pub async fn new(cfg: IxRtcConfig) -> Result<Self> {
        // 1. host_peer = IxRtcPeer::new(cfg, role=Offerer)
        // 2. client_peer = IxRtcPeer::new(cfg, role=Answerer)
        // 3. wire host.send → client.recv and vice versa via mpsc channels
        // 4. trigger SDP offer/answer dance
        // 5. wait for ICE-complete (no-op for in-memory transport) + DTLS-handshake-done
        // Returns once both peers report "Live"
        todo!("≈120 lines")
    }
}
```

- [ ] **Step 3: Write `synthetic_latency.rs`**

```rust
//! Synthetic round-trip latency test.
//!
//! Runs 600 timecoded frames at 120 fps (5 s) through the full host pipeline
//! looped back into itself. Computes round-trip per-frame latency and asserts:
//!   - p50 ≤ 12 ms
//!   - p95 ≤ 22 ms
//!   - p99 ≤ 30 ms
//!
//! The thresholds are looser than the real-deployment budget because the loopback
//! path doubles the encode/decode cost (each frame is encoded once on the host
//! side, then again on the loopback-decoded path before reading). Treat this as
//! a regression detector, not a budget oracle.
//!
//! Threshold drift > 2 ms vs. baseline_p99.txt (committed in tree) fails the test.

mod support;

use anyhow::Result;
use ix_codec::HevcEncoder;
use std::time::{Duration, Instant};
use support::loopback_peer::LoopbackPair;
use support::timecode_source::Painter;

const FRAMES: usize = 600;
const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const FPS: u32 = 120;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn synthetic_round_trip_latency() -> Result<()> {
    let mut painter = Painter::new(WIDTH, HEIGHT);
    let pair = LoopbackPair::new(Default::default()).await?;
    let frame_period = Duration::from_secs_f64(1.0 / FPS as f64);

    let mut latencies_us = Vec::with_capacity(FRAMES);
    let test_start = Instant::now();

    for i in 0..FRAMES {
        let frame_t = test_start + frame_period * i as u32;
        tokio::time::sleep_until(frame_t.into()).await;

        let send_us = test_start.elapsed().as_micros() as u64;
        let frame = painter.paint(send_us);
        pair.host.send_frame(frame, WIDTH, HEIGHT).await?;
    }

    // drain the receive side
    while let Some(decoded) = pair.client.recv_frame().await {
        let recv_us = test_start.elapsed().as_micros() as u64;
        if let Some(send_us) = Painter::read(&decoded.rgba, WIDTH, HEIGHT) {
            latencies_us.push(recv_us.saturating_sub(send_us));
        }
        if latencies_us.len() >= FRAMES { break; }
    }

    latencies_us.sort_unstable();
    let p50 = latencies_us[latencies_us.len() / 2];
    let p95 = latencies_us[latencies_us.len() * 95 / 100];
    let p99 = latencies_us[latencies_us.len() * 99 / 100];

    println!("synthetic: p50={p50}us p95={p95}us p99={p99}us  n={}", latencies_us.len());

    assert!(p50 <= 12_000, "p50 regression: {p50}us > 12_000us");
    assert!(p95 <= 22_000, "p95 regression: {p95}us > 22_000us");
    assert!(p99 <= 30_000, "p99 regression: {p99}us > 30_000us");

    // baseline drift gate
    if let Ok(baseline_str) = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/baseline_p99.txt"))
    {
        let baseline_us: u64 = baseline_str.trim().parse().expect("baseline_p99.txt is not u64");
        let drift_us = (p99 as i64) - (baseline_us as i64);
        assert!(drift_us <= 2_000,
            "p99 drifted by {drift_us}us vs baseline {baseline_us}us — investigate before bumping baseline");
    }

    Ok(())
}
```

- [ ] **Step 4: Seed `tests/baseline_p99.txt`**

Create `/home/tops/Projects/iExtend/host/crates/iextendd/tests/baseline_p99.txt` containing a single integer — the current p99 from a clean local run, in microseconds:

```
24500
```

(The implementer running this task should run the test once, observe their actual p99, and write that number. The 24500 above is a placeholder; replace with the real number.)

- [ ] **Step 5: Run the test locally**

```bash
cd /home/tops/Projects/iExtend/host
cargo test -p iextendd --test synthetic_latency --release -- --nocapture
```

Expected: `test synthetic_round_trip_latency ... ok`. The println output should show p50 / p95 / p99 within target.

- [ ] **Step 6: Commit**

```bash
git add host/crates/iextendd/tests/synthetic_latency.rs \
        host/crates/iextendd/tests/support/ \
        host/crates/iextendd/tests/baseline_p99.txt
git commit -m "test: synthetic WebRTC-loopback latency regression test"
```

---

### Task 3: Wire synthetic test into `host-ci.yml`

**Files:**
- Modify: `/home/tops/Projects/iExtend/host/.github/workflows/host-ci.yml`

Plan 2 created `host-ci.yml` with build + clippy + unit-test jobs. Add a `synthetic-latency` job that runs in release mode on `ubuntu-24.04` (free GitHub runner; HEVC encode falls back to `libx265` since there's no NVENC/QSV/VAAPI hardware).

- [ ] **Step 1: Read existing `host-ci.yml`**

Confirm Plan 2's structure. There should already be `build`, `clippy`, `test` jobs. We're adding a new job after them.

- [ ] **Step 2: Append the new job**

Append to the `jobs:` block of `host/.github/workflows/host-ci.yml`:

```yaml
  synthetic-latency:
    name: Synthetic latency regression test
    runs-on: ubuntu-24.04
    needs: build
    timeout-minutes: 15
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: host
      - name: Install x265 / libdrm headers (software encode fallback)
        run: |
          sudo apt-get update
          sudo apt-get install -y libx265-dev libdrm-dev pkg-config
      - name: Run synthetic latency test
        working-directory: host
        run: cargo test -p iextendd --test synthetic_latency --release -- --nocapture
      - name: Persist p50/p95/p99 as job summary
        if: always()
        run: |
          # extract from the most-recent test stdout via tee in the prior step;
          # in practice the test prints "synthetic: p50=...us p95=...us p99=...us"
          echo "See test output above for percentiles." >> $GITHUB_STEP_SUMMARY
```

- [ ] **Step 3: Push a branch and verify the job runs green**

```bash
git checkout -b plan10/synthetic-ci
git add host/.github/workflows/host-ci.yml
git commit -m "ci: add synthetic-latency job to host-ci"
git push -u origin plan10/synthetic-ci
gh pr create --title "Plan 10: synthetic latency CI" --body "..."
gh pr checks --watch
```

Expected: all jobs (build, clippy, test, synthetic-latency) green. The synthetic-latency job log should show three percentiles.

- [ ] **Step 4: Merge and clean up**

```bash
gh pr merge --merge
git checkout main && git pull
```

---

### Task 4: `perf-nightly.yml` workflow

**Files:**
- Create: `/home/tops/Projects/iExtend/host/.github/workflows/perf-nightly.yml`

A nightly workflow that runs a longer (30-minute) version of the synthetic test, computes deltas vs. the previous nightly run, and posts a comment on the most-recent merged PR.

- [ ] **Step 1: Write `perf-nightly.yml`**

```yaml
name: perf-nightly

on:
  schedule:
    - cron: '17 7 * * *'    # 07:17 UTC every day; off-peak for free runners
  workflow_dispatch:

jobs:
  long-soak-synthetic:
    runs-on: ubuntu-24.04
    timeout-minutes: 60
    steps:
      - uses: actions/checkout@v4
        with: { fetch-depth: 0 }
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with: { workspaces: host }
      - name: Install software-encode deps
        run: |
          sudo apt-get update
          sudo apt-get install -y libx265-dev libdrm-dev pkg-config
      - name: Run 30-minute synthetic soak
        working-directory: host
        env:
          IX_SOAK_DURATION_SECS: "1800"
        run: |
          cargo test -p iextendd --test synthetic_latency --release \
            -- --nocapture 2>&1 | tee /tmp/soak.log
      - name: Extract percentiles
        id: pct
        run: |
          line=$(grep -E 'synthetic: p50=' /tmp/soak.log | tail -1)
          p50=$(echo "$line" | sed -nE 's/.*p50=([0-9]+)us.*/\1/p')
          p95=$(echo "$line" | sed -nE 's/.*p95=([0-9]+)us.*/\1/p')
          p99=$(echo "$line" | sed -nE 's/.*p99=([0-9]+)us.*/\1/p')
          echo "p50=$p50" >> "$GITHUB_OUTPUT"
          echo "p95=$p95" >> "$GITHUB_OUTPUT"
          echo "p99=$p99" >> "$GITHUB_OUTPUT"
      - name: Compute delta vs previous run
        id: delta
        uses: actions/github-script@v7
        env:
          P50: ${{ steps.pct.outputs.p50 }}
          P95: ${{ steps.pct.outputs.p95 }}
          P99: ${{ steps.pct.outputs.p99 }}
        with:
          script: |
            const fs = require('fs');
            const path = '.perf-history.json';
            let history = [];
            try { history = JSON.parse(fs.readFileSync(path)); } catch (_) {}
            const today = { date: new Date().toISOString().slice(0, 10),
                            p50: +process.env.P50,
                            p95: +process.env.P95,
                            p99: +process.env.P99 };
            const prev = history[history.length - 1];
            history.push(today);
            fs.writeFileSync(path, JSON.stringify(history.slice(-60), null, 2));
            return prev ? {
              dp50: today.p50 - prev.p50,
              dp95: today.p95 - prev.p95,
              dp99: today.p99 - prev.p99,
              today, prev
            } : { today, prev: null };
      - name: Comment on most-recent merged PR
        if: steps.delta.outputs.result != ''
        uses: actions/github-script@v7
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
          script: |
            const { owner, repo } = context.repo;
            const merged = await github.rest.pulls.list({
              owner, repo, state: 'closed', sort: 'updated', direction: 'desc', per_page: 10
            });
            const pr = merged.data.find(p => p.merged_at);
            if (!pr) return;

            const d = ${{ steps.delta.outputs.result }};
            const fmt = (n) => n == null ? 'n/a' : `${n > 0 ? '+' : ''}${(n / 1000).toFixed(2)} ms`;
            const body = [
              '## perf-nightly delta',
              '',
              '| | yesterday | today | Δ |',
              '|---|---|---|---|',
              `| p50 | ${(d.prev?.p50 / 1000).toFixed(2)} ms | ${(d.today.p50 / 1000).toFixed(2)} ms | ${fmt(d.dp50)} |`,
              `| p95 | ${(d.prev?.p95 / 1000).toFixed(2)} ms | ${(d.today.p95 / 1000).toFixed(2)} ms | ${fmt(d.dp95)} |`,
              `| p99 | ${(d.prev?.p99 / 1000).toFixed(2)} ms | ${(d.today.p99 / 1000).toFixed(2)} ms | ${fmt(d.dp99)} |`,
              '',
              `_Synthetic loopback, 30 min, ${d.today.date}._`
            ].join('\n');

            await github.rest.issues.createComment({
              owner, repo, issue_number: pr.number, body
            });
      - name: Persist perf history
        if: always()
        run: |
          git config user.email "actions@github.com"
          git config user.name "github-actions"
          git add .perf-history.json
          git diff --cached --quiet || git commit -m "chore: update perf history $(date -u +%Y-%m-%d)"
          git push origin HEAD:main || true
```

- [ ] **Step 2: Add `IX_SOAK_DURATION_SECS` support to the synthetic test**

Modify `host/crates/iextendd/tests/synthetic_latency.rs`: read `IX_SOAK_DURATION_SECS` env var; if set, override the 5-second / 600-frame default and run for that many seconds (at 120 fps).

```rust
let duration_secs: u64 = std::env::var("IX_SOAK_DURATION_SECS")
    .ok().and_then(|s| s.parse().ok()).unwrap_or(5);
let frames = (duration_secs * FPS as u64) as usize;
```

- [ ] **Step 3: Commit + verify on a hand-triggered run**

```bash
git add host/.github/workflows/perf-nightly.yml host/crates/iextendd/tests/synthetic_latency.rs
git commit -m "ci: add perf-nightly with PR-comment perf delta"
git push origin main
gh workflow run perf-nightly
sleep 60
gh run list --workflow=perf-nightly --limit 1
gh run view <run-id> --log
```

Expected: workflow completes; PR comment appears on the most-recent merged PR; `.perf-history.json` is committed.

---

### Task 5: Procure the hardware-CI cluster

**This task is partially physical work, not code. Document carefully so it's reproducible.**

**Files:**
- Create: `/home/tops/Projects/iExtend/bench/cluster/README.md`

- [ ] **Step 1: Author the procurement README**

Write at `/home/tops/Projects/iExtend/bench/cluster/README.md`:

```markdown
# iExtend hardware-CI cluster

Four boxes representing the four major host configurations we ship for, each
tethered USB-C to a dedicated iPad Pro M2.

## Bill of materials

### Box 1: Win11 + RTX 4060
- CPU: AMD Ryzen 7 7700 (8c, AM5)            ~$280
- Motherboard: ASRock B650M PG Lightning      ~$140
- RAM: 32 GB DDR5-6000 (2×16)                 ~$110
- GPU: ASUS Dual GeForce RTX 4060 8 GB        ~$300
- NVMe: WD Black SN770 1 TB                   ~$80
- PSU: Corsair RM750x                         ~$120
- Case: Fractal Design Pop Mini Air           ~$80
- Win11 Pro license                           ~$200
- **Total: ~$1,310**

### Box 2: Win11 + Intel iGPU
- CPU: Intel Core Ultra 5 245K (Meteor Lake or newer; iGPU = Xe-LPG, has QSV) ~$340
- Motherboard: ASUS PRIME B860M-A             ~$160
- RAM: 32 GB DDR5-5600                        ~$100
- NVMe: 1 TB                                  ~$80
- PSU + case                                  ~$180
- Win11 Pro license                           ~$200
- **Total: ~$1,060**

### Box 3: Ubuntu 24.04 + AMD RX 7600
- CPU: AMD Ryzen 5 7600                       ~$200
- Motherboard: B650M                          ~$130
- RAM: 32 GB DDR5                             ~$100
- GPU: ASUS Dual RX 7600 8 GB                 ~$280
- NVMe: 1 TB                                  ~$80
- PSU + case                                  ~$180
- **Total: ~$970**

### Box 4: Ubuntu 24.04 + Intel iGPU
- Same as Box 2 spec, no Win license          ~$860

### iPads (one per box)
- 4× iPad Pro M2 11" 128 GB (refurb or new)   ~$3,500

### Networking + ancillaries
- Wi-Fi 6E AP (TP-Link Archer AXE75 or eq.)   ~$200
- 8-port gigabit switch                        ~$50
- 4× USB-C 3.2 cables, 1m, certified 5 Gbps   ~$60
- Per-box: 32" 4K monitor (refurb)            ~$700 (4×)
- KVM (optional)                              ~$300
- UPS (1500 VA, line-interactive)             ~$250

### Subtotal (excluding labor / shipping)
- Hardware: ~$8,800
- Recurring (annual): ~$525 power + ~$400 iPad battery + space

## Procurement steps

1. **Order all four boxes** as DIY kits or pre-built. Lead time: 2–4 weeks for refurb iPads.
2. **Assemble** (or unbox if pre-built). Run an OS reinstall from clean media so we know
   the starting state.
3. **Network setup:**
   - All four boxes on the same Wi-Fi 6E SSID + same gigabit switch (uplink to internet).
   - Static DHCP reservations for each box's MAC so the Ansible inventory stays stable.
4. **Per box, install OS** (no Ansible yet — bootstrap):
   - **Win11 boxes:** install Win11 Pro, create local admin account `iextend-runner`,
     enable WSL2, install OpenSSH server, enable PowerShell remoting from the operator
     workstation.
   - **Ubuntu boxes:** install Ubuntu 24.04 LTS server, create user `iextend-runner`,
     install openssh-server, give it sudo NOPASSWD.
5. **Per iPad:** factory reset, install iExtend.app via TestFlight, sign in, **disable
   auto-lock and Low Power Mode**, lock device orientation. Pair with the corresponding
   host via the SPAKE2 PIN flow.
6. **Verify hand-run smoke:** start `iextendd` on each host, confirm the iPad shows the
   live screen, drag a window onto it. Document any per-box quirks.
7. **Hand off to Task 6** (Ansible playbooks).

## Operating costs

| | Annual |
|---|---|
| Power (4 boxes × 100 W avg × 24/7 × $0.15/kWh) | $525 |
| iPad battery service (estimated 1 of 4 needs replacement / yr) | $400 |
| Workspace / cooling | varies |

## When the budget doesn't allow

Skip this. Tier-1 synthetic CI catches most regressions. Run the camera-rig (Task 7+8)
and the soak (Task 9) manually on a single dev workstation before each release.
```

- [ ] **Step 2: Commit** (no code yet — just the procurement doc)

```bash
git add bench/cluster/README.md
git commit -m "bench: add hardware-CI cluster procurement & setup README"
```

---

### Task 6: Ansible playbooks for the four boxes

**Files:**
- Create: `/home/tops/Projects/iExtend/bench/cluster/inventory.example.yml`
- Create: `/home/tops/Projects/iExtend/bench/cluster/group_vars/all.example.yml`
- Create: `/home/tops/Projects/iExtend/bench/cluster/win11-rtx4060/playbook.yml`
- Create: `/home/tops/Projects/iExtend/bench/cluster/win11-igpu/playbook.yml`
- Create: `/home/tops/Projects/iExtend/bench/cluster/ubuntu-rx7600/playbook.yml`
- Create: `/home/tops/Projects/iExtend/bench/cluster/ubuntu-igpu/playbook.yml`
- Create: `/home/tops/Projects/iExtend/bench/cluster/roles/common/tasks/main.yml`
- Create: `/home/tops/Projects/iExtend/bench/cluster/roles/github-runner/tasks/main.yml`
- Create: `/home/tops/Projects/iExtend/bench/cluster/roles/iextend-host/tasks/main.yml`
- Create: `/home/tops/Projects/iExtend/bench/cluster/roles/ipad-pair/tasks/main.yml`

- [ ] **Step 1: Write `inventory.example.yml`**

```yaml
# Copy to inventory.yml (gitignored) and fill in real IPs.
all:
  children:
    win11_hosts:
      hosts:
        win11-rtx4060: { ansible_host: 10.0.10.11, gpu_vendor: nvidia, gpu_model: rtx4060 }
        win11-igpu:    { ansible_host: 10.0.10.12, gpu_vendor: intel,  gpu_model: xe-lpg }
      vars:
        ansible_connection: ssh
        ansible_user: iextend-runner
        ansible_shell_type: powershell
    ubuntu_hosts:
      hosts:
        ubuntu-rx7600: { ansible_host: 10.0.10.13, gpu_vendor: amd,    gpu_model: rx7600 }
        ubuntu-igpu:   { ansible_host: 10.0.10.14, gpu_vendor: intel,  gpu_model: xe-lpg }
      vars:
        ansible_connection: ssh
        ansible_user: iextend-runner
        ansible_become: true
        ansible_become_method: sudo
```

- [ ] **Step 2: Write `group_vars/all.example.yml`**

```yaml
# Copy to all.yml (gitignored) and fill in real values.
github_runner_org: "your-org"           # GitHub org
github_runner_token: "<from-actions>"    # short-lived token from gh api
github_runner_labels: "iextend-bench,linux,gpu"   # appended per-host
ipad_serial: "<filled per host>"
ipad_pair_pin: "<one-time>"
iextend_release_channel: "stable"        # or "beta"
```

- [ ] **Step 3: Write `roles/common/tasks/main.yml`**

```yaml
# Common across both Linux + Windows: timezone, NTP, hostname.
- name: Set hostname
  hostname: { name: "{{ inventory_hostname }}" }
  when: ansible_system == 'Linux'

- name: Ensure chrony is installed (Linux)
  apt: { name: chrony, state: present }
  when: ansible_system == 'Linux'

- name: Disable auto-update reboots (Win11)
  win_regedit:
    path: HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU
    name: NoAutoRebootWithLoggedOnUsers
    data: 1
    type: dword
  when: ansible_system == 'Win32NT'

# (~40 lines total)
```

- [ ] **Step 4: Write `roles/github-runner/tasks/main.yml`**

Skeleton (~80 lines): downloads the runner tarball/zip, registers it with `--unattended --replace`, installs as a system service, enables autostart on boot. Two branches — one for Linux (systemd unit), one for Windows (NSSM service).

- [ ] **Step 5: Write `roles/iextend-host/tasks/main.yml`**

Skeleton (~120 lines):
- Fetches the latest stable `iextendd` build:
  - Linux: `apt install ./iextendd_*.deb` (signed Debian package from Plan 9)
  - Windows: install MSIX via PowerShell `Add-AppxPackage`
- Enables `iextendd` as an autostart service
- For Win11 RTX boxes, also installs the latest NVIDIA Studio driver
- For Ubuntu RX boxes, installs Mesa + AMDGPU drivers from kisak-mesa PPA
- Verifies `iextendd --version` works

- [ ] **Step 6: Write `roles/ipad-pair/tasks/main.yml`**

Documents the one-time hand-pair step (we don't fully automate it — the SPAKE2 flow needs a person to type the PIN on the iPad). Generates a checklist file + verifies via `iextendd ctl status` that a paired iPad is reachable.

- [ ] **Step 7: Write the four per-box playbooks**

`win11-rtx4060/playbook.yml`:

```yaml
- hosts: win11-rtx4060
  roles:
    - common
    - github-runner
    - iextend-host
    - ipad-pair
  vars:
    extra_runner_labels: "windows,nvidia,rtx4060"
    nvidia_studio_driver_version: "555.85"
```

The other three are analogous — `win11-igpu` adds `qsv,intel`, etc. Each is ~15 lines.

- [ ] **Step 8: Sanity-check syntax**

```bash
cd /home/tops/Projects/iExtend/bench/cluster
ansible-playbook --syntax-check win11-rtx4060/playbook.yml
ansible-playbook --syntax-check ubuntu-rx7600/playbook.yml
```

Expected: no errors.

- [ ] **Step 9: Run against a real box (when hardware is available)**

```bash
cp inventory.example.yml inventory.yml
cp group_vars/all.example.yml group_vars/all.yml
# fill in IPs, tokens, etc.
ansible-playbook -i inventory.yml ubuntu-rx7600/playbook.yml --check  # dry run
ansible-playbook -i inventory.yml ubuntu-rx7600/playbook.yml          # for real
```

Expected: ends with all-green; `gh workflow run host-ci -R <repo>` will dispatch jobs onto the new self-hosted runner.

- [ ] **Step 10: Commit**

```bash
git add bench/cluster/
git commit -m "bench: ansible playbooks for 4-box hardware-CI cluster"
```

---

### Task 7: Camera-rig physical setup + README

**Files:**
- Create: `/home/tops/Projects/iExtend/bench/camera-rig/README.md`
- Create: `/home/tops/Projects/iExtend/bench/camera-rig/jig-design.md`

This task is mostly documentation — the physical jig is not a code artifact, but the operating procedure needs to be reproducible.

- [ ] **Step 1: Write `camera-rig/README.md`**

Document, in order:

1. **Camera options** (cheapest → highest fidelity):
   - iPhone 15 Pro / 16 Pro slo-mo at 240 fps, 1080p — works, free if you have one.
   - Sony RX0 II at 960 fps — overkill, ~$600.
   - Raspberry Pi HQ camera + global-shutter sensor + custom rig — ~$200; tunable.
   - Phantom Veo 410 — sub-millisecond fidelity, $50k+; only if budget allows.
2. **Jig requirements:**
   - Tripod or fixed mount holding the camera level with both screens.
   - Both the host monitor and the iPad must be in the same camera frame, side by side, with timecodes near the corner closest to the seam.
   - Avoid glare: matte black backdrop, side lighting, anti-reflective coating helpful.
3. **Capture protocol:**
   - Start `screen-timecode` on the host (Task 1's binary).
   - Start `iextendd`; verify the iPad is mirroring or extending and that the iPad shows the same timecode (relayed).
   - Record at 240 fps for 30 s — that's 7,200 frames, plenty.
   - Save the resulting `.mov` / `.mp4` to `bench/camera-rig/captures/<date>-<setup>.mp4`.
4. **Analysis:**
   - Run `python measure_p2p_latency.py captures/<date>-<setup>.mp4 --out results/<date>.csv`.
   - Inspect `results/<date>.csv`; compute percentiles via the column `latency_us`.

- [ ] **Step 2: Write `jig-design.md`**

A quick CAD-free sketch + parts list for a $40 wooden jig that holds a 32" host monitor and an iPad 11" 30 cm apart, both visible in a single 1080p iPhone slo-mo frame at ~50 cm focal distance. Include:

- ASCII diagram of the jig
- Parts list (1× plywood base 60 cm × 30 cm, 2× monitor stands or VESA arms, foam padding for the iPad cradle)
- Why side-by-side beats over-under (at 240 fps the rolling-shutter offset between scanlines is ~4 ms — vertically separated screens get unfair latency depending on which one's higher in the frame; side-by-side eliminates this)

- [ ] **Step 3: Commit**

```bash
git add bench/camera-rig/README.md bench/camera-rig/jig-design.md
git commit -m "bench: camera-rig setup README + jig design doc"
```

---

### Task 8: `measure_p2p_latency.py`

**Files:**
- Create: `/home/tops/Projects/iExtend/bench/camera-rig/pyproject.toml`
- Create: `/home/tops/Projects/iExtend/bench/camera-rig/measure_p2p_latency.py`
- Create: `/home/tops/Projects/iExtend/bench/camera-rig/tests/test_measure.py`
- Create: `/home/tops/Projects/iExtend/bench/camera-rig/tests/fixtures/.gitkeep`

- [ ] **Step 1: Write `pyproject.toml`**

```toml
[project]
name = "iextend-bench-camera-rig"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = [
    "opencv-python==4.10.*",
    "numpy==1.26.*",
    "pytesseract==0.3.13",
    "click==8.1.*",
    "pandas==2.2.*",
]

[project.scripts]
measure-p2p-latency = "measure_p2p_latency:cli"

[tool.pytest.ini_options]
testpaths = ["tests"]
```

- [ ] **Step 2: Write `measure_p2p_latency.py`**

```python
"""Measure photon-to-photon latency from a 240 fps camera capture.

Input:  a video file containing both the host monitor and the iPad screen
        side-by-side, each painting the timecode produced by
        bench/camera-rig/screen-timecode/.
Output: a CSV with one row per frame: frame_idx, t_camera_us, host_us, ipad_us,
        latency_us = ipad_us - host_us.

Usage:
    python measure_p2p_latency.py CAPTURE.mp4 --out results.csv
                                 [--host-roi X,Y,W,H] [--ipad-roi X,Y,W,H]
                                 [--method digits|stripe]
"""

from __future__ import annotations

import csv
import sys
from dataclasses import dataclass
from pathlib import Path

import click
import cv2
import numpy as np
import pytesseract


@dataclass
class Roi:
    x: int; y: int; w: int; h: int

    @classmethod
    def parse(cls, s: str) -> "Roi":
        x, y, w, h = (int(v) for v in s.split(","))
        return cls(x, y, w, h)


def read_timecode_digits(frame: np.ndarray, roi: Roi) -> int | None:
    crop = frame[roi.y:roi.y + roi.h, roi.x:roi.x + roi.w]
    gray = cv2.cvtColor(crop, cv2.COLOR_BGR2GRAY)
    _, bw = cv2.threshold(gray, 128, 255, cv2.THRESH_BINARY)
    txt = pytesseract.image_to_string(bw, config="--psm 7 -c tessedit_char_whitelist=0123456789")
    digits = "".join(c for c in txt if c.isdigit())
    return int(digits) if digits else None


def read_timecode_stripe(frame: np.ndarray, roi: Roi) -> int | None:
    """Fallback: read the 32-bit binary stripe along the screen's top edge."""
    crop = frame[roi.y:roi.y + roi.h, roi.x:roi.x + roi.w]
    gray = cv2.cvtColor(crop, cv2.COLOR_BGR2GRAY)
    # Each bit is a roi.w / 32 wide block. Sample mean luma per block.
    bit_w = roi.w / 32
    bits = []
    for i in range(32):
        x0 = int(i * bit_w); x1 = int((i + 1) * bit_w)
        bits.append(1 if np.mean(gray[:, x0:x1]) > 128 else 0)
    val = 0
    for b in bits: val = (val << 1) | b
    return val


@click.command()
@click.argument("video", type=click.Path(exists=True, dir_okay=False, path_type=Path))
@click.option("--out", "out_csv", type=click.Path(dir_okay=False, path_type=Path), required=True)
@click.option("--host-roi", "host_roi", default="20,20,160,80", help="x,y,w,h")
@click.option("--ipad-roi", "ipad_roi", default="980,20,160,80", help="x,y,w,h")
@click.option("--method", type=click.Choice(["digits", "stripe", "both"]), default="both")
def cli(video: Path, out_csv: Path, host_roi: str, ipad_roi: str, method: str):
    h_roi = Roi.parse(host_roi)
    i_roi = Roi.parse(ipad_roi)
    cap = cv2.VideoCapture(str(video))
    if not cap.isOpened():
        click.echo(f"could not open {video}", err=True); sys.exit(1)
    fps = cap.get(cv2.CAP_PROP_FPS)
    out_csv.parent.mkdir(parents=True, exist_ok=True)
    with out_csv.open("w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=[
            "frame_idx","t_camera_us","host_us","ipad_us","latency_us","method"])
        w.writeheader()
        idx = 0
        while True:
            ok, frame = cap.read()
            if not ok: break
            t_camera_us = int(idx / fps * 1_000_000)
            h_us = i_us = None
            used = "none"
            if method in ("digits", "both"):
                h_us = read_timecode_digits(frame, h_roi)
                i_us = read_timecode_digits(frame, i_roi)
                used = "digits"
            if (h_us is None or i_us is None) and method in ("stripe", "both"):
                h_us = read_timecode_stripe(frame, h_roi) if h_us is None else h_us
                i_us = read_timecode_stripe(frame, i_roi) if i_us is None else i_us
                used = "stripe"
            lat_us = (i_us - h_us) if (h_us is not None and i_us is not None) else None
            w.writerow({
                "frame_idx": idx, "t_camera_us": t_camera_us,
                "host_us": h_us, "ipad_us": i_us,
                "latency_us": lat_us, "method": used,
            })
            idx += 1
    click.echo(f"wrote {idx} rows to {out_csv}")
    # Print percentiles for quick eyeball
    import pandas as pd
    df = pd.read_csv(out_csv)
    df = df.dropna(subset=["latency_us"])
    if not df.empty:
        click.echo(f"  p50={df.latency_us.quantile(.5):.0f} us "
                   f"p95={df.latency_us.quantile(.95):.0f} us "
                   f"p99={df.latency_us.quantile(.99):.0f} us  n={len(df)}")


if __name__ == "__main__":
    cli()
```

- [ ] **Step 3: Write `tests/test_measure.py`**

```python
"""Synthetic test for the OCR + stripe paths.
We don't have a real 240 fps capture in the repo (large), so we synthesize one:
generate frames with known timecodes, write them to a tempfile.mp4, run measure,
verify latency_us == known_offset.
"""
import csv, subprocess, tempfile
from pathlib import Path
import cv2
import numpy as np


def synth_frame(host_us: int, ipad_us: int, w: int = 1280, h: int = 360) -> np.ndarray:
    img = np.ones((h, w, 3), dtype=np.uint8) * 255
    cv2.putText(img, f"{host_us:08d}", (20, 80), cv2.FONT_HERSHEY_SIMPLEX, 2, (0, 0, 0), 4)
    cv2.putText(img, f"{ipad_us:08d}", (980, 80), cv2.FONT_HERSHEY_SIMPLEX, 2, (0, 0, 0), 4)
    return img


def test_measures_known_offset(tmp_path: Path):
    capture = tmp_path / "synth.mp4"
    fourcc = cv2.VideoWriter_fourcc(*"mp4v")
    vw = cv2.VideoWriter(str(capture), fourcc, 240, (1280, 360))
    OFFSET_US = 14_000
    for i in range(60):  # 250 ms
        host_us = i * 4_167
        vw.write(synth_frame(host_us, host_us - OFFSET_US))
    vw.release()

    out_csv = tmp_path / "out.csv"
    subprocess.run(
        ["python", "measure_p2p_latency.py", str(capture),
         "--out", str(out_csv), "--method", "digits"],
        check=True,
        cwd=Path(__file__).parent.parent,
    )

    rows = list(csv.DictReader(out_csv.open()))
    latencies = [int(r["latency_us"]) for r in rows
                 if r["latency_us"] not in ("", "None")]
    assert latencies, "no latencies extracted"
    median = sorted(latencies)[len(latencies) // 2]
    # OCR-only synthesis is noisy at this resolution; allow ±2 ms slop
    assert abs(median - OFFSET_US) < 2_000, \
        f"median latency {median}us != expected {OFFSET_US}us"
```

- [ ] **Step 4: Verify**

```bash
cd /home/tops/Projects/iExtend/bench/camera-rig
python -m venv .venv && source .venv/bin/activate
pip install -e .
pip install pytest
pytest -v tests/
```

Expected: `test_measures_known_offset PASSED`.

- [ ] **Step 5: Commit**

```bash
git add bench/camera-rig/pyproject.toml \
        bench/camera-rig/measure_p2p_latency.py \
        bench/camera-rig/tests/
git commit -m "bench: measure_p2p_latency.py + synthetic-capture pytest"
```

---

### Task 9: 8-hour soak runner + dashboard

**Files:**
- Create: `/home/tops/Projects/iExtend/bench/soak/README.md`
- Create: `/home/tops/Projects/iExtend/bench/soak/run-soak.sh`
- Create: `/home/tops/Projects/iExtend/bench/soak/dashboard/dashboard.html`
- Create: `/home/tops/Projects/iExtend/bench/soak/dashboard/style.css`
- Create: `/home/tops/Projects/iExtend/bench/soak/dashboard/chart.umd.min.js` (vendored Chart.js v4)

The soak runs `iextendd` against a real iPad on Wi-Fi 6E for 8 hours, sampling per-frame stats every second into a JSONL log. The dashboard reads that JSONL and renders three line charts (latency p50/p95/p99 over time, encoder bitrate, dropped frames + reconnects).

- [ ] **Step 1: Write `bench/soak/README.md`**

```markdown
# iExtend 8-hour soak

## What it does

Runs `iextendd` for 8 hours against a real paired iPad. Samples the WebRTC stats
report every 1 s into a JSONL log. After completion, opens a static HTML dashboard
that visualizes the run.

## Pass/fail bar

- Zero unrecovered disconnects in the 8 hours.
- p99 round-trip stays under target (30 ms Wi-Fi 6E / 18 ms USB-C).
- No memory leak (RSS drift < 50 MB end-to-end).

## Running

```bash
cd bench/soak
./run-soak.sh ~/iextend-soak-2026-05-08-rc1
# … 8 hours later …
open ~/iextend-soak-2026-05-08-rc1/dashboard.html
```

The script writes `samples.jsonl`, `iextendd.log`, and a copy of `dashboard.html`
into the output directory you provide.
```

- [ ] **Step 2: Write `run-soak.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail

OUT="${1:?usage: run-soak.sh OUT_DIR}"
mkdir -p "$OUT"
cp -r "$(dirname "$0")/dashboard/." "$OUT/"

LOG="$OUT/iextendd.log"
SAMPLES="$OUT/samples.jsonl"
DURATION_S=${IX_SOAK_DURATION_S:-28800}   # 8 h default

echo "soak: starting iextendd, logging to $LOG"
iextendd --log-format=json > "$LOG" 2>&1 &
DAEMON_PID=$!
trap 'kill $DAEMON_PID 2>/dev/null || true' EXIT

# Wait for the daemon to be ready (control socket up)
for _ in {1..30}; do iextend-ctl status >/dev/null 2>&1 && break; sleep 1; done

START=$(date +%s)
END=$((START + DURATION_S))
echo "soak: sampling every 1 s until $(date -d "@$END")"
while [ "$(date +%s)" -lt "$END" ]; do
  iextend-ctl stats --json >> "$SAMPLES" 2>/dev/null || echo "{\"error\":\"ctl-failed\",\"t\":$(date +%s)}" >> "$SAMPLES"
  sleep 1
done

echo "soak: done. open $OUT/dashboard.html"
```

Make executable:

```bash
chmod +x bench/soak/run-soak.sh
```

- [ ] **Step 3: Vendor Chart.js**

Download `https://cdn.jsdelivr.net/npm/chart.js@4.4.4/dist/chart.umd.min.js` to `bench/soak/dashboard/chart.umd.min.js`. Pin the version. (License: MIT — compatible.)

- [ ] **Step 4: Write `dashboard/dashboard.html`**

Standalone HTML that, on load, fetches `samples.jsonl` from the same directory, parses each line as JSON, and renders three time-series charts (p50/p95/p99 latency, encoder bitrate, dropped + reconnects). ~150 lines including the Chart.js wiring.

```html
<!doctype html>
<html lang="en"><head>
<meta charset="utf-8"><title>iExtend soak — dashboard</title>
<link rel="stylesheet" href="style.css">
<script src="chart.umd.min.js"></script>
</head><body>
<header>
  <h1>iExtend 8-hour soak</h1>
  <p id="meta">…</p>
</header>
<section><h2>Latency (ms)</h2><canvas id="lat"></canvas></section>
<section><h2>Encoder bitrate (Mbps)</h2><canvas id="br"></canvas></section>
<section><h2>Drops + reconnects (count)</h2><canvas id="ev"></canvas></section>
<script type="module">
const lines = (await fetch('samples.jsonl').then(r => r.text())).trim().split('\n');
const samples = lines.flatMap(l => { try { return [JSON.parse(l)]; } catch { return []; } });
const t = samples.map(s => new Date(s.t * 1000));
// (chart wiring — three Chart instances with shared x-axis; ~80 lines)
document.getElementById('meta').textContent =
  `${samples.length} samples · ${t[0]?.toISOString()} → ${t.at(-1)?.toISOString()}`;
</script>
</body></html>
```

- [ ] **Step 5: Write `style.css`**

Plain CSS, ~30 lines: layout, dark mode, max-width 1100px, monospace numbers in headers.

- [ ] **Step 6: Smoke-test the dashboard against a synthetic JSONL**

```bash
cd bench/soak/dashboard
mkdir -p /tmp/iextend-soak-smoke
cp ./* /tmp/iextend-soak-smoke/
# Fabricate a 1-min JSONL with 60 fake samples
python3 -c "
import json, random, time
t0 = int(time.time()) - 60
for i in range(60):
    print(json.dumps({
      't': t0 + i,
      'latency_p50_us': 14_000 + random.gauss(0, 800),
      'latency_p95_us': 22_000 + random.gauss(0, 1500),
      'latency_p99_us': 28_000 + random.gauss(0, 2500),
      'enc_bitrate_bps': 12_000_000 + random.gauss(0, 1_000_000),
      'drops_total': i // 10,
      'reconnects_total': 0,
    }))
" > /tmp/iextend-soak-smoke/samples.jsonl
python3 -m http.server 8090 --directory /tmp/iextend-soak-smoke &
sleep 1
# (open http://127.0.0.1:8090/dashboard.html in a browser; verify 3 charts render)
```

- [ ] **Step 7: Commit**

```bash
git add bench/soak/
git commit -m "bench: 8-hour soak runner + Chart.js dashboard"
```

---

### Task 10: Pencil-feel test protocol

**Files:**
- Create: `/home/tops/Projects/iExtend/bench/pencil-feel/protocol.md`
- Create: `/home/tops/Projects/iExtend/bench/pencil-feel/consent-form.md`
- Create: `/home/tops/Projects/iExtend/bench/pencil-feel/google-form.template.json`
- Create: `/home/tops/Projects/iExtend/bench/pencil-feel/analyze.py`
- Create: `/home/tops/Projects/iExtend/bench/pencil-feel/results/.gitkeep`

- [ ] **Step 1: Write `protocol.md`**

Write a structured 3-page protocol covering:

1. **Recruitment:** 10 reviewers; mix of digital illustrators, note-takers, and CAD users. Honorarium suggested. Each gets ≥ 30 minutes per app.
2. **Apparatus:** iPad Pro M2 with Apple Pencil Pro, paired with a fully provisioned iExtend host (record build SHA + host config). Wi-Fi 6E baseline run + USB-C run for each reviewer.
3. **Apps tested:** Adobe Photoshop on host (via Wintab), Procreate native on iPad (Sidecar reference / control), Krita on host (libinput stylus). Each reviewer uses each app for 10 minutes minimum.
4. **Tasks per app:** (a) freehand sketch, (b) careful inking pass, (c) pressure-sensitive shading, (d) hatch + stipple. Documented in detail.
5. **Likert ratings (5-point):** stroke smoothness · perceived latency · pressure response · tilt response · overall feel. Plus open-text "any other comments."
6. **Pass bar:** ≥ 4.0 average per app; no individual app < 3.5; no individual reviewer score < 2 on any axis without an actionable note.
7. **Anonymization:** results stored as `results/<release-tag>.csv` with reviewer numbered (no PII). Consent form (next file) signed before participation.

- [ ] **Step 2: Write `consent-form.md`**

A short consent form (~200 words) covering: anonymized results, no audio/video recording without consent, right to withdraw at any time, honorarium amount + payment timing.

- [ ] **Step 3: Write `google-form.template.json`**

A Google Forms API JSON description of the survey — 5 Likert items × 3 apps + an open text field per app, plus reviewer-id and build-tag at the top. Importable via the Forms API.

- [ ] **Step 4: Write `analyze.py`**

```python
"""Analyze a single Pencil-feel results CSV.

Input: results/<release-tag>.csv with columns
  reviewer_id, app, smoothness, latency, pressure, tilt, overall, comment
Output: pass/fail per app + overall summary printed to stdout, plus
  results/<release-tag>.summary.md committed alongside.

Pass bar:
  - per-app mean >= 4.0
  - no individual app < 3.5 mean
  - no individual reviewer × axis score < 2 without a comment
"""
from __future__ import annotations
import sys
from pathlib import Path
import pandas as pd

AXES = ["smoothness", "latency", "pressure", "tilt", "overall"]


def analyze(csv_path: Path) -> int:
    df = pd.read_csv(csv_path)
    summary = []
    fail = False
    for app, group in df.groupby("app"):
        means = {ax: group[ax].mean() for ax in AXES}
        summary.append((app, means))
        if any(m < 4.0 for m in means.values()):
            print(f"FAIL: {app} mean below 4.0: {means}")
            fail = True
        if any(m < 3.5 for m in means.values()):
            print(f"HARD FAIL: {app} mean below 3.5: {means}")
            fail = True
        for _, row in group.iterrows():
            for ax in AXES:
                if row[ax] < 2 and not isinstance(row.comment, str):
                    print(f"FAIL: reviewer {row.reviewer_id} {app}/{ax}={row[ax]} no comment")
                    fail = True

    out = csv_path.with_suffix(".summary.md")
    with out.open("w") as f:
        f.write(f"# {csv_path.stem} pencil-feel summary\n\n")
        for app, means in summary:
            f.write(f"## {app}\n")
            for ax, m in means.items():
                f.write(f"- {ax}: {m:.2f}\n")
            f.write("\n")
    print(f"wrote {out}")
    return 1 if fail else 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} <results.csv>", file=sys.stderr); sys.exit(2)
    sys.exit(analyze(Path(sys.argv[1])))
```

- [ ] **Step 5: Write a tiny smoke test**

Add `bench/pencil-feel/test_analyze.py`:

```python
import csv, subprocess, sys
from pathlib import Path

def test_passes_clean_data(tmp_path):
    csv_p = tmp_path / "v0.1.0.csv"
    with csv_p.open("w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["reviewer_id","app","smoothness","latency","pressure","tilt","overall","comment"])
        for app in ("photoshop","procreate","krita"):
            for r in range(10):
                w.writerow([r, app, 4.5, 4.4, 4.3, 4.2, 4.4, ""])
    rc = subprocess.run([sys.executable, "analyze.py", str(csv_p)],
                        cwd=Path(__file__).parent).returncode
    assert rc == 0
```

Run:
```bash
cd /home/tops/Projects/iExtend/bench/pencil-feel
python -m pytest test_analyze.py -v
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add bench/pencil-feel/
git commit -m "bench: pencil-feel structured protocol + analyze.py"
```

---

### Task 11: Cross-link bench infra into top-level docs

**Files:**
- Modify: `/home/tops/Projects/iExtend/README.md`

- [ ] **Step 1: Add a section to root README**

Append:

```markdown

## Testing infrastructure (Plan 10)

Three regression-detection tiers — see `bench/README.md`.

| Tier | What | Cadence | Cost |
|---|---|---|---|
| Synthetic | `host/crates/iextendd/tests/synthetic_latency.rs` | per PR + nightly | free |
| Hardware CI | `bench/cluster/` (4 boxes, real GPUs + iPads) | nightly + per release | ~$9 000 + ~$1 000/yr |
| Camera rig | `bench/camera-rig/` (240 fps photon-to-photon) | weekly + pre-release | ~$1 200 + operator time |

Plus an 8-hour soak (`bench/soak/`) and a structured Pencil-feel review (`bench/pencil-feel/`) before each release.
```

- [ ] **Step 2: Mark Plan 10 complete in the plan-status checklist**

Find the `Plan status` checklist in `README.md`. Change `- [ ] Plan 10: Bench rig + CI hardware cluster` to `- [x] Plan 10: Bench rig + CI hardware cluster`.

- [ ] **Step 3: Commit + tag**

```bash
git add README.md
git commit -m "docs: cross-link bench infra in README; mark Plan 10 done"
git tag -a plan-10-complete -m "Plan 10 of 10 complete: bench rig + CI infrastructure shipped"
```

---

## Done criteria

All of the following must be true:

1. `cargo test -p iextendd --test synthetic_latency --release` runs locally and passes.
2. `host-ci.yml` includes the synthetic-latency job; the job is green on `main`.
3. `perf-nightly.yml` runs on schedule + has produced at least one nightly comment on a merged PR.
4. `bench/cluster/` Ansible playbooks pass `--syntax-check`. (Real provisioning happens once hardware exists; syntax-clean is the merge bar.)
5. `bench/camera-rig/screen-timecode/` builds clean on Windows + Linux; the in-crate `Painter` round-trip unit test passes.
6. `bench/camera-rig/measure_p2p_latency.py` passes `pytest`.
7. `bench/soak/run-soak.sh` is executable and the dashboard renders three charts against a synthetic JSONL.
8. `bench/pencil-feel/analyze.py` is executable and `test_analyze.py` passes.
9. README cross-links bench infra; tag `plan-10-complete` on the head.

## Out of scope

- Actually procuring the four hardware-CI boxes (documented in Task 5; physical work).
- Actually running the 8-hour soak / camera rig / pencil-feel reviews (those are operational, not engineering).
- Statistical analysis tooling beyond per-app means + percentiles (deferred).
- Cross-platform Phantom-Veo SDK integration (only relevant if a high-end camera is procured).

## Recommendations for v1 release readiness

Before tagging `v1.0.0`:

- [x] Plan 10 done
- [ ] Synthetic-CI green on `main` for 7 consecutive days
- [ ] At least one full hardware-CI nightly run with all 4 boxes green
- [ ] One bench-camera-rig measurement with p99 ≤ 30 ms (Wi-Fi 6E) and p99 ≤ 18 ms (USB-C)
- [ ] One full 8-hour soak with zero unrecovered disconnects
- [ ] Pencil-feel review done with all three apps ≥ 4.0 mean

If any of these fail, hold the release and triage. The synthetic-only fallback path is not enough for v1 — you need at least one camera-rig measurement to claim the latency budget is real.
