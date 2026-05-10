# USB Pair Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add USB-cable pairing to iExtend, alongside the existing Wi-Fi pair flow. Reuses simple-pair-v0 wire protocol over a libimobiledevice/usbmuxd-tunneled TCP connection.

**Architecture:** New crate `ix-usb` wraps libimobiledevice; new module `iextendd::usb_listener` consumes USB plug events and routes incoming connections through the existing `handle_simple_one` handler. iPad-side adds a loopback `NWListener` on `127.0.0.1:7780`. Tray shows a "USB connected" chip when an iPad is plugged in.

**Tech Stack:** Rust (host), libimobiledevice (C library), Swift/Network.framework (iPad), tonic/protobuf (gRPC), egui (tray).

**Spec:** `docs/superpowers/specs/2026-05-10-usb-pair-design.md`

---

## Task 1: `ix-usb` crate skeleton + binding choice

**Files:**
- Create: `host/crates/ix-usb/Cargo.toml`
- Create: `host/crates/ix-usb/src/lib.rs`
- Modify: `host/Cargo.toml` (add to `members`)

- [ ] **Step 1:** Try `rusty_libimobiledevice` first. Add to `host/Cargo.toml` workspace deps and use in `ix-usb/Cargo.toml`. Run `cargo check -p ix-usb` on Linux.
- [ ] **Step 2:** If it builds: keep it. If it errors (linkage/missing symbols/abandoned crate), fall back to direct FFI via `bindgen` + `libimobiledevice-sys` written by hand. Use the C headers from `/usr/include/libimobiledevice` (Linux) and from the libimobiledevice-win32 release zip (Windows).
- [ ] **Step 3:** Define the public API in `lib.rs`:

```rust
//! Thin wrapper around libimobiledevice + libusbmuxd. Surface area is
//! intentionally minimal — just what iextendd needs for USB pair.

use anyhow::Result;
use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub udid: String,
    pub name: Option<String>,
    pub product_type: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceEvent {
    Plugged,
    Unplugged,
}

/// List currently-connected iOS devices via usbmuxd.
pub fn list_devices() -> Result<Vec<DeviceInfo>>;

/// Open a TCP-shaped socket tunneled to `udid:port` over USB. Returns a
/// blocking `std::net::TcpStream` (caller can convert to tokio if needed).
pub fn connect_socket(udid: &str, port: u16) -> Result<std::net::TcpStream>;

/// Subscribe to plug/unplug events. Each event arrives as a message on the
/// returned channel; drop the receiver to unsubscribe.
pub fn subscribe_events() -> Result<crossbeam_channel::Receiver<(DeviceEvent, DeviceInfo)>>;
```

- [ ] **Step 4:** Add a unit test in `lib.rs` that runs `list_devices()` and asserts it returns `Ok(vec)` (the vec is empty on CI, that's fine — test just verifies the binding loads and the call doesn't panic). Skip test if `IX_USB_SKIP=1` env var is set so it can be muted in CI environments without libimobiledevice.
- [ ] **Step 5:** Run `cargo test -p ix-usb`. Expect: passes locally with libimobiledevice installed; passes (skipped) on CI before we add the apt deps.
- [ ] **Step 6:** Commit. Message: `feat(ix-usb): wrap libimobiledevice for USB device tunneling`.

## Task 2: `iextendd::usb_listener` module

**Files:**
- Create: `host/crates/iextendd/src/usb_listener.rs`
- Modify: `host/crates/iextendd/src/lib.rs` (add `pub mod usb_listener;`)
- Modify: `host/crates/iextendd/Cargo.toml` (depend on `ix-usb`)

- [ ] **Step 1:** Write a failing integration test in `host/crates/iextendd/tests/usb_pair_smoke.rs`:

