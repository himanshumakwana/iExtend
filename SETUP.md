# iExtend — Setup Guide

> _Pick the section that matches what you want to do. Each section is independent — you don't need any of the others to follow it._

iExtend turns an **iPad** into a wireless second screen for a **Windows** or **Linux** laptop. The repo currently contains:

- A working **visual deliverable** (`iExtend.html`) you can open in a browser today.
- A **Rust host workspace** that builds and runs the daemon + tray on Linux today.
- **Scaffolded** Windows driver / Linux evdi / iPad Swift app source — runs after platform-specific resources are added (kernel SDK, codesigning cert, hardware encoder, evdi DKMS module, etc.).

---

## TL;DR — Just Show Me The Design

```bash
git clone git@github.com:himanshumakwana/iExtend.git
cd iExtend
npm install
npx playwright install chromium
npm run view                   # python3 -m http.server 8080
# open http://localhost:8080/iExtend.html
```

You'll see the full Figma-style design canvas — 15 artboards across 4 sections (Onboarding, Connected, Settings & errors, Floating toolbar variants), with pan/zoom/drag-reorder/fullscreen-focus and a live tweaks panel for dark mode + toolbar position + density + connection state.

---

## Repository Layout

```
iExtend/
├── iExtend.html                     # ← open in browser; full design canvas
├── design-source/                   # frozen Claude Design bundle (the .jsx files iExtend.html is built from)
├── docs/
│   ├── superpowers/specs/           # engineering design spec (single source of truth)
│   ├── superpowers/plans/           # 10 implementation plans (one per subsystem)
│   ├── codesigning-procurement.md   # EV cert / Apple Developer Program cost breakdown
│   └── install/linux.md             # Linux-side install guide for end users
├── host/                            # Rust workspace (daemon + tray + 9 ix-* libraries + xtask + screen-timecode)
│   ├── Cargo.toml
│   ├── crates/
│   ├── drivers/windows/             # iexdd.sys (IddCx) + vhf-stylus.sys driver source
│   ├── installer/{windows,linux}/   # MSIX manifest, .deb/.rpm/AppImage configs
│   ├── proto/iextend.proto
│   ├── CONTRIBUTING.md
│   └── target/                      # build output (gitignored)
├── ipad/                            # Swift Package + Xcode project
│   ├── Package.swift
│   ├── iExtendKit/                  # WebRTC + decode + Metal render
│   ├── iExtendUI/                   # SwiftUI screens (1:1 port of iExtend.html)
│   ├── iExtendInput/                # Touch / Pencil / keyboard capture
│   ├── iExtend/                     # App target (App entry, ContentView, Info.plist)
│   ├── installer/AppStoreConnect/   # fastlane config, privacy policy
│   └── Frameworks/                  # WebRTC.xcframework (download via CI cache or LFS)
├── bench/
│   ├── camera-rig/                  # 240 fps camera bench tooling (screen-timecode + analyzer)
│   ├── cluster/                     # Ansible playbooks for the 4-box hardware CI cluster
│   ├── soak/                        # 8-hour endurance runner + Chart.js dashboard
│   └── pencil-feel/                 # 10-reviewer Likert protocol
├── tests/iextend.spec.ts            # Playwright tests for iExtend.html
└── .github/workflows/               # CI — host-ci, ipad-ci, perf-nightly, release
```

---

## Prerequisites by Task

| What you want to do | What you need on your machine |
|---|---|
| **View** `iExtend.html` | A modern browser. (`npm` only needed for tests.) |
| **Run** Playwright tests | Node 20+, `npm`, `npx playwright install chromium`. |
| **Build & run** the Rust host on **Linux** | Rust 1.88+ (the repo's `rust-toolchain.toml` pins 1.90), `apt install build-essential libssl-dev pkg-config libxkbcommon-dev`. |
| **Build & run** the Rust host on **Windows** | Rust 1.88+, Visual Studio 2022 Build Tools with the C++ desktop workload. |
| **Build** the **Windows IddCx driver** | Windows + WDK 10.0.26100, MSBuild, EV Authenticode cert ($300/yr) for sign + WHQL submission. |
| **Build** the **Linux evdi DKMS** package | DKMS, kernel headers, evdi kernel module, optional MOK enrollment for SecureBoot. |
| **Build & run** the iPad app | macOS 14+, **Xcode 16+**, Apple Developer Program ($99/yr) for device install (simulator works without it), Google's `WebRTC.xcframework` (the CI workflow downloads stasel's M141 build automatically). |
| **Run** the bench rig | A 240 fps camera (iPhone slo-mo or Phantom Veo), a tripod jig holding the host display + iPad in the same frame, OpenCV + Tesseract on a workstation. |

---

## Path 1 — View the Design Canvas (5 minutes, any OS)

```bash
git clone git@github.com:himanshumakwana/iExtend.git
cd iExtend
npm install
npm run view
# Open http://localhost:8080/iExtend.html
```

