# Remaining Work — Resume Guide

Pick-up point for the next session. State frozen at commit `f6caef0` on 2026-05-11.

## Stage 0: Unblock CI (USER ACTION — NOTHING ELSE MOVES UNTIL DONE)

GitHub Actions is failing instantly with:
> "The job was not started because recent account payments have failed or your spending limit needs to be increased."

- GitHub → Settings → **Billing & plans** → fix payment method or raise spending limit.
- After fix: `gh workflow run host-ci.yml --ref main` and `gh workflow run ipad-ci.yml --ref main` to re-trigger on the four unvalidated commits.

Affected runs (all sub-5s failures): `25630746714` (host-ci on `4c91ca3`), `25630806558` (ipad-ci on `f6caef0`), `25630632420` (ipad-ci on `aee12d3`).

## Stage 1: Triage CI failures on unvalidated commits

Commits pushed in last session, **never CI-validated**:

| Commit | Description | Most likely failure |
|---|---|---|
| `d71c736` | M4b — daemon RtcPeer in signaling::connection_loop | should be green — local smoke passed |
| `aee12d3` | M4c — iPad WebRTC peer + signaling orchestrator + render bridge | Swift API surface mismatches against WebRTC.framework version |
| `4c91ca3` | M5 host — DXGI capture → encode → broadcast pipeline | webrtc-rs link on Windows; resolution defaults to 1920x1080 (issue if iPad's encoder negotiates other) |
| `f6caef0` | M5 iPad — wire WebRTCSession into post-pair flow | Compile error around @MainActor / Sendable boundaries possible |

Fix order: ipad-ci first (smaller surface, fewer cascade failures), then host-ci.

Likely fix shapes (from prior experience):
- `RTCRtpTransceiver.receiver.track` cast: may need `as? RTCVideoTrack` vs direct access depending on WebRTC.framework M147 API
- `@MainActor` polling task in `SessionViewModel.startStreaming` — Swift 6 strict concurrency may flag the `[weak self]` capture
- `MIME_TYPE_H264` constant location in webrtc-rs may differ between 0.10 and 0.11

## Stage 2: Validate first-pixel end-to-end (REQUIRES USER HARDWARE)

Once CI is green, user does:

1. Download `iextend-windows-x86_64` from latest host-ci, unzip on Windows.
2. Download `iextend-ipad-ipa-unsigned` from latest ipad-ci, sideload via AltStore.
3. Open Apple Devices app (provides AMDS) — required for USB pair runtime, also matters if WebRTC.framework's bundle init touches it.
4. Run `iextendd.exe` in a terminal (so logs are visible). Look for:
   - `"WebRTC signaling listener bound port=7783"`
   - `"screen-share: starting Windows DXGI capture path"`
   - `"DXGI duplication acquired"` (capture works)
5. Run `iextend-tray.exe`. Begin pairing.
6. On iPad: open app → "Get started" → Pair manually → enter laptop IP/port/PIN → tap Pair.
7. **Watch for**:
   - Daemon log: `"signaling: client connected"`, `"signaling: received offer"`, `"signaling: sent answer"`, `"screen-share: peer registered"`
   - iPad app: LiveView should swap to `RemoteVideoView` once WebRTC negotiates.

Realistic failure modes I expect to debug interactively:
- **DTLS handshake never completes** — ICE candidate exchange broken. Daemon's `on_ice_candidate` forwards via signaling, iPad's `didGenerateLocalCandidate` does the same. Check both directions are flowing.
- **Codec mismatch** — daemon's RtcPeer adds `MIME_TYPE_H264` track but doesn't pin profile-level-id. iPad may negotiate VP8 instead. Fix: explicitly register only H.264 codecs on both ends.
- **First frame never arrives** — encoder produces NALs but webrtc-rs's RTP packetizer wants specific framing. OpenH264 emits Annex-B start codes (00 00 00 01); webrtc-rs may want length-prefixed AVCC. Check `Sample.data` format expectations.
- **Resolution mismatch** — daemon hardcoded 1920x1080 in `screen_share::DEFAULT_W/H`; if iPad screen or laptop monitor is different, frame loop drops everything. Make resolution dynamic.

Have user paste daemon log + iPad console output (Xcode → Devices and Simulators → console while plugged in). From those, iterate.

## Stage 3: Complete Plan A polish (M6, M7)

**M6 — sustained 30 fps:** Tune `screen_share`:
- `intra_refresh_rows` currently 4 — may need 8 for screen content
- OpenH264 `set_max_frame_rate` already configured at construction; verify it's honored
- Add a stats overlay in `LiveView` showing fps + bitrate (read from `RTCStatsReport` via `pc.getStats()` on iPad)
- 60-second soak test: no dropped frames, no stutter

**M7 — adaptive bitrate:**
- Wire `ix-rtc/bitrate_controller.rs` to actually feed back into `X264Sw.set_bitrate(kbps)`
- Source RTCP REMB / transport-CC from webrtc-rs's `pc.on_signaling_state_change` or RTCRtpSender stats
- Tray Settings tab already has bitrate input — let it override the controller's ceiling

Test by throttling with `tc qdisc` on Linux or Network Link Conditioner on Mac.

## Stage 4: Outstanding features (gap from original scope)

### 4a. mDNS auto-discovery (~2 days)

- Wire `ix-discover` crate (currently scaffold) to publish a `_iextend._tcp` SRV record on the daemon side with the pair port.
- iPad's `DiscoverView` consumes via `NWBrowser` for `_iextend._tcp`, populates `discoveredPeers` automatically.
- Removes the "Pair manually" requirement for same-LAN devices.

### 4b. Input forwarding (Plan 8) — touch, Apple Pencil, keyboard (~1-2 weeks)

- `ix-input` crate is scaffolded. Wire wire-format messages over the WebRTC control DataChannel (already opened by `PeerConnection.swift` line 124).
- iPad: capture `UITouch` + `PencilKit` + `GCKeyboard` events, serialize, send via DataChannel.
- Host: deserialize, synthesize via Windows SendInput / Linux uinput.
- Key constraint: input latency budget is **<15 ms glass-to-glass**, so don't batch — send each touch event immediately.

### 4c. HDR pipeline (~1 week)

- Encoder side: `X264Sw` is BT.709 SDR only. HDR requires hardware encoder (NVENC / AMF / VAAPI with HEVC10).
- Capture side: DXGI already exposes HDR metadata when present (`DXGI_OUTPUT_DESC1::ColorSpace`).
- iPad render: `AVSampleBufferDisplayLayer` already does HDR if the buffer is tagged. `RTCMTLVideoView` may not — switch to a custom Metal renderer for HDR.
- Spec gate: only fire when both ends advertise HDR support.

### 4d. Hardware encoders (~1 week each, mostly independent)

- `ix-codec/nvenc_hevc.rs`, `qsv_hevc.rs`, `amf_hevc.rs`, `vaapi_hevc.rs` are scaffolds.
- Each needs the vendor SDK at build time (CUDA, oneVPL, AMF, libva).
- `ix_codec::probe` already detects available encoders; just need to wire the actual encode implementations.
- Order: NVENC first (most users have NVIDIA), then QSV (Intel iGPU), then AMF (AMD GPU), then VAAPI (Linux).

### 4e. Linux capture (Plan 4) (~1 week)

- `ix-display-linux` is scaffold. Two paths:
  - **PipeWire** (Wayland) — modern, but requires PipeWire 0.3+ and xdg-desktop-portal interaction for permission.
  - **DRM/KMS** (X11) — older but works without portals; needs root or `cap_sys_admin`.
- Recommend PipeWire as primary, DRM as fallback.

### 4f. Audio routing

- Explicitly skipped at project start, but documented as a feature gap.
- Host: WASAPI loopback capture on Windows, PipeWire audio capture on Linux.
- Transport: Opus via WebRTC audio track (just add `addTransceiver(of: .audio, ...)` on iPad + a `TrackLocalStaticSample` for Opus on host).
- Decode: WebRTC.framework handles Opus → AVAudioPlayer natively.

## Stage 5: Plan B — Extend mode (IddCx virtual display driver)

See `docs/superpowers/plans/2026-05-10-screen-share-extend-iddcx-plan.md`.

**Hard-blocked on user's Windows dev environment.** Gates to unlock in order:

| Gate | What's needed |
|---|---|
| G1 | WDK installed (`winget install Microsoft.WindowsSDK Microsoft.VisualStudio.2022.BuildTools`) |
| G2 | `bcdedit /set testsigning on`, reboot |
| G3 | Self-signed test cert imported to `TrustedRootCertificationAuthorities` |
| G4 | User installs the driver via `pnputil` and confirms it appears in Display Settings |
| G5 | (Distribution only) EV code-signing cert from DigiCert / Sectigo / etc. (~$200-400/yr) |

Nothing I can do from Linux until G1-G4 are confirmed.

## Stage 6: Production hardening

### 6a. Production code signing

- **Windows .exe + driver:** EV code-signing cert + WHCP / attestation signing portal for the .sys driver.
- **iOS .ipa:** App Store Connect distribution profile + provisioning. Currently we ship unsigned IPA + AltStore re-signs locally.

### 6b. Auto-update

- Windows: bundled MSIX + Microsoft Store delivery, OR custom Squirrel-like updater.
- iPad: TestFlight or App Store builds with normal Apple update channel.

### 6c. Crash reporting + analytics

- Sentry or similar. User opt-in.

### 6d. Documentation site

- README.md is minimal currently. Public-facing docs (install, setup, troubleshoot) need writing once the product is real.

## Reference: session memory entries

(Located at `/home/tops/.claude/projects/-home-tops-Projects-iExtend/memory/`)

- `feedback_test_locally_before_push.md` — fmt+clippy+test before every push
- `feedback_no_demo_no_stubs.md` — empty states with explanatory copy beat fake content
- `feedback_cargo_fmt_cfg_windows.md` — run rustfmt directly on cfg-gated files
- `feedback_check_ci_billing_first.md` — sub-5s CI failures are billing, not code
- `project_repo_layout.md` — host workspace + iPad SPM + 4 CI workflows
- `project_pair_protocol.md` — simple-pair-v0 over Wi-Fi + USB transports

## Reference: existing plan docs

- `2026-05-08-iextend-design.md` — original spec (full feature set, before MVP scoping)
- `2026-05-08-plan-2-rust-host-workspace.md` through `plan-10`-bench-rig-ci.md` — original ten-plan breakdown (much of this is now scaffolded but not wired)
- `2026-05-10-usb-pair-design.md` + `usb-pair-plan.md` — USB pair (SHIPPED)
- `2026-05-10-screen-share-mirror-webrtc-plan.md` — Plan A (M1-M4a + M4b shipped, M5 unvalidated, M6/M7 pending)
- `2026-05-10-screen-share-extend-iddcx-plan.md` — Plan B (BLOCKED)

## Quick-resume checklist

Next session:

- [ ] Confirm CI billing is restored. If not: stop, tell user.
- [ ] Re-run host-ci + ipad-ci on `f6caef0`. Capture any failures.
- [ ] Fix CI failures (iPad first). Push fixes. Verify green.
- [ ] If user has hardware available: walk Stage 2 (validate first-pixel). Iterate on whatever real issue surfaces.
- [ ] If first-pixel works: move to Stage 3 (M6/M7 polish), then Stage 4 features per user priority.
- [ ] If first-pixel fails: deep-debug the WebRTC interop. Most common: codec negotiation mismatch and ICE candidate routing.
