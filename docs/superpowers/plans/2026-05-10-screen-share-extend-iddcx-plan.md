# Screen Share — Extend Mode via IddCx Driver (Plan B)

> **For agentic workers:** This plan executes on a parallel branch from Plan A (Mirror + WebRTC). Several milestones below are GATED on the user's Windows dev environment — they cannot proceed without active human involvement. Those are flagged explicitly.

**Goal:** Make the iPad show up to Windows as a real secondary display, so dragging a window onto it just works (Extend mode, vs Plan A's Mirror mode). Encoded frames flow through the same encoder/transport pipeline Plan A builds.

**Architecture:** A user-mode IddCx (Indirect Display Component eXtension) driver registers a virtual display adapter with Windows; the kernel hands it framebuffer-accessible frames; the driver IPC's frames to `iextendd` over a named pipe; daemon feeds them into the existing encode → WebRTC → iPad pipeline.

**Tech Stack:** C++ (driver), WDK (Windows Driver Kit), IddCx ≥ 1.4 API (Win10 1903+), Visual Studio 2022 Build Tools, named-pipe IPC to the existing Rust daemon.

**Critical constraint:** This driver lives in user mode but still needs to be **signed for distribution** (EV code-signing certificate, ~$200-400/year). For development, your Windows machine needs to run in **Test Mode** OR have a self-signed test certificate installed in TrustedRootCertificationAuthorities. There is no way around this — Microsoft mandates driver signing.

---

## ⚠️ User involvement gates (read first)

| Gate | What's needed | When it blocks |
|---|---|---|
| **G1: WDK installed** | `winget install Microsoft.WindowsSDK Microsoft.VisualStudio.2022.BuildTools` + Windows Driver Kit | Before M1 |
| **G2: Test signing enabled** | `bcdedit /set testsigning on` (admin), reboot, accept the "Test Mode" watermark | Before M2 |
| **G3: Test cert in trust store** | Certutil-imported test cert (I'll script this) | Before M3 |
| **G4: Live driver test on your machine** | You install the .inf via `pnputil`, observe Windows Display Settings | Before M4 |
| **G5: Production EV cert** | Buy one (~$200-400/yr from DigiCert/Sectigo/etc.) | Before public distribution only — dev/CI runs are fine without |

I cannot complete G1-G5 from this Linux dev box. Tell me when each one is unblocked.

---

## Milestone M1: Driver scaffolding compiles 🟡 (gated on G1)

**Files:**
- Create: `host/driver/iextend-iddcx/iextend-iddcx.vcxproj` (driver project)
- Create: `host/driver/iextend-iddcx/Driver.cpp` (DriverEntry)
- Create: `host/driver/iextend-iddcx/IndirectDevice.cpp` (IddCxAdapter callbacks)
- Create: `host/driver/iextend-iddcx/iextend-iddcx.inf` (install manifest)
- Create: `xtask/src/driver_build.rs` (Rust subcommand to invoke msbuild + sign)

- [ ] **Step 1:** Bootstrap a minimal IddCx driver from the Microsoft sample at `Microsoft/Windows-driver-samples/general/IndirectDisplay`. Strip the bits we don't need (HDR for now), keep adapter init, monitor enumeration, frame swap.
- [ ] **Step 2:** Wire `xtask driver build` to invoke `msbuild iextend-iddcx.vcxproj /p:Configuration=Release` plus `signtool sign /fd SHA256 /a /v ...` with a test cert.
- [ ] **Step 3:** CI: add a `driver-ci.yml` workflow on windows-2022 that runs `xtask driver build` and uploads the .sys + .inf as artifacts. (Build verification only — install/load happens on user's machine.)
- [ ] **Step 4:** Commit. `feat(driver): minimal IddCx scaffold + xtask build pipeline`.

**Done when:** CI produces a `iextend-iddcx-driver-unsigned` artifact containing `iextend-iddcx.sys`, `iextend-iddcx.inf`, `iextend-iddcx.cat`.

## Milestone M2: Driver loads + registers virtual adapter 🔴 (gated on G2 + G4)

**Requires you to:**
1. Run `bcdedit /set testsigning on` and reboot (G2).
2. From an elevated PowerShell: `pnputil /add-driver iextend-iddcx.inf /install`.
3. Open Device Manager → System Devices, confirm "iExtend Indirect Display Adapter" appears.
4. Open Windows Display Settings, confirm a new monitor labeled "iExtend Virtual Display" appears.
5. Send me a screenshot of Display Settings and the output of `pnputil /enum-devices /class Display`.

If anything fails, the driver inf or signing cert is wrong — I'll fix and re-cut.

- [ ] **Step 1:** Implement `EVT_IDD_CX_ADAPTER_INIT_FINISHED` callback to register one monitor with mode list (1920×1080 @ 60Hz, 1440×900 @ 60Hz, 1280×720 @ 60Hz initial set).
- [ ] **Step 2:** Implement `EVT_IDD_CX_PARSE_MONITOR_DESCRIPTION` for our hardcoded EDID blob.
- [ ] **Step 3:** Local test on your machine (gate G4).
- [ ] **Step 4:** Commit. `feat(driver): register virtual monitor with three modes`.

## Milestone M3: Driver delivers frames to a named pipe

**Files:**
- Modify: `host/driver/iextend-iddcx/IndirectDevice.cpp`
- Create: `host/driver/iextend-iddcx/PipeChannel.cpp`

- [ ] **Step 1:** Implement `EVT_IDD_CX_MONITOR_ASSIGN_SWAPCHAIN` — when Windows hands us a swapchain, open a named pipe `\\.\pipe\iextend-frames-<adapter-id>` and stream BGRA frames to it as length-prefixed buffers. Skip frames if the pipe isn't connected (daemon not running).
- [ ] **Step 2:** Frame delivery thread: spin on `IDDCxMonitorWaitForFrame` → `IDDCxMonitorQueryNextFrame` → write to pipe → `IDDCxMonitorReleaseFrame`. Match the timing requirements in IddCx docs to avoid frame timeouts.
- [ ] **Step 3:** Driver test: a tiny Rust client that opens the named pipe and counts incoming frames. Run for 5 seconds with a window dragged onto the virtual display, expect ~150 frames.
- [ ] **Step 4:** Commit. `feat(driver): pipe BGRA frames to userland`.

## Milestone M4: Daemon consumes the pipe + reuses Plan A's encoder

**Files:**
- Create: `host/crates/iextendd/src/extend_capture.rs`

- [ ] **Step 1:** New module that opens `\\.\pipe\iextend-frames-*` (with retry — driver might not be loaded), reads frames, converts BGRA → YUV420P, pushes into the same `Frame` channel Plan A's DXGI capture uses.
- [ ] **Step 2:** Tray UI: add a "Mode" toggle (Mirror / Extend). Selecting Extend picks the pipe source instead of DXGI.
- [ ] **Step 3:** End-to-end: drag a window onto the virtual display → see it on the iPad through the same encode/transport/decode pipeline.
- [ ] **Step 4:** Commit. `feat(daemon): wire IddCx pipe into the existing encoder`.

**Done when:** dragging Notepad onto the "iExtend Virtual Display" results in Notepad appearing on the iPad's screen.

## Milestone M5: Production signing + distribution 🔴 (gated on G5)

Out of scope for the dev cycle. When you're ready to distribute:
1. Buy an EV code-signing cert.
2. Use Microsoft's WHCP (Windows Hardware Compatibility Program) or attestation-signing portal to get the .sys signed by Microsoft.
3. Bake into the MSIX installer in `release.yml`.

I'll author the `release.yml` driver-signing step when the cert exists.

---

## Out of scope (forever, or until a different plan)

- DriverKit on Windows (different API surface, irrelevant)
- Per-app capture (vs full virtual display)
- Mac as host (macOS doesn't ship IddCx; use `CGVirtualDisplay` in a separate plan if needed)