```rust
//! Smoke test that the USB listener spawn loop wires up cleanly without an
//! actual USB device. With no iPad plugged in, the listener should run idle
//! and return cleanly when the cancel token fires.
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

#[tokio::test(flavor = "multi_thread")]
async fn usb_listener_idle_no_device() {
    if std::env::var("IX_USB_SKIP").is_ok() {
        return;
    }
    let state = Arc::new(RwLock::new(iextendd::DaemonState::new()));
    let cancel = tokio_util::sync::CancellationToken::new();
    let token = cancel.clone();
    let handle = tokio::spawn(async move {
        iextendd::usb_listener::run(state, token).await
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_secs(2), handle).await;
    assert!(result.is_ok(), "usb_listener::run did not exit within 2s of cancel");
}
```

- [ ] **Step 2:** Run the test — expect compile error (`usb_listener::run` not defined).
- [ ] **Step 3:** Implement `usb_listener.rs`:

```rust
//! USB pair listener. Subscribes to libimobiledevice plug events; when an
//! iPad connects, opens a TCP-shaped socket via usbmuxd to its port 7780
//! and dispatches the resulting stream to the existing simple-pair-v0
//! handler.
//!
//! Unlike the Wi-Fi listener (`pair_listener.rs`), this side is the TCP
//! *client* — usbmuxd's tunneling makes the iPad the listener.

use crate::grpc_server::DaemonState;
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

const IPAD_PAIR_LISTEN_PORT: u16 = 7780;
const POLL_INTERVAL: Duration = Duration::from_secs(1);

pub async fn run(state: Arc<RwLock<DaemonState>>, cancel: CancellationToken) -> Result<()> {
    let rx = match ix_usb::subscribe_events() {
        Ok(rx) => rx,
        Err(e) => {
            warn!(err = %e, "USB event subscription unavailable; USB pair disabled");
            // Sleep until cancelled; do not fail — Wi-Fi pair still works.
            cancel.cancelled().await;
            return Ok(());
        }
    };

    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            _ = tokio::time::sleep(POLL_INTERVAL) => {}
        }

        // Drain pending events without blocking.
        while let Ok((event, info)) = rx.try_recv() {
            match event {
                ix_usb::DeviceEvent::Plugged => {
                    on_device_plugged(state.clone(), info).await;
                }
                ix_usb::DeviceEvent::Unplugged => {
                    on_device_unplugged(state.clone(), info).await;
                }
            }
        }
    }
}

async fn on_device_plugged(state: Arc<RwLock<DaemonState>>, info: ix_usb::DeviceInfo) {
    info!(udid = %info.udid, name = ?info.name, "iPad plugged in");
    {
        let mut s = state.write().await;
        s.usb_devices.retain(|d| d.udid != info.udid);
        s.usb_devices.push(info.clone());
    }

    // Spawn a connect task — we want pair to start as soon as the iPad's
    // listener is up. Apple's usbmuxd takes ~200ms after plug-in to surface
    // the device. Retry up to 5x with 250ms backoff before giving up.
    let state = state.clone();
    tokio::spawn(async move {
        for attempt in 0..5 {
            match ix_usb::connect_socket(&info.udid, IPAD_PAIR_LISTEN_PORT) {
                Ok(std_stream) => {
                    if let Err(e) = handle_usb_stream(std_stream, state.clone()).await {
                        warn!(err = %e, "USB pair handler error");
                    }
                    return;
                }
                Err(e) if attempt == 4 => {
                    warn!(err = %e, "USB connect failed after 5 attempts; iPad app likely not running or not foregrounded");
                    return;
                }
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            }
        }
    });
}

async fn on_device_unplugged(state: Arc<RwLock<DaemonState>>, info: ix_usb::DeviceInfo) {
    info!(udid = %info.udid, "iPad unplugged");
    let mut s = state.write().await;
    s.usb_devices.retain(|d| d.udid != info.udid);
}

async fn handle_usb_stream(
    std_stream: std::net::TcpStream,
    state: Arc<RwLock<DaemonState>>,
) -> Result<()> {
    std_stream.set_nonblocking(true)?;
    let stream = tokio::net::TcpStream::from_std(std_stream)?;
    let addr = stream.peer_addr().ok();

    // Reuse the Wi-Fi pair handler. It expects a SocketAddr; for USB we
    // synthesize "127.0.0.1:0" since the real peer is on the other end of
    // the cable (no IP routing involved).
    let addr = addr.unwrap_or_else(|| "127.0.0.1:0".parse().unwrap());

    crate::pair_listener::handle_one_usb(stream, addr, state).await
}
```

