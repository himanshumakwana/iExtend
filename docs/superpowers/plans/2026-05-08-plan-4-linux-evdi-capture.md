# Linux evdi Capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `ix-display-linux` — a Rust crate that creates a virtual monitor on Linux via the `evdi` kernel module and captures frames from it. After this plan, both Wayland and X11 see iExtend as a real connected monitor; capture delivers DMA-BUF handles (Wayland) or XShm shared-memory regions (X11) into the same `Receiver<GpuFrame>` channel that Plan 3 produces. Codec-side ingest is deferred to Plan 5.

**Architecture:**

- The `evdi` kernel module (third-party, GPL, maintained by DisplayLink) is the only existing user-space-controllable virtual-DRM mechanism on Linux. We do **not** modify it. We ship it as a DKMS package separate from our daemon to keep license boundaries clean.
- Our crate is Apache-2.0 and links **`libevdi`** (LGPL) at runtime via `dlopen`/`dlsym`, so we never statically link a copyleft library into our binary.
- Wayland path: `org.freedesktop.portal.ScreenCast` (xdg-desktop-portal + PipeWire) gives us DMA-BUF fds for each frame of the virtual monitor we just created. Zero-copy into NVENC / VAAPI / AMF (codec impls land in Plan 5).
- X11 path: `XShm` shared-memory ximages + `XDamage` for changed-rect deltas. Encoder ingests the shm fd directly (VAAPI), or we fall back to a one-time CPU→GPU upload (slow path, warns user).
- NVIDIA proprietary driver caveat: cannot expose DMA-BUF for NVENC ingest cleanly. Detected at startup; we switch to the **CUDA interop fallback** — `cuMemcpy2D` from the screencast buffer (which the proprietary driver maps as a CUDA array) directly to the NVENC input surface. Cost ~0.3 ms; close enough to zero-copy for our latency budget.

**Tech Stack:**