To run the Playwright tests:

```bash
npx playwright install chromium
npm test
```

You should see **6 tests passing** (load, render 15 artboards across 4 sections, pan, zoom, label visibility, section titles).

---

## Path 2 — Run the Rust Host (Linux, ~5 minutes for first build)

```bash
cd iExtend/host
cargo build --workspace                      # ~3 min first time, cached after
cargo test  --workspace --no-fail-fast       # 81 tests pass

# Terminal 1 — start the daemon
cargo run -p iextendd
# Output:
#   {"timestamp":"…","level":"INFO","message":"iextendd starting","version":"0.1.0",
#    "endpoint":"/run/user/1000/iextendd.sock"}

# Terminal 2 — start the tray GUI
cargo run -p iextend-tray
```

In the tray window, click **"Ping iextendd"** — you should see something like:

> `v0.1.0 · uptime 12s`

If you don't have a display server (running headless / SSH), skip the tray step. The daemon works on its own and the gRPC `Status` RPC can be hit directly:

```bash
# Verify the daemon is listening and the socket exists:
ls -la "$XDG_RUNTIME_DIR/iextendd.sock"
```

### Stopping cleanly

`Ctrl-C` in the daemon terminal — the SIGINT handler removes the socket and emits `{"message":"iextendd stopped"}`.

---

## Path 3 — Build the Rust Host on Windows

The workflow is identical to Linux except the IPC transport switches to a Named Pipe (`\\.\pipe\iextendd`).

```powershell
# In a Developer PowerShell for VS 2022:
cd iExtend\host
cargo build --workspace
cargo test --workspace --no-fail-fast

# Two PowerShell windows:
cargo run -p iextendd
cargo run -p iextend-tray
```

The Windows path of `iextendd::main` currently bails with a documented "named-pipe accept loop is the implementer's exercise" message — Plan 3 wires up the kernel-mode IddCx driver and Plan 5/Plan 7 wire the named-pipe loop. On a fresh clone the host will run on Windows enough to compile, test, and serve from the daemon binary's startup; the named-pipe path needs the next implementer to land it.

---

## Path 4 — Build the iPad App (macOS, ~10 minutes)

> Plan 6 produced 25 Swift source files matching the design canvas 1:1, but the actual `iExtend.xcodeproj/project.pbxproj` file is generated by Xcode itself — it isn't committed yet because Plan 6 was scaffolded on a Linux box without Xcode. The first macOS engineer to clone the repo runs the steps below.

### One-time setup

1. Install **Xcode 16** from the App Store.
2. Clone the repo:

   ```bash
   git clone git@github.com:himanshumakwana/iExtend.git
   cd iExtend/ipad
   ```

