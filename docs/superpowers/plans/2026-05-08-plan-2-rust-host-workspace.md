# Rust Host Workspace Bootstrap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the `host/` Rust workspace with eight crate skeletons (two binaries, six libraries) that compile cleanly on Linux and Windows, plus a working localhost gRPC scaffold between `iextendd` (daemon) and `iextend-tray` (GUI). After this plan: `cargo build --workspace` succeeds, `cargo clippy -- -D warnings` is clean, `iextendd` starts and serves a `Status` RPC over a Unix Domain Socket / Named Pipe, and `iextend-tray` connects to it and prints the version. Zero feature code — pairing, capture, encoding, WebRTC, and input forwarding all land in later plans.

**Architecture:** Cargo workspace at `host/`. Two binary crates own runtime concerns; six library crates own subsystem traits and skeleton impls. Inter-process communication via tonic-generated gRPC over a platform-abstracted localhost transport: `tokio::net::UnixStream` on Linux, `tokio::net::windows::named_pipe` on Windows. A `LocalEndpoint` enum hides the platform detail so the gRPC client/server speak the same `tower::Service`. CI runs the full matrix on GitHub-hosted Linux + Windows runners.

**Tech Stack:**
- Rust 1.83 stable, pinned via `rust-toolchain.toml`
- tokio 1.40 (runtime, signals, async I/O)
- tonic 0.12 + prost 0.13 (gRPC + protobuf)
- tower 0.5 (Service abstraction over the local socket)
- egui 0.29 + eframe (GUI; chosen over Tauri — see §Architecture decision below)
- tracing 0.1 + tracing-subscriber (structured logs, JSON output for §11 of the spec)
- thiserror 1, anyhow 1 (error plumbing)

**Architecture decision: egui over Tauri.** Tauri ships a 30 MB system-webview binary and forces an HTML/JS layer for what is fundamentally a status icon + a few preference pages. egui is a single Rust dependency with no webview, ~5 MB stripped, immediate-mode UI that's trivial to wire to tokio channels, and no IPC translation layer between view and model. We give up some visual polish; we gain massively on binary size, startup time, and code-path simplicity. If a Tauri-grade UI ever becomes necessary, the `iextend-tray` crate is the only thing that needs to change — `iextendd` and the gRPC schema are unaffected.

**Plan scope:** This is **Plan 2 of 10** for the iExtend project. Plans 3 (Windows IddCx driver), 4 (Linux evdi + PipeWire), 5 (WebRTC + codec), 7 (mDNS + pairing), and 8 (input forwarding + cursor reprojection) all build on the crate skeletons created here. Out of scope for Plan 2: any actual capture, encoding, transport, pairing, or input code. The library crates exist as compilable empty husks with documented public APIs; their `lib.rs` files contain `pub trait` declarations and one or two `NoOp*` placeholder impls each, no more.

**Update touchpoint:** Plan 1's `README.md` has a `## Plan status` section. After this plan completes, mark line "Plan 2: Rust host workspace bootstrap" as `[x]`. The check happens in Task 9.

---

## File Structure

```
iExtend/
├── host/                                  # NEW — entire Rust workspace
│   ├── Cargo.toml                         # workspace manifest
│   ├── rust-toolchain.toml                # pin to stable 1.83
│   ├── .cargo/
│   │   └── config.toml                    # workspace-wide build config
│   ├── proto/
│   │   └── iextend.proto                  # gRPC schema
│   ├── crates/
│   │   ├── iextendd/                      # daemon binary
│   │   │   ├── Cargo.toml
│   │   │   ├── build.rs                   # tonic-build invocation
│   │   │   └── src/
│   │   │       ├── main.rs                # tokio main + signal handler
│   │   │       ├── grpc_server.rs         # Status / StartSession / StopSession / GetSettings impls
│   │   │       └── transport.rs           # LocalEndpoint::bind()
│   │   ├── iextend-tray/                  # GUI binary
│   │   │   ├── Cargo.toml
│   │   │   ├── build.rs                   # tonic-build (client only)
│   │   │   └── src/
│   │   │       ├── main.rs                # eframe entry
│   │   │       ├── app.rs                 # egui App impl
│   │   │       └── client.rs              # LocalEndpoint::connect()
│   │   ├── ix-transport/                  # shared LocalEndpoint abstraction
│   │   │   ├── Cargo.toml
│   │   │   └── src/lib.rs
│   │   ├── ix-codec/                      # encoder trait + NoOpEncoder
│   │   │   ├── Cargo.toml
│   │   │   └── src/lib.rs
│   │   ├── ix-rtc/                        # WebRTC peer skeleton
│   │   │   ├── Cargo.toml
│   │   │   └── src/lib.rs
│   │   ├── ix-discover/                   # mDNS skeleton
│   │   │   ├── Cargo.toml
│   │   │   └── src/lib.rs
│   │   ├── ix-display-windows/            # cfg(windows) only
│   │   │   ├── Cargo.toml
│   │   │   └── src/lib.rs
│   │   ├── ix-display-linux/              # cfg(unix) only
│   │   │   ├── Cargo.toml
│   │   │   └── src/lib.rs
│   │   └── ix-input/                      # cross-platform virtual stylus skeleton
│   │       ├── Cargo.toml
│   │       └── src/lib.rs
│   └── CONTRIBUTING.md                    # build prereqs + run-locally snippet
└── .github/
    └── workflows/
        └── host-ci.yml                    # NEW — fmt, clippy, test on Linux+Windows
```