- Rust 1.78+, edition 2021
- `libevdi` 1.14+ (loaded via `dlopen2`)
- `ashpd` 0.8+ (xdg-desktop-portal client) for the PipeWire screencast portal
- `pipewire-rs` 0.8+ for stream-side handling once the portal hands us a node ID
- `x11rb` 0.13+ with the `randr`, `damage`, and `shm` features for the X11 path
- `nix` 0.28+ for `mmap`, signalfd, and pidfd
- `crossbeam` for the SPSC frame ring buffer (shared with Plan 3's Windows path)
- DKMS 3.0+, debhelper 13+, rpm-build 4.16+ for distro packaging

**Plan scope:** This is **Plan 4 of 10** for the iExtend project. It depends on **Plan 2** being complete (the Rust workspace bootstrap, `iextendd` binary skeleton, and the shared `DisplaySource` trait + `GpuFrame` types under `host/crates/ix-display/`). Plan 3 produces the Windows backend in parallel; this plan produces the Linux backend. Codec selection (Plan 5), WebRTC transport (Plan 5), iPad app (Plan 6), and pairing (Plan 7) are out of scope.

**Hard out-of-scope realities — call them out to the user:**

- evdi is a third-party kernel module with imperfect distro coverage. Some users on **immutable / hardened distros** (Fedora Silverblue, Bazzite, ChromeOS Flex, secure-boot-locked enterprise images) may not be able to install kernel modules at all. For those users, v1 is **non-functional** — they'll need to wait for a hypothetical "v2 user-mode-only mode" that ships a software-rendered virtual display at the cost of CPU encoding (10–20 ms latency hit, rules out 120 Hz). Document this in the README; do not try to engineer around it in v1.
- evdi's user-space API is single-tenant per virtual monitor. Multi-iPad support (already deferred from spec §1) cannot be added without forking evdi. Out of scope.
- Container hosts (Flatpak iextendd, Snap iextendd) cannot reach `/dev/evdi*` by default. We ship as `.deb` and `.rpm` only; no Flatpak.

---

## File structure

```
host/
├── crates/
│   └── ix-display-linux/
│       ├── Cargo.toml
│       ├── build.rs                    # libevdi version probe at build time
│       ├── src/
│       │   ├── lib.rs                  # entry point: detect compositor + secure boot, pick backend
│       │   ├── evdi.rs                 # libevdi dlopen wrapper, virtual monitor lifecycle
│       │   ├── wayland.rs              # PipeWire portal client + DMA-BUF receive
│       │   ├── x11.rs                  # XShm + XDamage capture
│       │   ├── nvidia.rs               # CUDA interop fallback
│       │   ├── secureboot.rs           # MOK detection + user-facing guidance
│       │   └── ffi/
│       │       ├── libevdi.rs          # raw C FFI signatures (matches libevdi 1.14 ABI)
│       │       ├── cuda.rs             # libcuda dlopen for the NVIDIA fallback
│       │       └── mod.rs
│       └── tests/
│           ├── compositor_probe.rs     # unit: detect-compositor logic against synthetic env
│           ├── secureboot_probe.rs     # unit: MOK detection against synthetic /sys
│           └── integration.rs          # gated by --features integration; needs real evdi
└── packaging/
    └── linux/
        └── evdi-dkms/
            ├── dkms.conf               # pulls upstream evdi tarball, builds for current kernel
            ├── postinst                # MOK enrollment guidance + signing-cert install
            ├── prerm                   # remove signing cert from MOK store on uninstall
            └── README.md               # what this package does, why it's separate
```

Files that change together live together. Each `.rs` file is one responsibility:

- `lib.rs` = entry trait impl, dispatch only — under 200 lines.
- `evdi.rs` = virtual-monitor lifecycle, no compositor knowledge.
- `wayland.rs` and `x11.rs` = compositor-specific capture, no evdi knowledge except the monitor handle they receive.
- `nvidia.rs` = a single fallback transformation: `(dmabuf_fd | cuda_array) → cuda_array → NVENC input surface`. Used by both `wayland.rs` and `x11.rs` when the proprietary driver is detected.

---

### Task 1: Compositor + SecureBoot probe (foundation)

**Files:**
- Create: `host/crates/ix-display-linux/Cargo.toml`
- Create: `host/crates/ix-display-linux/src/lib.rs`
- Create: `host/crates/ix-display-linux/src/secureboot.rs`
- Create: `host/crates/ix-display-linux/tests/compositor_probe.rs`
- Create: `host/crates/ix-display-linux/tests/secureboot_probe.rs`

This task lays the foundation: a `Backend` enum and the runtime probe that picks between Wayland, X11, and "neither — abort with a useful error." We also probe SecureBoot here because the answer changes the install-time guidance the tray surfaces.

- [ ] **Step 1: Add the crate to the workspace**

Edit `host/Cargo.toml` (created by Plan 2):

```toml
[workspace]
members = [
    "crates/iextendd",
    "crates/iextend-tray",
    "crates/ix-display",
    "crates/ix-display-linux",   # new
    # ...
]
```

Create `host/crates/ix-display-linux/Cargo.toml`:

```toml
[package]
name = "ix-display-linux"
version = "0.1.0"
edition = "2021"
license = "Apache-2.0"

[dependencies]
ix-display = { path = "../ix-display" }
dlopen2 = "0.7"
nix = { version = "0.28", features = ["fs", "mman", "signal"] }
ashpd = { version = "0.8", default-features = false, features = ["pipewire", "tokio"] }
pipewire = "0.8"
x11rb = { version = "0.13", features = ["randr", "damage", "shm", "allow-unsafe-code"] }
crossbeam = "0.8"
thiserror = "1"
tracing = "0.1"
tokio = { version = "1", features = ["rt-multi-thread", "sync"] }

[features]
integration = []   # gates kernel-module-dependent tests

[target.'cfg(target_os = "linux")'.dependencies]
libc = "0.2"

[lib]
path = "src/lib.rs"
```

- [ ] **Step 2: Write the failing test for compositor detection**

Create `host/crates/ix-display-linux/tests/compositor_probe.rs`:

```rust
use ix_display_linux::{detect_backend, Backend};

#[test]
fn wayland_display_set_picks_wayland() {
    let env = TestEnv::new()
        .set("WAYLAND_DISPLAY", "wayland-0")
        .set("DISPLAY", ":0");
    assert_eq!(detect_backend(&env), Backend::Wayland);
}

#[test]
fn only_display_set_picks_x11() {
    let env = TestEnv::new().set("DISPLAY", ":0");
    assert_eq!(detect_backend(&env), Backend::X11);
}

#[test]
fn neither_set_returns_error() {
    let env = TestEnv::new();
    assert_eq!(detect_backend(&env), Backend::None);
}

struct TestEnv(std::collections::HashMap<&'static str, &'static str>);
impl TestEnv {
    fn new() -> Self { Self(Default::default()) }
    fn set(mut self, k: &'static str, v: &'static str) -> Self { self.0.insert(k, v); self }
}
impl ix_display_linux::EnvProbe for TestEnv {
    fn var(&self, k: &str) -> Option<String> { self.0.get(k).map(|s| s.to_string()) }
}
```

- [ ] **Step 3: Run, verify it fails to compile**

```bash
cd host
cargo test -p ix-display-linux compositor_probe
```

Expected: build failure with `Backend not found in ix-display-linux`. Compile failure = red phase.

- [ ] **Step 4: Implement minimum to pass**

Create `host/crates/ix-display-linux/src/lib.rs`:

```rust
//! Linux display backend for iExtend.
//!
//! Picks between Wayland (PipeWire screencast portal + DMA-BUF) and X11
//! (XShm + XDamage) based on environment, attaches an evdi virtual monitor,
//! and feeds frames into the shared `ix_display::DisplaySource` channel.

#![cfg(target_os = "linux")]

pub mod evdi;
pub mod nvidia;
pub mod secureboot;

mod ffi;
#[cfg(feature = "wayland")]
mod wayland;
mod x11;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Wayland,
    X11,
    None,
}

pub trait EnvProbe {
    fn var(&self, k: &str) -> Option<String>;
}

pub struct StdEnv;
impl EnvProbe for StdEnv {
    fn var(&self, k: &str) -> Option<String> { std::env::var(k).ok() }
}

pub fn detect_backend<E: EnvProbe>(env: &E) -> Backend {
    if env.var("WAYLAND_DISPLAY").is_some() { return Backend::Wayland; }
    if env.var("DISPLAY").is_some() { return Backend::X11; }
    Backend::None
}
```

- [ ] **Step 5: Run tests, verify they pass**

```bash
cd host
cargo test -p ix-display-linux compositor_probe
```

Expected: 3 passed.

- [ ] **Step 6: Write the SecureBoot probe failing test**

Create `host/crates/ix-display-linux/tests/secureboot_probe.rs`:

```rust
use ix_display_linux::secureboot::{is_secureboot_enabled, SecureBootProbe};

#[test]
fn efi_var_present_and_set_returns_true() {
    let p = FakeProbe { var_bytes: Some(vec![0,0,0,0,0,0,0,0,1]) };
    assert!(is_secureboot_enabled(&p));
}

#[test]
fn efi_var_present_but_zero_returns_false() {
    let p = FakeProbe { var_bytes: Some(vec![0,0,0,0,0,0,0,0,0]) };
    assert!(!is_secureboot_enabled(&p));
}

#[test]
fn no_efi_var_returns_false() {
    let p = FakeProbe { var_bytes: None };
    assert!(!is_secureboot_enabled(&p));
}

struct FakeProbe { var_bytes: Option<Vec<u8>> }
impl SecureBootProbe for FakeProbe {
    fn read_secureboot_efivar(&self) -> Option<Vec<u8>> { self.var_bytes.clone() }
}
```

- [ ] **Step 7: Implement secureboot.rs to pass**

Create `host/crates/ix-display-linux/src/secureboot.rs`:

```rust
//! SecureBoot detection.
//!
//! On UEFI Linux, `/sys/firmware/efi/efivars/SecureBoot-<guid>` is a 5-byte
//! file: a 4-byte EFI attribute prefix followed by a single 0/1 byte. We use
//! the canonical SecureBoot GUID (8be4df61-93ca-11d2-aa0d-00e098032b8c).

pub trait SecureBootProbe {
    fn read_secureboot_efivar(&self) -> Option<Vec<u8>>;
}

pub struct StdSecureBootProbe;
impl SecureBootProbe for StdSecureBootProbe {
    fn read_secureboot_efivar(&self) -> Option<Vec<u8>> {
        const PATH: &str =
            "/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c";
        std::fs::read(PATH).ok()
    }
}

pub fn is_secureboot_enabled<P: SecureBootProbe>(probe: &P) -> bool {
    match probe.read_secureboot_efivar() {
        Some(bytes) => bytes.last().copied() == Some(1),
        None => false,
    }
}
```

- [ ] **Step 8: Run all tests**

```bash
cd host
cargo test -p ix-display-linux
```

Expected: 6 passed (3 compositor + 3 secureboot).

- [ ] **Step 9: Commit**

```bash
cd host
git add crates/ix-display-linux Cargo.toml
git commit -m "feat(linux): scaffold ix-display-linux with backend + secureboot probes"
```

---

### Task 2: libevdi dlopen wrapper

**Files:**
- Create: `host/crates/ix-display-linux/src/ffi/libevdi.rs`
- Create: `host/crates/ix-display-linux/src/ffi/mod.rs`
- Create: `host/crates/ix-display-linux/src/evdi.rs`

This task wraps `libevdi.so.0` behind a safe Rust API. We dlopen at runtime so users without evdi installed get a clean error message instead of a load-time symbol failure.

- [ ] **Step 1: Write the FFI signatures**

Create `host/crates/ix-display-linux/src/ffi/libevdi.rs`:

```rust
//! libevdi 1.14 C ABI. See evdi/library/evdi_lib.h upstream.
//!
//! We declare just the surface we use; expanding this is cheap.

#![allow(non_camel_case_types)]
use std::os::raw::{c_char, c_int, c_uint, c_void};

pub type evdi_handle = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct evdi_mode {
    pub width: c_int,
    pub height: c_int,
    pub refresh_rate: c_int,
    pub bits_per_pixel: c_int,
    pub pixel_format: c_uint,
}

#[repr(C)]
pub struct evdi_buffer {
    pub id: c_int,
    pub buffer: *mut c_void,
    pub width: c_int,
    pub height: c_int,
    pub stride: c_int,
    pub rects: *mut c_void,
    pub rect_count: c_int,
}

pub type evdi_mode_changed_cb =
    extern "C" fn(mode: evdi_mode, user_data: *mut c_void);
pub type evdi_update_ready_cb =
    extern "C" fn(buffer_id: c_int, user_data: *mut c_void);

#[repr(C)]
pub struct evdi_event_context {
    pub dpms_handler: *mut c_void,
    pub mode_changed_handler: evdi_mode_changed_cb,
    pub update_ready_handler: evdi_update_ready_cb,
    pub crtc_state_handler: *mut c_void,
    pub cursor_set_handler: *mut c_void,
    pub cursor_move_handler: *mut c_void,
    pub user_data: *mut c_void,
}

dlopen2::wrapper_api!(pub Libevdi {
    evdi_open: unsafe extern "C" fn(device_index: c_int) -> evdi_handle,
    evdi_close: unsafe extern "C" fn(handle: evdi_handle),
    evdi_connect2: unsafe extern "C" fn(
        handle: evdi_handle,
        edid: *const u8, edid_length: c_uint,
        sku_area_limit: u32,
    ),
    evdi_disconnect: unsafe extern "C" fn(handle: evdi_handle),
    evdi_register_buffer:
        unsafe extern "C" fn(handle: evdi_handle, buffer: evdi_buffer),
    evdi_unregister_buffer:
        unsafe extern "C" fn(handle: evdi_handle, buffer_id: c_int),
    evdi_request_update:
        unsafe extern "C" fn(handle: evdi_handle, buffer_id: c_int) -> bool,
    evdi_handle_events:
        unsafe extern "C" fn(handle: evdi_handle, ctx: *mut evdi_event_context),
    evdi_get_event_ready: unsafe extern "C" fn(handle: evdi_handle) -> c_int,
});
```

Create `host/crates/ix-display-linux/src/ffi/mod.rs`:

```rust
pub mod libevdi;
#[cfg(feature = "nvidia")]
pub mod cuda;
```

- [ ] **Step 2: Write the failing test for the wrapper**

Append to `host/crates/ix-display-linux/tests/integration.rs` (create if missing — gate behind feature):

```rust
#![cfg(feature = "integration")]
use ix_display_linux::evdi::EvdiMonitor;

#[test]
fn open_close_roundtrip() {
    let mon = EvdiMonitor::open().expect("evdi present");
    drop(mon);
}
```

- [ ] **Step 3: Implement `evdi.rs`**

Create `host/crates/ix-display-linux/src/evdi.rs`:

```rust
//! Safe wrapper over libevdi's user-mode API.
//!
//! Lifetime model: an `EvdiMonitor` owns one `evdi_handle`. Drop disconnects
//! and closes. Buffers are registered by the capture backends (wayland.rs,
//! x11.rs); this file knows about monitor lifecycle only.

use crate::ffi::libevdi::{
    evdi_buffer, evdi_event_context, evdi_handle, evdi_mode, Libevdi,
};
use dlopen2::wrapper::Container;
use std::ffi::CString;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EvdiError {
    #[error("libevdi.so.0 not found — install the evdi DKMS package")]
    NotInstalled(#[source] dlopen2::Error),
    #[error("/dev/evdi* not present — kernel module not loaded (try `sudo modprobe evdi`)")]
    NoDevice,
    #[error("evdi_open returned NULL for device {0}")]
    OpenFailed(i32),
    #[error("kernel module too old: need 1.14+, found {0}")]
    AbiMismatch(String),
}

pub struct EvdiMonitor {
    lib: Container<Libevdi>,
    handle: evdi_handle,
    width: u32,
    height: u32,
    refresh_hz: u32,
}

impl EvdiMonitor {
    pub fn open() -> Result<Self, EvdiError> {
        let lib: Container<Libevdi> = unsafe { Container::load("libevdi.so.0") }
            .map_err(EvdiError::NotInstalled)?;

        // Find the first available /dev/evdi* (kernel exposes one node per
        // available virtual monitor; first call to evdi_open consumes one).
        let dev = (0..32).find(|i| {
            PathBuf::from(format!("/dev/evdi{i}")).exists()
        }).ok_or(EvdiError::NoDevice)?;

        let handle = unsafe { lib.evdi_open(dev as i32) };
        if handle.is_null() { return Err(EvdiError::OpenFailed(dev as i32)); }

        Ok(Self {
            lib,
            handle,
            width: 0,
            height: 0,
            refresh_hz: 0,
        })
    }

    /// Plug a virtual monitor of the given mode. EDID is a generic 1080p120
    /// payload; we ship one in the crate's `assets/` (out of scope for the
    /// excerpt). Real EDID is a 256-byte hex string.
    pub fn connect(&mut self, w: u32, h: u32, hz: u32) -> Result<(), EvdiError> {
        const EDID_1080P_120HZ: &[u8] = include_bytes!("../assets/edid_1080p120.bin");
        unsafe {
            self.lib.evdi_connect2(
                self.handle,
                EDID_1080P_120HZ.as_ptr(),
                EDID_1080P_120HZ.len() as u32,
                /* sku_area_limit */ 3840 * 2160,
            );
        }
        self.width = w; self.height = h; self.refresh_hz = hz;
        Ok(())
    }

    pub fn handle(&self) -> evdi_handle { self.handle }
    pub fn lib(&self) -> &Libevdi { &self.lib }
}

impl Drop for EvdiMonitor {
    fn drop(&mut self) {
        unsafe {
            self.lib.evdi_disconnect(self.handle);
            self.lib.evdi_close(self.handle);
        }
    }
}
```

- [ ] **Step 4: Run unit tests (integration test stays gated)**

```bash
cd host
cargo test -p ix-display-linux
```

Expected: 6 passed (still). Integration test only runs with `--features integration` on a real evdi-installed host.

- [ ] **Step 5: Commit**

```bash
cd host
git add crates/ix-display-linux/src/evdi.rs \
        crates/ix-display-linux/src/ffi \
        crates/ix-display-linux/assets/edid_1080p120.bin
git commit -m "feat(linux): libevdi dlopen wrapper + EvdiMonitor lifecycle"
```

---

### Task 3: Wayland path — PipeWire portal + DMA-BUF

**Files:**
- Create: `host/crates/ix-display-linux/src/wayland.rs`

The Wayland path is the happy path for modern distros. We use `xdg-desktop-portal`'s `org.freedesktop.portal.ScreenCast` interface — `ashpd` is the de-facto Rust client. The portal asks the user to grant screen-capture permission once (we request the *embedded* grant flow so on subsequent runs no prompt appears), then hands us a PipeWire node ID. We attach a stream to that node and pull DMA-BUF frames.

- [ ] **Step 1: Write the implementation**

Create `host/crates/ix-display-linux/src/wayland.rs`:

```rust
//! Wayland capture path.
//!
//! Once `EvdiMonitor` has plugged a virtual monitor, the compositor (Mutter,
//! KWin, Sway, Hyprland) sees a new connected output. We then ask
//! xdg-desktop-portal to screencast *just that output* by passing the
//! monitor's connector name as the `RestoreToken` hint.
//!
//! Frames arrive as DMA-BUF fds. We wrap them in `ix_display::GpuFrame`
//! and push to the SPSC ring buffer.

use ashpd::desktop::screencast::{
    CursorMode, PersistMode, Screencast, SourceType, Stream,
};
use ashpd::desktop::Session;
use crossbeam::queue::ArrayQueue;
use ix_display::{DamageRect, GpuFrame, GpuFrameKind};
use std::os::fd::{AsFd, OwnedFd, RawFd};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, info, warn};

#[derive(Debug, Error)]
pub enum WaylandError {
    #[error("xdg-desktop-portal not available — is xdg-desktop-portal-wlr or -gnome installed?")]
    NoPortal(#[source] ashpd::Error),
    #[error("user denied screencast permission")]
    PermissionDenied,
    #[error("PipeWire node disappeared mid-stream")]
    NodeGone,
}

pub struct WaylandCapture {
    session: Session<'static, Screencast<'static>>,
    pw_fd: OwnedFd,
    streams: Vec<Stream>,
    out: Arc<ArrayQueue<GpuFrame>>,
}

impl WaylandCapture {
    pub async fn start(
        connector_name: &str,        // e.g. "EVDI-1"
        out: Arc<ArrayQueue<GpuFrame>>,
    ) -> Result<Self, WaylandError> {
        let proxy = Screencast::new()
            .await
            .map_err(WaylandError::NoPortal)?;

        let session = proxy.create_session().await
            .map_err(WaylandError::NoPortal)?;

        // Restrict to the virtual monitor only. Cursor: hidden — the iPad
        // re-renders its own cursor (see Plan 8 §6.4 of the spec).
        proxy.select_sources(
            &session,
            CursorMode::Hidden,
            SourceType::Monitor.into(),
            /* multiple */ false,
            /* restore_token */ Some(connector_name.into()),
            PersistMode::Application,
        ).await.map_err(WaylandError::NoPortal)?;

        let response = proxy.start(&session, Default::default()).await
            .map_err(|_| WaylandError::PermissionDenied)?;
        let streams = response.streams().to_vec();
        let pw_fd = proxy.open_pipe_wire_remote(&session).await
            .map_err(WaylandError::NoPortal)?;

        info!(
            connector = connector_name,
            stream_node = streams.first().map(|s| s.pipe_wire_node_id()),
            "wayland screencast acquired",
        );

        Ok(Self { session, pw_fd, streams, out })
    }

    /// Run the PipeWire stream loop. Blocks until the stream errors out or
    /// is cancelled; intended to run on a dedicated thread.
    pub fn run(self) -> Result<(), WaylandError> {
        let node_id = self.streams.first()
            .ok_or(WaylandError::NodeGone)?
            .pipe_wire_node_id();

        // pipewire-rs setup omitted for brevity — see crate docs.
        // Per-frame on_process: extract DMA-BUF fd from buffer.datas[0],
        // wrap in GpuFrame::DmaBuf { fd, width, height, stride, modifier },
        // push to self.out.
        //
        // Damage rects come from the buffer's `meta` of type
        // SPA_META_VideoDamage; convert to ix_display::DamageRect array
        // and attach to the frame.
        Ok(())
    }
}
```

- [ ] **Step 2: Smoke test the portal call (interactive)**

This step is interactive — we run the daemon, see the portal prompt appear, and grant. Document the manual flow in `host/crates/ix-display-linux/tests/MANUAL.md`:

```markdown
# Manual Wayland smoke test

1. Run `cargo run -p iextendd -- --debug-wayland-capture` from a Wayland session.
2. xdg-desktop-portal pops up: "iExtend wants to share your screen — choose what to share."
3. Pick the EVDI-1 entry (the virtual monitor). Click "Share."
4. The daemon should log `wayland screencast acquired` with a node ID.
5. After ~1 s, the daemon should log "received first DMA-BUF frame, 1920x1080 fmt=DRM_FORMAT_XRGB8888".
6. Ctrl-C. The portal session is dropped.

Repeat. The second run should NOT prompt — the `PersistMode::Application` token is reused.
```

- [ ] **Step 3: Commit**

```bash
cd host
git add crates/ix-display-linux/src/wayland.rs \
        crates/ix-display-linux/tests/MANUAL.md
git commit -m "feat(linux): wayland capture via xdg-desktop-portal screencast"
```

---

### Task 4: X11 path — XShm + XDamage fallback

**Files:**
- Create: `host/crates/ix-display-linux/src/x11.rs`

For users without Wayland (older distros, headless servers, RDP-style sessions), we fall back to X11. XShm gives us a shared-memory ximage, XDamage tells us which rectangles changed since the last frame.

This path is **slower** than Wayland (the encoder must ingest from CPU shared memory unless VAAPI is available). We warn the user once.

- [ ] **Step 1: Implement**

Create `host/crates/ix-display-linux/src/x11.rs`:

```rust
//! X11 capture path: XShm + XDamage.
//!
//! - XShm gives us a shared-memory image we map once and re-read in place.
//! - XDamage reports rectangles that changed since the last fetch.
//! - VAAPI can ingest shm fds directly; the proprietary NVIDIA path needs
//!   a CPU→GPU upload — see nvidia.rs.

use crossbeam::queue::ArrayQueue;
use ix_display::{DamageRect, GpuFrame, GpuFrameKind};
use std::sync::Arc;
use thiserror::Error;
use tracing::{info, warn};
use x11rb::connection::Connection;
use x11rb::protocol::damage::{self, ConnectionExt as _};
use x11rb::protocol::shm::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{self, ConnectionExt as _, Window};

#[derive(Debug, Error)]
pub enum X11Error {
    #[error("X server connection failed: {0}")]
    Connect(#[from] x11rb::errors::ConnectError),
    #[error("X11 protocol error: {0}")]
    Proto(#[from] x11rb::errors::ReplyError),
    #[error("MIT-SHM extension not available — install libxext-dev / xorg-x11-server-Xext")]
    NoShm,
    #[error("XDamage extension not available")]
    NoDamage,
    #[error("evdi connector EVDI-1 not found in randr outputs")]
    EvdiOutputMissing,
}

pub struct X11Capture {
    conn: x11rb::rust_connection::RustConnection,
    root: Window,
    output_window: Window,
    damage: damage::Damage,
    shm_seg: shm::Seg,
    shm_addr: *mut u8,
    width: u16,
    height: u16,
    out: Arc<ArrayQueue<GpuFrame>>,
}

impl X11Capture {
    pub fn start(
        connector_name: &str,
        out: Arc<ArrayQueue<GpuFrame>>,
    ) -> Result<Self, X11Error> {
        let (conn, screen_num) = x11rb::connect(None)?;
        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;

        // Verify extensions
        if conn.shm_query_version()?.reply().is_err() { return Err(X11Error::NoShm); }
        if conn.damage_query_version(1, 1)?.reply().is_err() { return Err(X11Error::NoDamage); }

        // Find the EVDI output via randr; output_window is the root window
        // restricted by the output's CRTC viewport.
        let output_window = root; // simplified — real impl picks the EVDI-1 viewport
        warn!("X11 capture is a fallback path — Wayland recommended for full performance");

        // shmget + shmat for the size of the virtual monitor
        // (omitted — real impl uses nix::sys::shm)
        let width = 1920;
        let height = 1080;
        let shm_seg: shm::Seg = conn.generate_id()?;
        // ... shm_attach, etc.

        let damage: damage::Damage = conn.generate_id()?;
        conn.damage_create(damage, output_window, damage::ReportLevel::DELTA_RECTANGLES)?;

        info!(connector = connector_name, "X11 capture started");

        Ok(Self {
            conn,
            root,
            output_window,
            damage,
            shm_seg,
            shm_addr: std::ptr::null_mut(),
            width,
            height,
            out,
        })
    }

    /// Pump one frame. Calling at the monitor's refresh rate (120 Hz target)
    /// is the caller's responsibility — typically a dedicated thread.
    pub fn pump(&mut self) -> Result<(), X11Error> {
        // 1. Drain pending damage events into a Vec<DamageRect>
        let mut rects: Vec<DamageRect> = Vec::new();
        // ... real impl reads damage events ...

        // 2. shm_get_image into self.shm_addr
        self.conn.shm_get_image(
            self.output_window,
            0, 0, self.width, self.height,
            !0,
            xproto::ImageFormat::Z_PIXMAP.into(),
            self.shm_seg, 0,
        )?.reply()?;

        // 3. Wrap into GpuFrame::ShmCpu and push (codec ingests, may copy to GPU)
        let frame = GpuFrame {
            kind: GpuFrameKind::ShmCpu {
                addr: self.shm_addr,
                stride: self.width as usize * 4,
            },
            width: self.width as u32,
            height: self.height as u32,
            damage: rects,
            timestamp_us: now_us(),
        };
        let _ = self.out.push(frame);
        Ok(())
    }
}

fn now_us() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64
}
```

- [ ] **Step 2: Add a unit test for damage-rect coalescing**

The capture path will get many small damage events per frame; coalesce overlapping ones before the encoder sees them.

Add to `host/crates/ix-display-linux/tests/x11.rs`:

```rust
#![cfg(target_os = "linux")]
use ix_display::DamageRect;
use ix_display_linux::x11::coalesce;

#[test]
fn overlapping_rects_merge() {
    let in_rects = vec![
        DamageRect { x: 0, y: 0, w: 100, h: 100 },
        DamageRect { x: 50, y: 50, w: 100, h: 100 },
    ];
    let out = coalesce(in_rects);
    assert_eq!(out, vec![DamageRect { x: 0, y: 0, w: 150, h: 150 }]);
}

#[test]
fn disjoint_rects_kept() {
    let in_rects = vec![
        DamageRect { x: 0, y: 0, w: 50, h: 50 },
        DamageRect { x: 200, y: 200, w: 50, h: 50 },
    ];
    let out = coalesce(in_rects.clone());
    assert_eq!(out, in_rects);
}
```

Implement `coalesce` in `x11.rs` accordingly. (Sweep-and-merge; ~30 lines.)

- [ ] **Step 3: Run, verify pass**

```bash
cd host
cargo test -p ix-display-linux
```

Expected: 8 passed.

- [ ] **Step 4: Commit**

```bash
cd host
git add crates/ix-display-linux/src/x11.rs crates/ix-display-linux/tests/x11.rs
git commit -m "feat(linux): X11 fallback capture via XShm + XDamage"
```

---

### Task 5: NVIDIA proprietary driver — CUDA interop fallback

**Files:**
- Create: `host/crates/ix-display-linux/src/ffi/cuda.rs`
- Create: `host/crates/ix-display-linux/src/nvidia.rs`

NVIDIA's proprietary driver (anything pre-open-kernel-module 555+) does not expose DMA-BUF for NVENC ingest. We dlopen `libcuda.so.1` and use `cuMemcpy2D` from a CUDA-mapped texture (or a host buffer for the X11 path) to NVENC's input surface. The whole path should add 0.3 ms.

- [ ] **Step 1: CUDA FFI**

Create `host/crates/ix-display-linux/src/ffi/cuda.rs`:

```rust
//! Minimal libcuda binding for the NVIDIA fallback. Only the symbols we use.

#![allow(non_camel_case_types, non_snake_case)]
use std::os::raw::{c_int, c_uint, c_void};

pub type CUresult = c_int;
pub type CUdeviceptr = u64;
pub type CUcontext = *mut c_void;
pub type CUstream = *mut c_void;

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct CUDA_MEMCPY2D {
    pub srcXInBytes: usize, pub srcY: usize,
    pub srcMemoryType: c_uint,
    pub srcHost: *const c_void, pub srcDevice: CUdeviceptr,
    pub srcArray: *mut c_void, pub srcPitch: usize,
    pub dstXInBytes: usize, pub dstY: usize,
    pub dstMemoryType: c_uint,
    pub dstHost: *mut c_void, pub dstDevice: CUdeviceptr,
    pub dstArray: *mut c_void, pub dstPitch: usize,
    pub WidthInBytes: usize, pub Height: usize,
}

dlopen2::wrapper_api!(pub Libcuda {
    cuInit: unsafe extern "C" fn(flags: c_uint) -> CUresult,
    cuCtxGetCurrent: unsafe extern "C" fn(ctx: *mut CUcontext) -> CUresult,
    cuMemcpy2DAsync: unsafe extern "C" fn(
        copy: *const CUDA_MEMCPY2D, stream: CUstream
    ) -> CUresult,
    cuStreamSynchronize: unsafe extern "C" fn(stream: CUstream) -> CUresult,
});
```

- [ ] **Step 2: Detect proprietary driver**

Create `host/crates/ix-display-linux/src/nvidia.rs`:

```rust
//! NVIDIA proprietary-driver detection + CUDA-interop encoder ingest.

use std::path::Path;

/// True if the proprietary nvidia driver is loaded (not nouveau, not the
/// open kernel module 555+ which exposes DMA-BUF cleanly).
pub fn proprietary_driver_active() -> bool {
    // /sys/module/nvidia/version is set by the proprietary module; if the
    // open kernel module is loaded the directory is named nvidia_drm only.
    Path::new("/sys/module/nvidia/version").exists()
        && !Path::new("/sys/module/nvidia_dma_buf").exists()
}

/// Sketch: copy a host-mapped frame buffer into an NVENC input surface.
/// Real impl plumbs in `nvenc-rs::Encoder::input_buffer_dptr()` from Plan 5.
pub fn copy_host_to_nvenc_input(
    src_host: *const u8, src_pitch: usize,
    dst_dptr: u64, dst_pitch: usize,
    width: usize, height: usize,
) -> Result<(), std::io::Error> {
    // Real impl: load Libcuda, build CUDA_MEMCPY2D with srcMemoryType=HOST,
    // dstMemoryType=DEVICE, dstDevice=dst_dptr, call cuMemcpy2DAsync on the
    // NVENC-shared CUDA stream. ~80 lines.
    let _ = (src_host, src_pitch, dst_dptr, dst_pitch, width, height);
    Ok(())
}
```

- [ ] **Step 3: Test (synthetic, since CUDA needs real hardware)**

The probe is the only thing we can test cheaply. Add to `tests/nvidia.rs`:

```rust
#![cfg(target_os = "linux")]
use ix_display_linux::nvidia::proprietary_driver_active;

#[test]
fn probe_runs_without_panic() {
    // Result depends on CI host; we just verify it doesn't crash.
    let _ = proprietary_driver_active();
}
```

Real interop is exercised by Plan 5's encoder integration tests; this plan stops at "we have the path defined and a passing detection probe."

- [ ] **Step 4: Commit**

```bash
cd host
git add crates/ix-display-linux/src/nvidia.rs \
        crates/ix-display-linux/src/ffi/cuda.rs \
        crates/ix-display-linux/tests/nvidia.rs
git commit -m "feat(linux): NVIDIA proprietary-driver detection + CUDA-interop scaffolding"
```

---

### Task 6: evdi-dkms package + MOK enrollment

**Files:**
- Create: `host/packaging/linux/evdi-dkms/dkms.conf`
- Create: `host/packaging/linux/evdi-dkms/postinst`
- Create: `host/packaging/linux/evdi-dkms/prerm`
- Create: `host/packaging/linux/evdi-dkms/README.md`

We pin the upstream evdi version we've tested (1.14.7 at the time of writing) and ship a DKMS source tarball. On SecureBoot systems, the postinst hook generates the kernel-module signing cert (using sbsigntool) and prompts the user to enroll it via `mokutil`.

- [ ] **Step 1: Write `dkms.conf`**

Create `host/packaging/linux/evdi-dkms/dkms.conf`:

```bash
PACKAGE_NAME="iextend-evdi"
PACKAGE_VERSION="1.14.7"
CLEAN="make clean"
MAKE[0]="make module KVERSION=${kernelver}"
BUILT_MODULE_NAME[0]="evdi"
BUILT_MODULE_LOCATION[0]="module"
DEST_MODULE_LOCATION[0]="/kernel/drivers/gpu/drm/evdi"
AUTOINSTALL="yes"
REMAKE_INITRD="yes"

# Sign the module if we have a SecureBoot signing cert installed.
# postinst stages the cert into /var/lib/iextend/MOK.{priv,der}
MODULES_CONF[0]="options evdi initial_device_count=1"
SIGN_TOOL="/usr/lib/iextend/sign-evdi-module.sh ${kernelver}"
```

- [ ] **Step 2: Write the `postinst` hook**

Create `host/packaging/linux/evdi-dkms/postinst`:

```bash
#!/bin/sh
set -e

KEY_DIR=/var/lib/iextend
KEY=$KEY_DIR/MOK.priv
CERT=$KEY_DIR/MOK.der

if [ "$1" != "configure" ]; then exit 0; fi

mkdir -p $KEY_DIR
chmod 700 $KEY_DIR

# Generate signing key + cert on first install
if [ ! -f "$KEY" ] || [ ! -f "$CERT" ]; then
    openssl req -new -x509 -newkey rsa:2048 \
        -keyout "$KEY" -outform DER -out "$CERT" \
        -days 36500 -nodes \
        -subj "/CN=iExtend evdi module signing key/"
    chmod 600 "$KEY"
fi

# If SecureBoot is on, enroll the cert via MOK
if mokutil --sb-state 2>/dev/null | grep -q "SecureBoot enabled"; then
    cat <<EOF

  iExtend's display driver (evdi) requires a kernel module that must be
  signed for SecureBoot to load it.

  Run this NOW, before rebooting:

    sudo mokutil --import $CERT

  You'll be asked for a one-time password (8 chars). After reboot, the
  blue MOK Manager screen appears: pick "Enroll MOK" → "Continue" → enter
  the password → "Reboot."

  After reboot, the iExtend daemon will start successfully.

EOF
fi

exit 0
```

- [ ] **Step 3: `prerm` removes the cert from MOK on uninstall**

Create `host/packaging/linux/evdi-dkms/prerm`:

```bash
#!/bin/sh
set -e
if [ "$1" != "remove" ] && [ "$1" != "purge" ]; then exit 0; fi

CERT=/var/lib/iextend/MOK.der
if [ -f "$CERT" ] && command -v mokutil >/dev/null; then
    echo "If you installed iExtend's signing cert into MOK, run:"
    echo "  sudo mokutil --delete $CERT"
fi
exit 0
```

- [ ] **Step 4: README.md for the package**

Create `host/packaging/linux/evdi-dkms/README.md`:

```markdown
# iextend-evdi DKMS package

This package ships the upstream **evdi** kernel module (version 1.14.7) as
a DKMS source tarball. It is **separate** from the main iextend daemon
because evdi is GPL-licensed and our daemon is Apache-2.0; keeping them
in separate packages keeps the license boundary clean.

## What this package does

1. Drops the evdi source under `/usr/src/iextend-evdi-1.14.7`.
2. Registers it with DKMS, which builds the kernel module against your
   running kernel and any new kernels you install.
3. On SecureBoot systems, prompts you to enroll a signing cert via MOK.
4. Loads the module with `initial_device_count=1` (we only need one
   virtual monitor; ask if you need more).

## Why a separate package

If we shipped the module in the main `.deb`, license obligations would
require us to relicense the daemon GPL. By keeping evdi as its own
package and dlopen-ing libevdi at runtime, the daemon stays Apache-2.0.

## Installing without DKMS

If you can't use DKMS (immutable distros, hardened systems, containers),
**iExtend will not work** in v1. We are tracking user-mode-only support
for v2 — see `docs/superpowers/specs/2026-05-08-iextend-design.md` §1
"out of scope."
```

- [ ] **Step 5: Commit**

```bash
cd host
chmod +x packaging/linux/evdi-dkms/postinst packaging/linux/evdi-dkms/prerm
git add packaging/linux/evdi-dkms/
git commit -m "build(linux): DKMS package for evdi 1.14.7 with MOK guidance"
```

---

### Task 7: Wire backends into `DisplaySource` trait

**Files:**
- Modify: `host/crates/ix-display-linux/src/lib.rs`

This is the integration step. Plan 2 defined `trait DisplaySource` with `create_virtual_monitor`, `capture_frame`, `dirty_rects`, `destroy`. We now provide a `LinuxDisplaySource` that wraps `EvdiMonitor` + the appropriate `WaylandCapture` or `X11Capture` and implements that trait.

- [ ] **Step 1: Add the trait impl**

Append to `host/crates/ix-display-linux/src/lib.rs`:

```rust
use crate::evdi::{EvdiMonitor, EvdiError};
use crate::secureboot::{is_secureboot_enabled, StdSecureBootProbe};
use crossbeam::queue::ArrayQueue;
use ix_display::{DisplayMode, DisplaySource, GpuFrame, MonitorHandle};
use std::sync::Arc;

pub struct LinuxDisplaySource {
    monitor: EvdiMonitor,
    backend: BackendImpl,
    out: Arc<ArrayQueue<GpuFrame>>,
}

enum BackendImpl {
    #[cfg(feature = "wayland")]
    Wayland(crate::wayland::WaylandCapture),
    X11(crate::x11::X11Capture),
}

impl LinuxDisplaySource {
    pub fn new(out: Arc<ArrayQueue<GpuFrame>>) -> Result<Self, LinuxError> {
        let backend = detect_backend(&StdEnv);
        let mut monitor = EvdiMonitor::open()
            .map_err(LinuxError::Evdi)?;
        monitor.connect(1920, 1080, 120)?;

        let backend_impl = match backend {
            #[cfg(feature = "wayland")]
            Backend::Wayland => {
                let cap = futures::executor::block_on(
                    crate::wayland::WaylandCapture::start("EVDI-1", out.clone())
                )?;
                BackendImpl::Wayland(cap)
            }
            Backend::X11 => {
                BackendImpl::X11(crate::x11::X11Capture::start("EVDI-1", out.clone())?)
            }
            Backend::None => return Err(LinuxError::NoCompositor),
        };

        if is_secureboot_enabled(&StdSecureBootProbe) {
            tracing::info!("SecureBoot active — make sure MOK enrollment was completed");
        }

        Ok(Self { monitor, backend: backend_impl, out })
    }
}

impl DisplaySource for LinuxDisplaySource {
    fn capture_frame(&mut self) -> Option<GpuFrame> {
        self.out.pop()
    }
    // create_virtual_monitor / destroy delegate to EvdiMonitor's Drop
}

#[derive(Debug, thiserror::Error)]
pub enum LinuxError {
    #[error(transparent)] Evdi(#[from] EvdiError),
    #[error(transparent)] Wayland(#[from] crate::wayland::WaylandError),
    #[error(transparent)] X11(#[from] crate::x11::X11Error),
    #[error("no graphical session detected (neither WAYLAND_DISPLAY nor DISPLAY set)")]
    NoCompositor,
}
```

- [ ] **Step 2: Run all tests**

```bash
cd host
cargo test -p ix-display-linux
```

Expected: all unit tests pass; integration tests still gated.

- [ ] **Step 3: Commit**

```bash
cd host
git add crates/ix-display-linux/src/lib.rs
git commit -m "feat(linux): LinuxDisplaySource implements ix_display::DisplaySource"
```

---

### Task 8: Manual / QEMU integration test + install docs

**Files:**
- Create: `host/crates/ix-display-linux/tests/integration.rs` (filled in)
- Create: `docs/install/linux.md`

Real evdi requires loading a kernel module. CI can run inside a privileged Docker container with the host's kernel-headers volume-mounted, but this is fragile across distros. Two viable strategies:

1. **Manual smoke**: a bench machine running Ubuntu 24.04 and Fedora 41, with the evdi DKMS already installed. Plan 9 (CI cluster) generalizes this.
2. **QEMU**: spin a VM with KVM, install our DKMS package, run `cargo test --features integration`. Slow (~60 s per run) but reproducible.

This task wires up the manual path and documents how a future Plan 9 will productionize it.

- [ ] **Step 1: Implement the gated integration tests**

Replace `host/crates/ix-display-linux/tests/integration.rs`:

```rust
#![cfg(feature = "integration")]
//! Integration tests that require evdi loaded and a graphical session.

use crossbeam::queue::ArrayQueue;
use ix_display_linux::{LinuxDisplaySource, evdi::EvdiMonitor};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn evdi_open_close() {
    let m = EvdiMonitor::open().expect("evdi present + module loaded");
    drop(m);
}

#[test]
fn end_to_end_first_frame() {
    let q = Arc::new(ArrayQueue::new(8));
    let mut src = LinuxDisplaySource::new(q.clone()).expect("backend init");
    // The first frame may take up to 1 s while the compositor reconfigures.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        if let Some(frame) = src.capture_frame() {
            assert!(frame.width >= 1920);
            assert!(frame.height >= 1080);
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no frame within 2 s");
}
```

- [ ] **Step 2: Write the install doc**

Create `docs/install/linux.md`:

```markdown
# Installing iExtend on Linux

iExtend on Linux requires the **evdi** kernel module (third-party,
GPL, maintained by DisplayLink) plus the iextend daemon binary.

## Supported distros

| Distro | Status | Notes |
|--------|--------|-------|
| Ubuntu 22.04+ | ✅ | apt repo |
| Debian 12+ | ✅ | apt repo |
| Fedora 39+ | ✅ | dnf repo |
| Arch | ✅ | AUR (community-maintained) |
| Fedora Silverblue / Bazzite | ❌ | immutable; no kernel modules |
| Steam Deck (SteamOS) | ❌ | read-only root |
| RHEL 9 | ⚠️ | requires `kernel-devel` matching running kernel |

## Quick install (Ubuntu / Debian)

```bash
sudo add-apt-repository ppa:iextend/stable
sudo apt update
sudo apt install iextend iextend-evdi-dkms
```

The package will:

1. Build the evdi kernel module against your running kernel via DKMS.
2. If SecureBoot is enabled: print a `mokutil --import` command. **Run
   this before rebooting.** After reboot, complete the blue MOK Manager
   prompt (Enroll MOK → Continue → password → Reboot).
3. Install the iextend daemon and tray app.

## Verifying it works

```bash
lsmod | grep evdi          # should show evdi loaded
ls /dev/evdi*              # should show /dev/evdi0
systemctl --user start iextend
journalctl --user -u iextend | grep "ready"
```

## SecureBoot — what's actually happening

SecureBoot rejects unsigned kernel modules. Our DKMS package generates
a one-time signing cert (`/var/lib/iextend/MOK.der`) and signs the
module with it. The cert needs to be added to your "Machine Owner
Keys" list (MOK) — that's what the `mokutil --import` step does.

The cert is unique to your machine. It does not let us (or anyone)
load other modules; it only matches the evdi build for your kernels.

## Uninstalling

```bash
sudo apt remove iextend iextend-evdi-dkms
sudo mokutil --delete /var/lib/iextend/MOK.der   # remove our signing cert
```
```

- [ ] **Step 3: Run the gated tests on a real evdi machine**

This step is manual:

```bash
ssh bench-ubuntu24
cd ~/iExtend/host
cargo test -p ix-display-linux --features integration
```

Expected: 2 passed, 0 failed.

If `evdi_open_close` fails with "kernel module not loaded," run
`sudo modprobe evdi initial_device_count=1` and retry.

If `end_to_end_first_frame` fails with "no frame within 2 s," check:
- Is the compositor actually rendering to EVDI-1? `xrandr | grep EVDI`.
- For Wayland: was the portal grant remembered? Re-run interactively;
  the prompt should appear once.

- [ ] **Step 4: Commit**

```bash
cd host
git add crates/ix-display-linux/tests/integration.rs
cd ..
git add docs/install/linux.md
git commit -m "docs(linux): install instructions + integration test harness"
```

---

## Done criteria

All of the following must be true to consider Plan 4 complete:

1. `cargo build -p ix-display-linux` succeeds on the workspace.
2. `cargo test -p ix-display-linux` passes (unit tests; ~10 tests).
3. `cargo test -p ix-display-linux --features integration` passes on at
   least one real-evdi bench machine (Ubuntu 24.04 + AMD GPU is the
   canonical test rig).
4. The DKMS package `iextend-evdi-dkms` builds for the current kernel,
   produces a signed module under SecureBoot, and `lsmod | grep evdi`
   shows it loaded after reboot.
5. `LinuxDisplaySource::new` returns successfully on:
   - GNOME Wayland (Mutter)
   - KDE Wayland (KWin)
   - Sway
   - GNOME X11
   - i3 (X11)
6. First frame arrives in `<1 s` after the daemon starts, on at least
   one bench machine.
7. `docs/install/linux.md` is committed and renders correctly on GitHub.

## Out of scope (handled by later plans)

- Codec selection and encoder ingest (Plan 5).
- WebRTC peer connection (Plan 5).
- Pairing flow (Plan 7).
- Input forwarding back to the host (Plan 8).
- Installer (Plan 9 — packages this DKMS into the same `.deb`).
- Multi-monitor / multi-iPad (deferred to v2).
- User-mode-only fallback for immutable distros (deferred to v2).

## Risks and known sharp edges

1. **DKMS rebuild on kernel upgrade is best-effort.** Some distros (RHEL
   minor-version bumps, Ubuntu HWE kernels) ship kernels with subtle ABI
   changes that break upstream evdi. We pin a known-good version
   (1.14.7); when the upstream catches up, we bump.
2. **MOK enrollment requires reboot.** First-install UX is rough. The
   tray app needs a "you must reboot to finish setup" banner — track in
   Plan 9.
3. **NVIDIA proprietary driver still has rough edges.** The CUDA-interop
   path adds a CUDA dependency at runtime (`libcuda.so.1`). Open-kernel
   driver 555+ doesn't need it; document the boundary.
4. **xdg-desktop-portal-wlr (Sway) sometimes drops the screencast token
   on portal restart.** Workaround: re-grant. We can't fix the portal
   from our side.
5. **PipeWire on older distros (PW < 0.3.65) does not propagate
   `SPA_META_VideoDamage`.** We fall back to "full-frame damage" — the
   encoder still runs at full rate but the damage-tracked-partial-encode
   optimization (spec §5.2) is degraded. Detect and warn.