3. Drop in Google's WebRTC binary. The CI workflow uses [stasel's community build](https://github.com/stasel/WebRTC/releases) — fetch the same one locally:

   ```bash
   mkdir -p Frameworks
   curl -L \
     -o /tmp/WebRTC.xcframework.zip \
     https://github.com/stasel/WebRTC/releases/download/141.0.0/WebRTC-M141.xcframework.zip
   unzip /tmp/WebRTC.xcframework.zip -d Frameworks/
   ```

4. **Generate the Xcode project.** Easiest path: use [`xcodegen`](https://github.com/yonaskolb/XcodeGen) with a `project.yml`, or open Xcode and create a new "iOS App" target then add the existing `iExtendKit/`, `iExtendUI/`, `iExtendInput/` directories as Swift Package Manager local-path dependencies. Commit the resulting `iExtend.xcodeproj/`.

5. Configure signing in Xcode → Project → Signing & Capabilities. For Simulator-only development you can leave it as "automatic, no team."

### Build for the Simulator

```bash
cd iExtend/ipad
xcodebuild build \
  -project iExtend.xcodeproj \
  -scheme iExtend \
  -configuration Debug \
  -destination 'generic/platform=iOS Simulator' \
  CODE_SIGNING_ALLOWED=NO
```

### Run on a Simulator

Open Xcode → press ▶︎ with an **iPad Pro 11" (M4)** simulator selected.

### Build for a Real Device (TestFlight)

You'll need:

- Apple Developer Program enrollment ($99/yr).
- A Distribution provisioning profile + signing identity, set up via [`fastlane match`](https://fastlane.tools).
- The `release.yml` workflow handles this automatically on tag push (`v*`); it uses `ipad/installer/AppStoreConnect/Fastfile`.

---

## Path 5 — Build & Install the Linux .deb / RPM / AppImage

> Same caveat as the iPad app: the daemon and tray binaries build today on any Linux box; the `iextend-evdi-dkms` package needs the upstream `evdi` source vendored under `host/packaging/linux/evdi-dkms/` before it produces a working kernel module.

### Debian / Ubuntu

```bash
cd iExtend/host
cargo install cargo-deb --locked
cargo build --release -p iextendd -p iextend-tray
cargo deb -p iextendd --no-build --no-strip
cargo deb -p iextend-tray --no-build --no-strip
ls target/debian/                # iextend-daemon_*.deb, iextend-tray_*.deb
sudo dpkg -i target/debian/iextend-daemon_*.deb target/debian/iextend-tray_*.deb
```

### Fedora / RHEL / openSUSE

The RPM spec is at `host/installer/linux/rpm/iextend.spec`. Build with `rpmbuild -bb` after staging the binaries.

### AppImage

`host/installer/linux/AppImage.yml` is configured for `appimage-builder`. Note: the AppImage **cannot install the kernel module** — users will need the DKMS package separately for full functionality.

### SecureBoot users

Run the helper script to enroll the evdi signing key into your MOK:

```bash
sudo bash host/installer/linux/dkms-mok-enrollment.sh
# Follow on-screen instructions and reboot to confirm enrollment in MOK Manager.
```

---

## Path 6 — Build the Windows MSIX Installer

> Distribution-grade only. EV Authenticode cert + WHQL submission required.

```powershell
cd iExtend\host\installer\windows
.\build.ps1 -Version 0.1.0
# Output: iExtend_0.1.0_x64.msix (signed if EV cert is configured)
```

The build script:

1. Runs `cargo build --release -p iextendd -p iextend-tray`.
2. Stages binaries into a Package layout.
3. Patches `iextend.appxmanifest` with the version.
4. Runs `MakeAppx pack`.
5. Runs `signtool sign /sha1 $env:IEXTEND_EV_THUMBPRINT` to apply the EV cert.

The kernel-mode driver (`iexdd.sys`) is signed separately via the WHQL flow — see `host/drivers/windows/README.md` for the HLK Studio submission walkthrough.

---

## CI / Downloadable Builds

Every push or PR triggers GitHub Actions and produces downloadable artifacts retained for 30 days:

| Workflow | Artifact name | Contents |
|---|---|---|
| `host-ci` (Linux) | `iextend-linux-x86_64` | `iextendd`, `iextend-tray`, `iextend-daemon_*.deb`, `iextend-tray_*.deb` |
| `host-ci` (Windows) | `iextend-windows-x86_64` | `iextendd.exe`, `iextend-tray.exe` (unsigned dev builds) |
| `ipad-ci` | `iextend-ipad-simulator-app` | `iExtend.app` for the iOS Simulator (drop into a booted simulator) |
| `release.yml` (on tag push) | All of the above, signed | Signed MSIX, signed .deb, TestFlight upload via fastlane |

Find them at: **Repo → Actions → click any successful run → Artifacts** at the bottom.

---

## Common Gotchas

- **`cargo metadata` exits 101 with "missing manifest"** — happens when running on a fresh clone before any crate has been built. Solution: just run `cargo build --workspace` once; it auto-fetches the toolchain via `rust-toolchain.toml`.
- **`-D warnings` rejects edits in scaffolded crates** — Plan 3-10 sources have known minor lints. The `host-ci` workflow runs clippy as **non-fatal** during the scaffolding phase. Promote back to `-D warnings` when each subsystem reaches its first verified end-to-end milestone.
- **`apt install protobuf-compiler` is NOT required** — `host/crates/iextendd/build.rs` and `host/crates/iextend-tray/build.rs` use the `protoc-bin-vendored` build dependency, which ships pre-built protoc binaries inside the crate.
- **Chrome blocks `file://` font loads for `iExtend.html`** — always use the local server (`npm run view`). Firefox tolerates `file://` but Chrome doesn't.
- **iPad app won't build without WebRTC.xcframework** — see Path 4 step 3 above; the framework is ~280 MB and not committed (Git LFS recommended for vendoring it).
- **Workspace tests run for ~13 minutes the first time** — `cargo build --workspace` compiles ~400 crates including the entire WebRTC + tonic + tokio stack. Subsequent builds are ~30 seconds with `Swatinem/rust-cache@v2` (already wired into CI) or a warm local cache.

---

## What's Implemented vs. Stubbed

See `README.md` § _Plan status_ for the per-plan checkbox table. In short:

- **Plan 1** ✅ (iExtend.html — runs in your browser today)
- **Plan 2** ✅ (Rust workspace — daemon + tray + gRPC, runs on Linux today)
- **Plans 3-10** scaffolded (~30K lines of source), need platform-specific resources for full end-to-end deployment

Each plan's `docs/superpowers/plans/2026-05-08-plan-N-*.md` document is a complete bite-sized task list. Pick a plan to drive forward, follow the tasks, and the relevant subsystem becomes runnable.

---

## Where to Get Help

- **Spec & design:** `docs/superpowers/specs/2026-05-08-iextend-design.md`
- **Per-plan details:** `docs/superpowers/plans/`
- **Visual reference:** open `iExtend.html` — every iPad screen and every PC tray screen is laid out as a clickable artboard in the design canvas.
- **CI status:** https://github.com/himanshumakwana/iExtend/actions