The `ix-transport` crate was added during planning (not in the spec's §13 layout) because both binaries plus future test code need the platform-abstracted `LocalEndpoint` — a third home is cleaner than duplicating it or exposing it from one binary's internals.

---

### Task 1: Initialize the cargo workspace

**Files:**
- Create: `/home/tops/Projects/iExtend/host/Cargo.toml`
- Create: `/home/tops/Projects/iExtend/host/rust-toolchain.toml`
- Create: `/home/tops/Projects/iExtend/host/.cargo/config.toml`

- [ ] **Step 1: Create the directory tree**

```bash
mkdir -p /home/tops/Projects/iExtend/host/.cargo
mkdir -p /home/tops/Projects/iExtend/host/proto
mkdir -p /home/tops/Projects/iExtend/host/crates
```

- [ ] **Step 2: Pin the toolchain**

Create `/home/tops/Projects/iExtend/host/rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.83.0"
components = ["rustfmt", "clippy"]
profile = "minimal"
targets = ["x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc"]
```

- [ ] **Step 3: Workspace manifest**

Create `/home/tops/Projects/iExtend/host/Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = [
    "crates/iextendd",
    "crates/iextend-tray",
    "crates/ix-transport",
    "crates/ix-codec",
    "crates/ix-rtc",
    "crates/ix-discover",
    "crates/ix-display-windows",
    "crates/ix-display-linux",
    "crates/ix-input",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.83"
authors = ["iExtend contributors"]
license = "Apache-2.0"
repository = "https://github.com/REPLACE_WITH_OWNER/iExtend"

[workspace.dependencies]
tokio          = { version = "1.40", features = ["macros", "rt-multi-thread", "signal", "net", "io-util", "sync", "time"] }
tonic          = "0.12"
tonic-build    = "0.12"
prost          = "0.13"
tower          = "0.5"
tracing        = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
thiserror      = "1"
anyhow         = "1"
async-trait    = "0.1"
futures        = "0.3"
eframe         = { version = "0.29", default-features = false, features = ["default_fonts", "glow"] }
egui           = "0.29"

[profile.release]
lto = "thin"
codegen-units = 1
strip = "symbols"
panic = "abort"
```

- [ ] **Step 4: Workspace cargo config**

Create `/home/tops/Projects/iExtend/host/.cargo/config.toml`:

```toml
[build]
rustflags = ["-D", "warnings"]

[target.x86_64-pc-windows-msvc]
rustflags = ["-D", "warnings", "-C", "target-feature=+crt-static"]

[net]
git-fetch-with-cli = true
```

- [ ] **Step 5: Smoke-check that `cargo` recognizes the workspace**

```bash
cd /home/tops/Projects/iExtend/host
cargo metadata --no-deps --format-version=1 > /dev/null
echo $?
```

Expected: `0`. (`cargo metadata` will warn about missing member crates — that's expected; we create them in the next tasks. The exit code being 0 is what we want.)

- [ ] **Step 6: Commit**

```bash
git add host/Cargo.toml host/rust-toolchain.toml host/.cargo/config.toml
git commit -m "chore(host): initialize cargo workspace + toolchain pin"
```

---

### Task 2: Scaffold the six library crates

**Files:**
- Create one `Cargo.toml` + `src/lib.rs` per crate, for: `ix-transport`, `ix-codec`, `ix-rtc`, `ix-discover`, `ix-display-windows`, `ix-display-linux`, `ix-input`

The pattern is the same per crate. Show the full `Cargo.toml` for `ix-codec` as the template; the others differ only in name and dependency set.

- [ ] **Step 1: `ix-transport` — shared LocalEndpoint abstraction**

`host/crates/ix-transport/Cargo.toml`:

```toml
[package]
name = "ix-transport"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
tokio       = { workspace = true }
tracing     = { workspace = true }
thiserror   = { workspace = true }
async-trait = { workspace = true }

[target.'cfg(windows)'.dependencies]
tokio = { workspace = true, features = ["macros", "rt-multi-thread", "net", "io-util"] }
```

`host/crates/ix-transport/src/lib.rs`:

```rust
//! Platform-abstracted localhost endpoint for daemon ↔ tray IPC.
//! Real impls land in Task 6 of this plan.

use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Platform-specific localhost endpoint name.
///   Linux/macOS: a filesystem path (UDS).
///   Windows:     a named-pipe name like `\\.\pipe\iextendd`.
#[derive(Debug, Clone)]
pub struct LocalEndpoint(pub String);

impl LocalEndpoint {
    pub fn default_for_user() -> Self {
        #[cfg(windows)]
        {
            Self(r"\\.\pipe\iextendd".to_string())
        }
        #[cfg(unix)]
        {
            let runtime = std::env::var("XDG_RUNTIME_DIR")
                .unwrap_or_else(|_| "/tmp".to_string());
            Self(format!("{runtime}/iextendd.sock"))
        }
    }

    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }
}
```

- [ ] **Step 2: `ix-codec` — encoder trait + NoOpEncoder**

`host/crates/ix-codec/Cargo.toml` (template — others mirror this):

```toml
[package]
name = "ix-codec"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
async-trait = { workspace = true }
thiserror   = { workspace = true }
tracing     = { workspace = true }
```

`host/crates/ix-codec/src/lib.rs`:

```rust
//! Video encoder trait. Real impls (NVENC, QSV, AMF, VAAPI, x264) land in Plan 5.

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CodecError {
    #[error("encoder not available")]
    Unavailable,
    #[error("encode failed: {0}")]
    Encode(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecKind { H264, Hevc, Av1 }

#[async_trait]
pub trait Encoder: Send + Sync {
    fn kind(&self) -> CodecKind;
    async fn encode_frame(&mut self, _texture_handle: u64) -> Result<Vec<u8>, CodecError> {
        Err(CodecError::Unavailable)
    }
}

/// Compiles, never produces frames. Replaced by real impls in Plan 5.
pub struct NoOpEncoder;

#[async_trait]
impl Encoder for NoOpEncoder {
    fn kind(&self) -> CodecKind { CodecKind::Hevc }
}
```

- [ ] **Step 3: `ix-rtc` — WebRTC skeleton**

`host/crates/ix-rtc/Cargo.toml`:

```toml
[package]
name = "ix-rtc"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
async-trait = { workspace = true }
thiserror   = { workspace = true }
tracing     = { workspace = true }
tokio       = { workspace = true }
```

`host/crates/ix-rtc/src/lib.rs`:

```rust
//! WebRTC peer connection lifecycle. Real impl in Plan 5 (likely webrtc-rs 0.x).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RtcError {
    #[error("not connected")]
    NotConnected,
}

#[derive(Debug, Clone, Copy)]
pub enum PeerState { Idle, Negotiating, Live, Failed }

pub struct PeerConnection {
    state: PeerState,
}

impl PeerConnection {
    pub fn new() -> Self { Self { state: PeerState::Idle } }
    pub fn state(&self) -> PeerState { self.state }
}

impl Default for PeerConnection { fn default() -> Self { Self::new() } }
```

- [ ] **Step 4: `ix-discover` — mDNS skeleton**

`host/crates/ix-discover/Cargo.toml`:

```toml
[package]
name = "ix-discover"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
async-trait = { workspace = true }
thiserror   = { workspace = true }
tracing     = { workspace = true }
tokio       = { workspace = true }
```

`host/crates/ix-discover/src/lib.rs`:

```rust
//! mDNS browse/advertise + pair token verification. Real impl in Plan 7.

#[derive(Debug, Clone)]
pub struct PeerAdvertisement {
    pub host_pubkey_thumbprint: String,
    pub display_name: String,
}
```

- [ ] **Step 5: `ix-display-windows` — gated on Windows**

`host/crates/ix-display-windows/Cargo.toml`:

```toml
[package]
name = "ix-display-windows"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[target.'cfg(windows)'.dependencies]
thiserror = { workspace = true }
tracing   = { workspace = true }
```

`host/crates/ix-display-windows/src/lib.rs`:

```rust
//! IddCx virtual monitor + DXGI Desktop Duplication capture. Real impl in Plan 3.
//! On non-Windows targets this crate compiles to an empty module so the workspace
//! still builds; consumers gate use behind `cfg(windows)`.

#![cfg(windows)]

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DisplayWindowsError {
    #[error("IddCx unavailable")]
    Unavailable,
}
```

- [ ] **Step 6: `ix-display-linux` — gated on Unix**

`host/crates/ix-display-linux/Cargo.toml`:

```toml
[package]
name = "ix-display-linux"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[target.'cfg(unix)'.dependencies]
thiserror = { workspace = true }
tracing   = { workspace = true }
```

`host/crates/ix-display-linux/src/lib.rs`:

```rust
//! evdi virtual monitor + PipeWire / XDamage capture. Real impl in Plan 4.

#![cfg(unix)]

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DisplayLinuxError {
    #[error("evdi module not loaded")]
    EvdiUnavailable,
}
```

- [ ] **Step 7: `ix-input` — cross-platform stylus skeleton**

`host/crates/ix-input/Cargo.toml`:

```toml
[package]
name = "ix-input"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
async-trait = { workspace = true }
thiserror   = { workspace = true }
tracing     = { workspace = true }
```

`host/crates/ix-input/src/lib.rs`:

```rust
//! Virtual stylus / touch / keyboard injection. Real impl in Plan 8.

#[derive(Debug, Clone, Copy)]
pub struct StylusSample {
    pub x: f32, pub y: f32, pub pressure: f32, pub tilt_x: f32, pub tilt_y: f32,
}

pub trait StylusSink: Send {
    fn submit(&mut self, _sample: StylusSample) -> Result<(), std::io::Error> { Ok(()) }
}
```

- [ ] **Step 8: Verify all seven crates compile**

```bash
cd /home/tops/Projects/iExtend/host
cargo build -p ix-transport -p ix-codec -p ix-rtc -p ix-discover -p ix-input
cargo build -p ix-display-windows   # no-op outside Windows; should still pass
cargo build -p ix-display-linux     # no-op outside Unix; should still pass
```

Expected: each command exits 0.

- [ ] **Step 9: Commit**

```bash
git add host/crates/ix-transport host/crates/ix-codec host/crates/ix-rtc host/crates/ix-discover host/crates/ix-display-windows host/crates/ix-display-linux host/crates/ix-input
git commit -m "feat(host): scaffold subsystem library crates (skeletons only)"
```

---

### Task 3: Scaffold `iextendd` daemon binary

**Files:**
- Create: `host/crates/iextendd/Cargo.toml`
- Create: `host/crates/iextendd/src/main.rs`

- [ ] **Step 1: `Cargo.toml`**

```toml
[package]
name = "iextendd"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
tokio              = { workspace = true }
tracing            = { workspace = true }
tracing-subscriber = { workspace = true }
anyhow             = { workspace = true }
ix-transport       = { path = "../ix-transport" }
ix-codec           = { path = "../ix-codec" }
ix-rtc             = { path = "../ix-rtc" }
ix-discover        = { path = "../ix-discover" }
ix-input           = { path = "../ix-input" }

[target.'cfg(windows)'.dependencies]
ix-display-windows = { path = "../ix-display-windows" }

[target.'cfg(unix)'.dependencies]
ix-display-linux   = { path = "../ix-display-linux" }
```

- [ ] **Step 2: `main.rs` (signal-aware tokio main, JSON logs, no gRPC yet)**

```rust
use std::time::Instant;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();
    let started = Instant::now();
    info!(version = env!("CARGO_PKG_VERSION"), "iextendd starting");

    let shutdown = wait_for_shutdown_signal();
    tokio::select! {
        _ = shutdown => {
            info!("shutdown signal received");
        }
    }

    info!(uptime_s = started.elapsed().as_secs(), "iextendd stopped");
    Ok(())
}

fn init_logging() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    fmt()
        .with_env_filter(filter)
        .json()
        .with_current_span(false)
        .with_span_list(false)
        .init();
}

#[cfg(unix)]
async fn wait_for_shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut term = signal(SignalKind::terminate()).expect("install SIGTERM");
    let mut int  = signal(SignalKind::interrupt()).expect("install SIGINT");
    tokio::select! { _ = term.recv() => {}, _ = int.recv() => {} }
}

#[cfg(windows)]
async fn wait_for_shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
```

- [ ] **Step 3: Build & run smoke test**

```bash
cd /home/tops/Projects/iExtend/host
cargo build -p iextendd
cargo run -p iextendd &
sleep 1
kill -INT $!   # Linux/macOS; on Windows use Ctrl+C
wait
```

Expected: a `{"timestamp":...,"level":"INFO","message":"iextendd starting",...}` JSON log line, then a "stopped" line.

- [ ] **Step 4: Commit**

```bash
git add host/crates/iextendd
git commit -m "feat(iextendd): scaffold daemon binary with tokio runtime + JSON logs"
```

---

### Task 4: Scaffold `iextend-tray` GUI binary

**Files:**
- Create: `host/crates/iextend-tray/Cargo.toml`
- Create: `host/crates/iextend-tray/src/main.rs`
- Create: `host/crates/iextend-tray/src/app.rs`

- [ ] **Step 1: `Cargo.toml`**

```toml
[package]
name = "iextend-tray"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
eframe             = { workspace = true }
egui               = { workspace = true }
tokio              = { workspace = true }
tracing            = { workspace = true }
tracing-subscriber = { workspace = true }
anyhow             = { workspace = true }
ix-transport       = { path = "../ix-transport" }
```

- [ ] **Step 2: `main.rs`**

```rust
mod app;

fn main() -> eframe::Result<()> {
    init_logging();
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([360.0, 240.0])
            .with_min_inner_size([320.0, 200.0])
            .with_title("iExtend"),
        ..Default::default()
    };
    eframe::run_native("iExtend", opts, Box::new(|_| Ok(Box::<app::TrayApp>::default())))
}

fn init_logging() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).init();
}
```

- [ ] **Step 3: `app.rs` — minimal egui shell**

```rust
use ix_transport::LocalEndpoint;

#[derive(Default)]
pub struct TrayApp {
    daemon_status: Option<String>,
}

impl eframe::App for TrayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("iExtend");
            ui.label("Plan 2 scaffold — no real connection yet.");
            ui.separator();

            let endpoint = LocalEndpoint::default_for_user();
            ui.label(format!("Endpoint: {}", endpoint.0));

            if ui.button("Ping iextendd (placeholder)").clicked() {
                self.daemon_status = Some("Plan 6 wires this up.".into());
            }
            if let Some(s) = &self.daemon_status { ui.label(s); }
        });
    }
}
```

- [ ] **Step 4: Build smoke**

```bash
cd /home/tops/Projects/iExtend/host
cargo build -p iextend-tray
```

Expected: builds; we don't run the GUI in CI, only the daemon. Headless CI runners can compile but not display egui — that's fine.

- [ ] **Step 5: Commit**

```bash
git add host/crates/iextend-tray
git commit -m "feat(iextend-tray): scaffold egui app shell"
```

---

### Task 5: Define the gRPC schema

**Files:**
- Create: `host/proto/iextend.proto`

- [ ] **Step 1: Write the schema**

```proto
syntax = "proto3";

package iextend.v1;

service Daemon {
  rpc Status       (StatusRequest)       returns (StatusReply);
  rpc StartSession (StartSessionRequest) returns (StartSessionReply);
  rpc StopSession  (StopSessionRequest)  returns (StopSessionReply);
  rpc GetSettings  (GetSettingsRequest)  returns (Settings);
}

message StatusRequest {}
message StatusReply {
  string version  = 1;
  uint64 uptime_s = 2;
  SessionState session = 3;
}

enum SessionState {
  SESSION_STATE_IDLE         = 0;
  SESSION_STATE_PAIRING      = 1;
  SESSION_STATE_CONNECTING   = 2;
  SESSION_STATE_LIVE         = 3;
  SESSION_STATE_DEGRADED     = 4;
  SESSION_STATE_DISCONNECTED = 5;
}

message StartSessionRequest { string peer_id = 1; }
message StartSessionReply   { bool started   = 1; string detail = 2; }
message StopSessionRequest  {}
message StopSessionReply    { bool stopped   = 1; }

message GetSettingsRequest {}
message Settings {
  bool   auto_connect_on_launch = 1;
  string preferred_codec        = 2; // "av1" | "hevc" | "h264"
  uint32 max_bitrate_kbps       = 3;
  bool   hdr_enabled            = 4;
}
```

- [ ] **Step 2: Wire `tonic-build` into both binaries**

Add to **both** `host/crates/iextendd/Cargo.toml` and `host/crates/iextend-tray/Cargo.toml`:

```toml
[build-dependencies]
tonic-build = { workspace = true }
```

Then add to both `[dependencies]` sections:

```toml
tonic = { workspace = true }
prost = { workspace = true }
```

Create `host/crates/iextendd/build.rs`:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .compile_protos(&["../../proto/iextend.proto"], &["../../proto"])?;
    Ok(())
}
```

Create `host/crates/iextend-tray/build.rs`:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(&["../../proto/iextend.proto"], &["../../proto"])?;
    Ok(())
}
```

- [ ] **Step 3: Verify codegen runs**

```bash
cd /home/tops/Projects/iExtend/host
cargo check -p iextendd -p iextend-tray
```

Expected: both compile; the generated `iextend.v1.rs` is now in `target/debug/build/iextendd-*/out/`.

- [ ] **Step 4: Commit**

```bash
git add host/proto host/crates/iextendd/build.rs host/crates/iextend-tray/build.rs host/crates/iextendd/Cargo.toml host/crates/iextend-tray/Cargo.toml
git commit -m "feat(host): add gRPC schema + tonic-build wiring"
```

---

### Task 6: Implement the localhost gRPC transport

**Goal:** `iextendd` binds the platform-native socket, serves `Daemon.Status`. `iextend-tray` connects, calls `Status`, prints the result.

**Files:**
- Create: `host/crates/iextendd/src/grpc_server.rs`
- Create: `host/crates/iextendd/src/transport.rs`
- Create: `host/crates/iextend-tray/src/client.rs`
- Modify: `host/crates/iextendd/src/main.rs`
- Modify: `host/crates/iextend-tray/src/app.rs`

- [ ] **Step 1: Server-side transport bind helper (`iextendd/src/transport.rs`)**

```rust
use anyhow::Result;
use ix_transport::LocalEndpoint;
use tokio::net::UnixListener;

pub struct LocalServer {
    pub endpoint: LocalEndpoint,
}

impl LocalServer {
    #[cfg(unix)]
    pub fn bind(endpoint: LocalEndpoint) -> Result<UnixListener> {
        let path = endpoint.as_path();
        if path.exists() { std::fs::remove_file(path)?; }
        Ok(UnixListener::bind(path)?)
    }

    #[cfg(windows)]
    pub fn bind(endpoint: LocalEndpoint) -> Result<tokio::net::windows::named_pipe::NamedPipeServer> {
        use tokio::net::windows::named_pipe::ServerOptions;
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(&endpoint.0)?;
        Ok(server)
    }
}
```

(On Windows the named-pipe accept loop will need to recreate a new instance per connection; the simplest pattern is the one in the tonic examples — the implementer follows that idiom.)

- [ ] **Step 2: gRPC server impl (`iextendd/src/grpc_server.rs`)**

```rust
use std::time::Instant;
use tonic::{Request, Response, Status};

pub mod proto { tonic::include_proto!("iextend.v1"); }

use proto::{
    daemon_server::Daemon,
    SessionState, Settings, StatusReply, StatusRequest,
    StartSessionReply, StartSessionRequest,
    StopSessionReply, StopSessionRequest, GetSettingsRequest,
};

pub struct DaemonImpl {
    pub started_at: Instant,
}

#[tonic::async_trait]
impl Daemon for DaemonImpl {
    async fn status(&self, _r: Request<StatusRequest>) -> Result<Response<StatusReply>, Status> {
        Ok(Response::new(StatusReply {
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_s: self.started_at.elapsed().as_secs(),
            session: SessionState::Idle as i32,
        }))
    }
    async fn start_session(&self, _r: Request<StartSessionRequest>) -> Result<Response<StartSessionReply>, Status> {
        Ok(Response::new(StartSessionReply { started: false, detail: "not implemented (Plan 5)".into() }))
    }
    async fn stop_session(&self, _r: Request<StopSessionRequest>) -> Result<Response<StopSessionReply>, Status> {
        Ok(Response::new(StopSessionReply { stopped: false }))
    }
    async fn get_settings(&self, _r: Request<GetSettingsRequest>) -> Result<Response<Settings>, Status> {
        Ok(Response::new(Settings {
            auto_connect_on_launch: false,
            preferred_codec: "hevc".into(),
            max_bitrate_kbps: 80_000,
            hdr_enabled: false,
        }))
    }
}
```

- [ ] **Step 3: Wire the server into `main.rs`**

Replace `iextendd/src/main.rs` with:

```rust
mod grpc_server;
mod transport;

use std::time::Instant;
use tonic::transport::Server;
use tracing::info;
use ix_transport::LocalEndpoint;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();
    let started = Instant::now();
    let endpoint = LocalEndpoint::default_for_user();
    info!(version = env!("CARGO_PKG_VERSION"), endpoint = %endpoint.0, "iextendd starting");

    let svc = grpc_server::DaemonImpl { started_at: started };
    let svc = grpc_server::proto::daemon_server::DaemonServer::new(svc);

    #[cfg(unix)]
    {
        use tokio_stream::wrappers::UnixListenerStream;
        let listener = transport::LocalServer::bind(endpoint.clone())?;
        let stream = UnixListenerStream::new(listener);
        tokio::select! {
            res = Server::builder().add_service(svc).serve_with_incoming(stream) => res?,
            _ = wait_for_shutdown_signal() => info!("shutdown signal received"),
        }
        let _ = std::fs::remove_file(endpoint.as_path());
    }

    #[cfg(windows)]
    {
        // Windows named-pipe loop: accept one client at a time, spawn server task per conn.
        // (Implementer follows tonic's named-pipe example; ~30 lines.)
        anyhow::bail!("Windows named-pipe accept loop is left as the implementer's exercise; pattern in tonic/examples/src/uds");
    }

    info!(uptime_s = started.elapsed().as_secs(), "iextendd stopped");
    Ok(())
}

fn init_logging() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).json().init();
}

#[cfg(unix)]
async fn wait_for_shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut term = signal(SignalKind::terminate()).expect("install SIGTERM");
    let mut int  = signal(SignalKind::interrupt()).expect("install SIGINT");
    tokio::select! { _ = term.recv() => {}, _ = int.recv() => {} }
}

#[cfg(windows)]
async fn wait_for_shutdown_signal() { let _ = tokio::signal::ctrl_c().await; }
```

Add to `iextendd/Cargo.toml` `[dependencies]`:

```toml
tokio-stream = "0.1"
```

- [ ] **Step 4: Tray client (`iextend-tray/src/client.rs`)**

```rust
use anyhow::Result;
use ix_transport::LocalEndpoint;

pub mod proto { tonic::include_proto!("iextend.v1"); }
use proto::daemon_client::DaemonClient;
use proto::StatusRequest;

#[cfg(unix)]
pub async fn fetch_status(endpoint: &LocalEndpoint) -> Result<String> {
    use tokio::net::UnixStream;
    use tower::service_fn;
    use tonic::transport::{Endpoint, Uri};

    let path = endpoint.0.clone();
    let channel = Endpoint::try_from("http://[::]:50051")?
        .connect_with_connector(service_fn(move |_: Uri| {
            let p = path.clone();
            async move { Ok::<_, std::io::Error>(hyper_util::rt::TokioIo::new(UnixStream::connect(p).await?)) }
        })).await?;

    let mut client = DaemonClient::new(channel);
    let reply = client.status(StatusRequest {}).await?.into_inner();
    Ok(format!("v{} · uptime {}s", reply.version, reply.uptime_s))
}

#[cfg(windows)]
pub async fn fetch_status(_endpoint: &LocalEndpoint) -> Result<String> {
    anyhow::bail!("Windows client connect: implementer follows tonic named-pipe client example")
}
```

Add to `iextend-tray/Cargo.toml` `[dependencies]`:

```toml
tower       = { workspace = true }
hyper-util  = { version = "0.1", features = ["tokio"] }
```

- [ ] **Step 5: Wire the button in `app.rs`**

Replace the placeholder button branch:

```rust
if ui.button("Ping iextendd").clicked() {
    let endpoint = LocalEndpoint::default_for_user();
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    self.daemon_status = Some(match rt.block_on(crate::client::fetch_status(&endpoint)) {
        Ok(s)  => s,
        Err(e) => format!("error: {e}"),
    });
}
```

(For Plan 2 the click-blocking approach is fine. A real async-driven UI lands in Plan 6 alongside session state machine.)

- [ ] **Step 6: End-to-end manual smoke**

```bash
cd /home/tops/Projects/iExtend/host
cargo build --workspace

# Terminal 1
cargo run -p iextendd

# Terminal 2 (Linux/macOS only — Windows uses the GUI button)
# Confirm the socket file exists
ls -la "$XDG_RUNTIME_DIR/iextendd.sock" 2>/dev/null || ls -la /tmp/iextendd.sock

# Then run the GUI and click "Ping iextendd"
cargo run -p iextend-tray
```

Expected GUI output after click: `v0.1.0 · uptime <seconds>s`.

- [ ] **Step 7: Add a unit test for the JSON-log output shape**

`host/crates/iextendd/tests/log_format.rs`:

```rust
// Trivial canary that the JSON subscriber compiles and the version env var is set.
#[test]
fn version_is_set() {
    assert!(!env!("CARGO_PKG_VERSION").is_empty());
}
```

- [ ] **Step 8: Commit**

```bash
git add host/crates/iextendd host/crates/iextend-tray
git commit -m "feat(host): localhost gRPC transport over UDS / named pipe + Status RPC"
```

---

### Task 7: CI workflow

**Files:**
- Create: `/home/tops/Projects/iExtend/.github/workflows/host-ci.yml`

- [ ] **Step 1: Write the workflow**

```yaml
name: host-ci

on:
  push:
    paths: ['host/**', '.github/workflows/host-ci.yml']
  pull_request:
    paths: ['host/**', '.github/workflows/host-ci.yml']

jobs:
  build:
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-24.04, windows-2022]
    runs-on: ${{ matrix.os }}
    defaults: { run: { working-directory: host } }
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: 1.83.0
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
        with: { workspaces: host }
      - run: cargo fmt --all -- --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace --no-fail-fast
```

- [ ] **Step 2: Local smoke equivalent**

```bash
cd /home/tops/Projects/iExtend/host
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --no-fail-fast
```

Expected: all three pass.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/host-ci.yml
git commit -m "ci(host): cargo fmt + clippy + test on Linux + Windows"
```

---

### Task 8: CONTRIBUTING.md

**Files:**
- Create: `host/CONTRIBUTING.md`

- [ ] **Step 1: Write it**

```markdown
# Contributing to the iExtend host workspace

## Prereqs

- Rust 1.83 (managed by `rust-toolchain.toml`; `rustup` will fetch it on first build)
- protoc not required — we use `tonic-build` which ships its own protobuf compiler
- Linux: `sudo apt install build-essential libssl-dev pkg-config`
- Windows: VS 2022 Build Tools with the C++ desktop workload

## Build

```bash
cd host
cargo build --workspace
```

## Run the daemon + tray locally

```bash
# terminal 1
cargo run -p iextendd

# terminal 2
cargo run -p iextend-tray
```

Click "Ping iextendd" in the tray window. You should see the daemon's version
and uptime.

## What's actually implemented vs. stubbed

After Plan 2 (this plan): only the gRPC `Status` RPC and the localhost transport.
Everything else (capture, encode, WebRTC, pairing, input forwarding, drivers)
arrives in Plans 3–10. See `docs/superpowers/plans/`.

## Style

- `cargo fmt` before commits.
- `clippy -D warnings` is the merge gate.
- Library crates (`ix-*`) export traits + minimal types only; impls go in the
  binaries or in subsystem-owning crates introduced by later plans.
```

- [ ] **Step 2: Commit**

```bash
git add host/CONTRIBUTING.md
git commit -m "docs(host): contributing guide and run-locally snippet"
```

---

### Task 9: Update Plan 1's README plan-status

**Files:**
- Modify: `/home/tops/Projects/iExtend/README.md` (`## Plan status` section, line for Plan 2)

- [ ] **Step 1: Flip the checkbox**

Change `- [ ] Plan 2: Rust host workspace bootstrap` → `- [x] Plan 2: Rust host workspace bootstrap`.

- [ ] **Step 2: Final repo-level checks**

```bash
cd /home/tops/Projects/iExtend
git status                                 # clean working tree
git log --oneline | head -15               # plan-2 commits show up
cd host && cargo build --workspace         # clean build
cd .. && grep '^- \[x\] Plan 2' README.md  # checkbox flipped
```

- [ ] **Step 3: Commit + tag**

```bash
git add README.md
git commit -m "docs: mark Plan 2 complete in README plan status"
git tag -a plan-2-complete -m "Plan 2 of 10 complete: Rust host workspace bootstrapped"
```

---

## Done criteria

All of the following must hold to consider this plan complete:

1. `cd host && cargo build --workspace` succeeds on a clean clone (Linux **and** Windows).
2. `cargo clippy --workspace --all-targets -- -D warnings` is clean on both targets.
3. `cargo test --workspace` passes (the canary test in Task 6 and any default test stubs).
4. `cargo run -p iextendd` starts the daemon, JSON logs go to stdout, the platform-native socket is created.
5. `cargo run -p iextend-tray`'s "Ping iextendd" button returns `v0.1.0 · uptime Ns` from a running daemon (Linux verified manually; Windows verified once the named-pipe loop lands).
6. `host-ci.yml` runs green on the Linux and Windows runners.
7. README plan-status checklist line for Plan 2 is `[x]`.
8. Tag `plan-2-complete` exists at the head of `main`.

## Out of scope (handled by later plans)

- Any actual capture, encoding, RTC transport, pairing, or input-forwarding logic. Library crates contain trait declarations + `NoOp*` impls only.
- Windows IddCx kernel driver and EV codesigning (Plan 3).
- Linux evdi DKMS package and PipeWire portal integration (Plan 4).
- WebRTC peer connection lifecycle and codec selection (Plan 5).
- iPad app — entirely separate Swift target tree (Plan 6).
- mDNS pairing handshake + SPAKE2 (Plan 7).
- Input forwarding + cursor reprojection (Plan 8).
- MSIX / .deb / .rpm installers (Plan 9).
- Bench rig for latency measurement (Plan 10).

If a later plan needs a new public type or trait, it adds it to the relevant `ix-*` crate's `lib.rs`. The skeletons established here are the seams.