- [ ] **Step 4:** Add a public `handle_one_usb` thin wrapper to `pair_listener.rs` that exposes the existing `handle_one` logic for USB callers:

```rust
/// Public entry point used by `usb_listener` so the same simple-pair-v0
/// state machine handles both transports. Reads the first frame to decide
/// SPAKE2 vs simple-pair, then dispatches identically to the Wi-Fi path.
pub async fn handle_one_usb(
    stream: tokio::net::TcpStream,
    addr: std::net::SocketAddr,
    state: Arc<RwLock<DaemonState>>,
) -> Result<()> {
    let pin = state.read().await.pairing.pin.clone();
    if pin.is_empty() {
        return Err(anyhow::anyhow!(
            "USB connect arrived but no PIN is active — click Begin Pairing first"
        ));
    }
    let mut server = PairingServer::new(&pin);
    let _ = handle_one(stream, addr, &mut server, state).await?;
    Ok(())
}
```

- [ ] **Step 5:** Add `usb_devices: Vec<ix_usb::DeviceInfo>` to `DaemonState` in `grpc_server.rs`. Initialize as empty in `DaemonState::new`.
- [ ] **Step 6:** Re-run the integration test from Step 1 — expect: passes (returns within 2s of cancel).
- [ ] **Step 7:** Commit. Message: `feat(daemon): USB pair listener via libimobiledevice`.

## Task 3: Spawn `usb_listener::run` from `main.rs`

**Files:**
- Modify: `host/crates/iextendd/src/main.rs`

- [ ] **Step 1:** In `main`, after the gRPC server spawn, add:

```rust
let usb_state = state.clone();
let usb_cancel = shutdown.clone();
tokio::spawn(async move {
    if let Err(e) = iextendd::usb_listener::run(usb_state, usb_cancel).await {
        tracing::error!(err = %e, "usb_listener exited with error");
    }
});
```

(Use the existing `shutdown` cancellation token if present, or add one.)

- [ ] **Step 2:** Run `cargo run -p iextendd` and verify the daemon starts cleanly with no USB device plugged. Expect log: `WARN USB event subscription unavailable; USB pair disabled` if libimobiledevice isn't installed locally, or no warning if it is.
- [ ] **Step 3:** Commit. Message: `feat(daemon): wire usb_listener into main`.

## Task 4: Surface USB devices to tray via gRPC

**Files:**
- Modify: `host/proto/iextend.proto` (add `UsbDeviceInfo`, extend `StatusReply`)
- Modify: `host/crates/iextendd/src/grpc_server.rs` (populate the new field)

- [ ] **Step 1:** Add to `iextend.proto`:

```proto
message UsbDeviceInfo {
  string udid          = 1;
  string display_name  = 2;
  string product_type  = 3; // e.g. "iPad13,1"
}
```

And in `StatusReply`:

```proto
repeated UsbDeviceInfo usb_devices = 8;
```

- [ ] **Step 2:** Run `cargo check -p iextendd` to regenerate the prost types.
- [ ] **Step 3:** Populate `usb_devices` in the `Status` RPC handler:

```rust
let usb_devices: Vec<UsbDeviceInfo> = state.usb_devices
    .iter()
    .map(|d| UsbDeviceInfo {
        udid: d.udid.clone(),
        display_name: d.name.clone().unwrap_or_default(),
        product_type: d.product_type.clone().unwrap_or_default(),
    })
    .collect();
```

- [ ] **Step 4:** Run `cargo build -p iextendd`. Expect: clean.
- [ ] **Step 5:** Commit. Message: `feat(grpc): surface USB devices in StatusReply`.

## Task 5: Tray "USB connected" chip on Pair tab

**Files:**
- Modify: `host/crates/iextend-tray/src/app.rs`

- [ ] **Step 1:** In `draw_pair`, before the heading, render a chip when `status.usb_devices` is non-empty:

```rust
if let Some(s) = &status {
    if !s.usb_devices.is_empty() {
        let names: Vec<String> = s.usb_devices.iter()
            .map(|d| if d.display_name.is_empty() {
                format!("iPad ({})", &d.udid[..8.min(d.udid.len())])
            } else {
                d.display_name.clone()
            })
            .collect();
        ui.colored_label(
            egui::Color32::from_rgb(0, 200, 80),
            format!("USB connected: {}", names.join(", ")),
        );
        ui.add_space(8.0);
    }
}
```

- [ ] **Step 2:** Run `cargo run -p iextend-tray` and visually verify the chip is empty when no iPad is plugged in.
- [ ] **Step 3:** Commit. Message: `feat(tray): show USB-connected iPads on Pair tab`.

## Task 6: iPad `USBPairingListener`

**Files:**
- Create: `ipad/iExtendKit/Sources/iExtendKit/Connection/USBPairingListener.swift`
- Modify: `ipad/iExtendKit/Sources/iExtendKit/IExtendSession.swift` (start/stop the listener)

- [ ] **Step 1:** Create the listener:

```swift
import Foundation
import Network

/// Listens on 127.0.0.1:7780 for incoming connections from the laptop daemon
/// over Apple's usbmuxd USB tunnel. The wire protocol is identical to the
/// Wi-Fi pair flow (simple-pair-v0).
public final class USBPairingListener {
    public typealias OnPair = (PairResult) -> Void
    public typealias OnPinPrompt = (@escaping (String) -> Void) -> Void

    private let port: NWEndpoint.Port = 7780
    private var listener: NWListener?
    private let queue = DispatchQueue(label: "iextend.usb-pair-listener")
    private let onPair: OnPair
    private let onPinPrompt: OnPinPrompt
    private let displayName: String

    public init(displayName: String, onPinPrompt: @escaping OnPinPrompt, onPair: @escaping OnPair) {
        self.displayName = displayName
        self.onPinPrompt = onPinPrompt
        self.onPair = onPair
    }

    public func start() throws {
        let params = NWParameters.tcp
        params.allowLocalEndpointReuse = true
        // Bind to loopback only — usbmuxd is the only thing that can reach us.
        params.requiredLocalEndpoint = NWEndpoint.hostPort(host: "127.0.0.1", port: port)
        let listener = try NWListener(using: params, on: port)
        listener.newConnectionHandler = { [weak self] conn in
            self?.handle(conn)
        }
        listener.start(queue: queue)
        self.listener = listener
    }

    public func stop() {
        listener?.cancel()
        listener = nil
    }

    private func handle(_ conn: NWConnection) {
        conn.start(queue: queue)
        // Prompt the UI for a PIN. The user typed it from the laptop's tray.
        onPinPrompt { [weak self] pin in
            guard let self else { return }
            Task {
                do {
                    let result = try await PairingFlow.completeUSB(
                        connection: conn,
                        pin: pin,
                        displayName: self.displayName
                    )
                    self.onPair(result)
                } catch {
                    self.onPair(.failure("\(error)"))
                }
            }
        }
    }
}
```

- [ ] **Step 2:** Add `PairingFlow.completeUSB(connection:pin:displayName:)` in `PairingFlow.swift`. It should: assemble PSimpleHello with the pin + the iPad's pubkey + displayName, write it to the connection, read PSimpleAck, parse, return `PairResult`. (95% of the logic is already in the existing `pair(host:port:pin:displayName:)` method — extract the post-connection-open part into a shared helper and reuse it.)
- [ ] **Step 3:** In `IExtendSession.swift`, instantiate `USBPairingListener` on `init()` and call `start()` when the app foregrounds; `stop()` on background. Wire the `onPinPrompt` callback to the existing PIN-entry UI (same one Wi-Fi uses).
- [ ] **Step 4:** Run the iPad target locally in Simulator and verify nothing crashes when the listener binds. (Simulator can't actually receive USB connections — that's fine, we're only smoke-testing the bind.)
- [ ] **Step 5:** Commit. Message: `feat(ipad): USB pair listener on 127.0.0.1:7780`.

## Task 7: Linux `.deb` depends on libimobiledevice

**Files:**
- Modify: `host/crates/iextendd/Cargo.toml` (`[package.metadata.deb]`)

- [ ] **Step 1:** Update `depends`:

```toml
[package.metadata.deb]
depends = "libc6 (>= 2.31), libimobiledevice6, usbmuxd"
```

- [ ] **Step 2:** Verify with `cargo deb --no-build -p iextendd --output /tmp/test.deb` if `cargo-deb` is installed; otherwise just inspect the diff.
- [ ] **Step 3:** Commit. Message: `chore(packaging): add libimobiledevice dependency to deb`.

## Task 8: Windows CI bundles libimobiledevice DLLs

**Files:**
- Modify: `.github/workflows/host-ci.yml`

- [ ] **Step 1:** In the Windows job, after the cargo build but before artifact upload, add a step that downloads the libimobiledevice-win32 release zip, extracts the relevant DLLs (`imobiledevice.dll`, `libusbmuxd.dll`, `libplist.dll`, `libcrypto.dll`, etc.) to a `dist/` folder alongside the iextendd.exe / iextend-tray.exe, and uploads the whole `dist/` as the artifact instead of just the bare exes.
- [ ] **Step 2:** The download URL is currently `https://github.com/libimobiledevice-win32/imobiledevice-net/releases/download/v1.3.17/libimobiledevice.1.3.17-r1122-win-x64.zip` — pin this exactly so a future libimobiledevice-win32 release doesn't break the build.
- [ ] **Step 3:** Push and watch the host-ci run for the windows-latest job. Expect: artifact `iextend-windows` contains `iextendd.exe`, `iextend-tray.exe`, and ~5 DLLs.
- [ ] **Step 4:** Commit. Message: `ci(host): bundle libimobiledevice DLLs into Windows artifact`.

## Task 9: End-to-end manual test

**Files:** none (test plan only)

- [ ] **Step 1:** Build a fresh Windows tray + daemon from CI artifact 8.
- [ ] **Step 2:** Build a fresh iPad IPA via ipad-ci with Task 6's changes.
- [ ] **Step 3:** Sideload IPA on iPad. Plug iPad into laptop via USB-C cable.
- [ ] **Step 4:** Run daemon + tray on Windows. Expect: tray Pair tab shows "USB connected: <iPad name>" within 2 seconds.
- [ ] **Step 5:** Click "Begin pairing". Expect: PIN appears.
- [ ] **Step 6:** Open iExtend on iPad. Expect: a PIN-entry sheet auto-presents (because the laptop's `usb_listener::on_device_plugged` opened a connection to the iPad's port 7780, which woke the listener).
- [ ] **Step 7:** Type the PIN displayed on the laptop into the iPad. Expect: tray transitions to "Paired with <name>" within 500ms; iPad transitions to the "Paired!" screen.
- [ ] **Step 8:** Open Devices tab on tray. Expect: the new iPad appears in the list.
- [ ] **Step 9:** Unplug. Expect: "USB connected" chip disappears within 2s.
- [ ] **Step 10:** Re-plug. Expect: no re-pair required (PinStore already has the iPad's pubkey).
