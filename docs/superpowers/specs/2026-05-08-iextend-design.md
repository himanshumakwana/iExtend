# iExtend — design spec

_Status: draft for review · 2026-05-08_

iExtend turns an iPad into a wireless second screen for Windows and Linux laptops. The product target is "imperceptibly smooth on Wi-Fi 6E, Sidecar-parity on USB-C": **p50 14 ms end-to-end on Wi-Fi, 10 ms on USB-C, with Apple Pencil pressure/tilt forwarded to the host as a real Wintab/Windows Ink stylus.**

This spec covers the engineering architecture and a single visual deliverable (`iExtend.html` — the Figma-style design canvas mockup).

---

## 1. Scope

**In scope (v1):**

- Extended desktop (iPad as a real second monitor) on Windows 10/11 and Linux (Wayland + X11)
- Mirror mode toggle
- Touch input → mouse / touch
- Apple Pencil Pro / Pencil 2 with pressure + tilt + barrel-button forwarded to the host as a Wintab/Windows Ink stylus
- iPad on-screen keyboard → host keyboard input
- HDR (HEVC Main10 BT.2020 PQ); end-to-end where the host display and iPad both support it
- Wi-Fi 6/6E preferred (best latency); Wi-Fi 5 / 802.11ac supported at the degraded latency tier. USB-C NCM tethered link is the "max-push" alternative path

**Out of scope (v1; tracked for later):**

- Audio routed to iPad
- Multiple iPads as separate monitors simultaneously
- macOS host (Apple already has Sidecar)
- Touch Bar / DEX-style mobile-first reflow

---

## 2. System overview

Three processes, two devices:

- **`iextendd`** — Rust daemon on the host; owns the virtual monitor, capture pipeline, encoder, and WebRTC peer connection. Headless, restarts on Wi-Fi flap.
- **`iextend-tray`** — Rust GUI on the host (Tauri or egui shell); status icon, pairing UI, settings. Talks to `iextendd` over a localhost-only gRPC socket. Can crash and restart without dropping the live session.
- **`iExtend.app`** — Swift / SwiftUI / Metal iPad app. Receives video via WebRTC, decodes via VideoToolbox, renders via CAMetalLayer.

```
┌─────────────────  Laptop (Win/Linux)  ─────────────────┐
│  iextend-tray ──localhost gRPC──► iextendd            │
│                                       │                │
│                    DTLS-SRTP + DataChannels (WebRTC)   │
└───────────────────────────────────────┼────────────────┘
                                        │ Wi-Fi 6/6E or USB-C NCM
┌───────────────────────────────────────┼────────────────┐
│  iExtend.app (iPadOS 17+)             ▼                │
│  ─ VideoToolbox decode → IOSurface-backed CVPixelBuffer│
│  ─ Network.framework + WebRTC.framework (Google build) │
│  ─ Metal compute reproject → CAMetalLayer (ProMotion)  │
└────────────────────────────────────────────────────────┘
```

**Why two host processes:** UI restartability without dropping video. Daemon owns the kernel-mode driver handles; UI is replaceable.

**Why WebRTC even on the same Wi-Fi:** SRTP encryption, transport-CC + GCC congestion control, NACK-based loss recovery, jitter buffers, and FEC are all things we need at 120 Hz and would otherwise rebuild ourselves badly. ICE host-only candidates keep traffic strictly LAN.

**USB-C path** is just another ICE candidate (the iPad shows up as an NCM Ethernet interface). No code-path branch — only the `tethered` flag in telemetry differs.

---

## 3. Latency budget (target, p50)

| Stage | Wi-Fi 6E | USB-C NCM |
|---|---|---|
| Capture (IddCx / PipeWire DMA-BUF) | 1 ms | 1 ms |
| Encode (HEVC, intra-refresh, hw) | 3 ms | 3 ms |
| Packetize + DTLS-SRTP | 1 ms | 1 ms |
| Wire | 3–5 ms | 1–2 ms |
| Adaptive jitter buffer (1-frame target) | 1–2 ms | 0–1 ms |
| Decode (VideoToolbox) | 3 ms | 3 ms |
| Metal reproject + ProMotion latch | 2 ms | 2 ms |
| **Total** | **14 ms** | **10 ms** |

p99 target: ≤ 30 ms Wi-Fi, ≤ 18 ms USB-C.

A12X–A15 iPads (no M-series GPU) cannot run the reprojection shader at full rate; they get a degraded path with **~22 ms p50 / ~40 ms p99 Wi-Fi**, still smooth but not Sidecar-parity.

---

## 4. Capture & virtual display

OS-specific code lives behind a single Rust trait:

```rust
trait DisplaySource {
    fn create_virtual_monitor(&mut self, mode: DisplayMode) -> Result<MonitorHandle>;
    fn capture_frame(&mut self, dst: &mut GpuTexture) -> CaptureResult;
    fn dirty_rects(&self) -> &[Rect];
    fn destroy(self);
}
```

### 4.1 Windows (`DxgiSource`)

- **Virtual monitor:** Indirect Display Driver (IddCx). Signed minifilter `.sys`, registers a virtual GPU + monitor; daemon attaches via the user-mode IddCx callback. WDDM treats it as real — taskbar, snap, multi-DPI all work for free.
- **Capture:** IddCx hands us the swapchain `ID3D11Texture2D` directly. NVENC / QSV / AMF takes the same handle as input. **Zero-copy.**
- **Dirty rects:** IddCx delivers per-frame dirty-rect array.
- **Codesigning:** EV cert + WHQL attestation required for kernel-mode driver. ~$300/yr cert + ~1-week WHQL turnaround per release.

### 4.2 Linux (`EvdiPipewireSource`)

- **Virtual monitor:** `evdi` kernel module (DKMS, GPL). Creates a virtual DRM device user-space can plug `drm_framebuffer` into. Wayland and X11 see it as a real connected monitor. Daemon links `libevdi` (LGPL) — Apache-licensed daemon stays clean.
- **Capture:** Wayland → `org.freedesktop.portal.ScreenCast` PipeWire portal, DMA-BUF handles. X11 → XShm + XDamage.
- **Dirty rects:** PipeWire portal damage regions; XDamage rects.
- **NVIDIA proprietary driver caveat:** does not expose DMA-BUF for NVENC ingest cleanly. We use the CUDA interop path (VRAM→VRAM copy, ~0.3 ms) — not literally zero-copy, but the cost is negligible.
- **Secure-boot users:** detect MOK requirement at install, walk the user through enrolling the evdi module's signing key.

### 4.3 Threading

Capture runs on a dedicated thread pinned to a P-core (Win) / `SCHED_FIFO` (Linux). Owns the GPU texture queue. Encoder thread reads via single-producer single-consumer ring buffer (`crossbeam::ArrayQueue`). No mutex on the hot path. Frame drops surface in telemetry but never block.

---

## 5. Encode & transport

Three Rust crates, one job each:

- `ix-codec` — encoder trait + impls: `NvencHevc`, `NvencAv1`, `QsvHevc`, `AmfHevc`, `VaapiHevc`, `X264UltraLowLatency`.
- `ix-rtc` — WebRTC peer connection, video track, two DataChannels.
- `ix-discover` — mDNS / Bonjour, pair-token verification, candidate exchange.

### 5.1 Codec selection (runtime, in priority)

1. **AV1 hardware** on host **AND** confirmed **M4 iPad Pro** decoder. VideoToolbox AV1 hardware decode landed with the M4 SoC; M1/M2/M3 iPad Pro do not have it. Encoder availability ≠ decoder availability — both ends must advertise AV1 in SDP.
2. **HEVC hardware** — default for ~99% of installs; supported by all M-series and A12+ iPads.
3. **H.264 hardware** — universal fallback; ~30% bitrate cost vs. HEVC.
4. **x264 ultra-low-latency software** — only when no host hardware encoder. Warns user about battery.

Capability is advertised in SDP offer/answer; iPad is the source of truth for what it can decode. Codec switches mid-session are supported but cost **80–150 ms** including SDP renegotiation roundtrip — user-perceptible blip.

### 5.2 Encoder configuration (HEVC, the common case)

- `tune=ultralowlatency`, `lookahead=0`, `b_frames=0`, rate control `cbr_hq`.
- **Intra-refresh, 16-row slice gradient.** No periodic I-frames; instead 1/16 of the picture refreshes per frame. Steady bitrate, no key-frame blast.
- **Adaptive bitrate** 8–80 Mbps driven by transport-CC feedback. Floor at **6 Mbps for 1080p120** before dropping to 60 Hz; below 2 Mbps sustained, suggest USB-C and pause.
- **HDR:** HEVC Main10, BT.2020 PQ. Encoder takes 10-bit input from DXGI on Win11-HDR or PipeWire HDR metadata on Wayland.

### 5.3 WebRTC channels

- **1 video track:** HEVC/AV1, FEC off (intra-refresh + RTX is enough), NACK on, key-frame-on-demand on.
- **DataChannel `input`:** unordered, `maxRetransmits: 0`. Touch / Pencil / keyboard. Loss is fine — next sample is 4 ms away.
- **DataChannel `control`:** reliable, ordered. Mode changes, settings, heartbeat (250 ms cadence; 4 missed = 1 s disconnect detection), latency probes, cursor position broadcast (see §6.4), end-of-session.

### 5.4 ICE strategy

Host candidates only. Same LAN; STUN/TURN would add latency and aren't needed. SDP swapped through the local mDNS service record, signed with the device cert from pairing. Zero external dependencies at runtime.

---

## 6. Input forwarding

### 6.1 Wire format — fixed 32-byte packet

```
┌──────────┬──────────┬──────────┬──────────┐
│ kind (1) │ time (8) │ seq  (4) │ flags(1) │
├──────────┴──────────┴──────────┴──────────┤
│            payload (18 bytes)              │
└────────────────────────────────────────────┘

kind: TOUCH_BEGIN | TOUCH_MOVE | TOUCH_END
      PENCIL_BEGIN | PENCIL_MOVE | PENCIL_END
      KEY_DOWN | KEY_UP | MODIFIER

time: iPad-side mach_absolute_time (µs)
seq:  monotonic per-channel
```

### 6.2 Pencil

- **Sample rate:** Apple Pencil Pro on iPad Pro M2/M4 → 240 Hz raw. Pencil 2 on M1 / A-series → 120 Hz raw + UIKit predicted-touch interpolation we forward as separate predicted samples (host-side filter discards retroactively-superseded predictions).
- **Payload:** `x_q16, y_q16, pressure_q16, tilt_q16, azimuth_q16, twist_q16, buttons, hover_flag` (Q16 fixed-point — packs tighter and the host doesn't lose precision).
- **Coordinates:** iPad logical pixels; host scales to virtual-monitor pixels.

### 6.3 Host-side stylus injection

- **Windows:** Virtual HID Framework (`vhf.sys`) emits `HID_USAGE_DIGITIZER_STYLUS` with full pressure + tilt + barrel-button. **Windows Ink (built-in) is the primary path** — most modern apps (Photoshop, Krita, OneNote, Clip Studio Paint, Affinity) use Ink directly. **Wintab compatibility for legacy apps** ships as our own thin Wintab shim that translates Windows Pointer Input → Wintab API. No Wacom dependency.
- **Linux:** `/dev/uinput` emits `EV_ABS / ABS_PRESSURE / ABS_TILT_X / ABS_TILT_Y`. libinput sees a real Wacom-class tablet; Krita / Inkscape / Blender / GIMP work without per-app config.

### 6.4 Cursor reprojection (the "max-push" piece)

The host cursor is drawn into the captured video frame, so the iPad cannot independently shift it without double-rendering. Real approach:

1. Host emits a **`cursor`** message on the control DataChannel each frame: `(x, y, sprite_id, hotspot)`.
2. Host hints the encoder to render the cursor as a luma-keyed solid sprite the iPad can mask (encoded with a known signature; the iPad blacks it out post-decode).
3. iPad re-renders the cursor sprite on a SwiftUI overlay layer at the **reprojected** position, derived from `(host_cursor_pos, last_input_event, measured_rtt)`.
4. Pencil-tip indicator gets the same treatment — host renders nothing under the tip; iPad draws its own indicator at the predicted iPad-side position.

Net effect: cursor and pencil tip feel glued to the screen even when the underlying frame is 14 ms behind. ~150 lines of Swift + 80 lines of Metal compute (mask shader). A-series iPads disable this and accept the higher perceived input latency.

---

## 7. Pairing & security

### 7.1 First-pair (PIN + SPAKE2 PAKE)

```
1. iPad mDNS-browses _iextend._tcp; finds host.
2. User taps host card → PC shows 4-digit PIN.
3. User types PIN on iPad.
4. SPAKE2 handshake using PIN as the shared secret. PIN never crosses the wire — even encrypted. Offline brute force impossible.
5. Derived key K_pair encrypts a one-shot device-cert exchange:
     iPad → host: pubkey + device name + iPadOS version
     host → iPad: cert{iPad-pubkey, expiry=never, pair-id} signed by host root
6. Both sides pin the other's pubkey (iPad: keychain; host: DPAPI / libsecret).
7. PIN is destroyed.
```

`spake2` crate from RustCrypto on host; `swift-crypto` SPAKE2 on iPad.

### 7.2 Steady state (already paired)

1. iPad mDNS-browses, finds host service record.
2. Opens DTLS connection, presents device cert.
3. Host validates against its pinned-pubkey list.
4. WebRTC PeerConnection comes up; SRTP keys derived from DTLS.
5. All video / input / control authenticated and encrypted.

### 7.3 Revocation

"Forget device" deletes the pinned pubkey on either side. Next reconnect from that iPad is treated as a new pairing.

### 7.4 Threat model and what's explicitly NOT in v1

- Threat: hostile LAN (coffee shop, conference, default-creds router). Out: targeted nation-state attack on a paired host.
- No cloud account, no "iExtend ID," no telemetry-by-default, no login.
- No federated identity. Pairing survives without internet.
- Biometric on iPad (Face ID gate for auto-connect) is opt-in, v1.1.

### 7.5 Replay protection

SRTP handles media. DataChannels: per-channel monotonic seq + 1024-slot sliding window. Reorder OK; replay rejected.

---

## 8. iPad app architecture

iPadOS 17+. Two-tier hardware support:

- **M-series (M1/M2/M4 iPad Pro, M-series Air):** full max-push pipeline, reprojection shader on, 240 Hz Pencil sampling on M2+.
- **A12X–A15 (older iPads):** reprojection off, ProMotion-locked vsync where available, ~22 ms p50.

### 8.1 Modules

```
iExtend.app
├── iExtendKit         // Networking + WebRTC + decode plumbing (no UI)
├── iExtendUI          // SwiftUI screens — onboarding, live, settings
└── iExtendInput       // Touch / Pencil / keyboard event capture + serialization
```

### 8.2 Render pipeline

```
WebRTC frame (CVPixelBuffer, VideoToolbox-decoded)
  → MTLTexture wrapping IOSurface (zero-copy)
  → Metal compute: mask host cursor sprite + render reprojected cursor
  → CAMetalLayer drawable, pinned to ProMotion vsync via CADisplayLink
  → Display
```

`AVSampleBufferDisplayLayer` is convenient but owns its own vsync timing — we cannot synchronize the reprojection compute with display latch. Direct `CAMetalLayer` lets us land the pixel on the next vsync edge precisely.

The live-screen view is one Metal layer. SwiftUI is used only for:

- The floating toolbar (UIViewController overlay)
- Onboarding, discover, pair, settings

### 8.3 State management

`@Observable` classes (Swift macro). Connection state machine ~8 nodes; settings struct; frame-stats struct. No Redux/TCA — overkill for the surface area.

### 8.4 Threading

- Main: SwiftUI / UIKit chrome only.
- `iExtendNet` actor: WebRTC lifecycle, signaling.
- `iExtendDecode` actor: VideoToolbox decode session, frame queue.
- Metal + display: tied to `CADisplayLink` callback on a high-priority dispatch queue.
- Input: touch/pencil events arrive on main thread (UIKit-mandated), serialized inline (~5 µs/event), posted to data-channel send queue.

### 8.5 Background behavior

App backgrounded (home swipe) → `BACKGROUND` on control channel; host pauses encoder, freezes virtual monitor at black, keeps WebRTC alive ~30 s on iOS background time. Foreground within 30 s = instant resume; longer = clean reconnect. (iOS forbids background video decode; not worth fighting.)

### 8.6 Planned file layout

```
iExtendKit/
├── Connection/
│   ├── PeerConnection.swift     (~250 lines)
│   ├── Signaling.swift          (~180 lines, mDNS + cert pinning)
│   └── PairingFlow.swift        (~220 lines, SPAKE2 over Network.framework)
├── Decode/
│   ├── DecodeSession.swift      (~150 lines, VideoToolbox)
│   └── FrameQueue.swift         (~80 lines, lock-free SPSC)
└── Render/
    ├── MetalRenderer.swift      (~200 lines)
    ├── CursorMaskShader.metal   (~80 lines)
    └── ReprojectShader.metal    (~120 lines)
```

Each file does one thing; nothing exceeds 300 lines.

---

## 9. Connection state machine

Single Rust enum (host) and single Swift enum (iPad). Both sides agree via control-channel messages — no inferring state from packet flow.

| State | Trigger | Behavior |
|---|---|---|
| `Idle` | App launched, not paired, or "Disconnect" pressed | mDNS browse only |
| `Pairing` | User picked a device + entered PIN | SPAKE2 handshake; 30 s timeout |
| `Connecting` | Pair done, WebRTC handshake in flight | Connecting overlay; 10 s timeout → `Failed` |
| `Live` | First video frame arrived | Normal operation; 250 ms heartbeat |
| `Degraded` | RTT > 80 ms p95 OR loss > 3% for 2 s OR encoder bitrate floored | Yellow latency badge; offer "drop to 60 Hz"; keep going |
| `Disconnected` | 4 missed heartbeats (1 s) OR DTLS alert | Disconnect overlay; auto-retry 3× with 1/3/10 s backoff |
| `Failed` | Auto-retry exhausted, or unpair, or codec exhausted | Show error; manual recovery only |

Codec fallback chain: AV1 → HEVC → H.264 → 60 Hz HEVC → `Failed`. Each step is a clean keyframe boundary; ~80–150 ms blip.

---

## 10. Testing strategy

### 10.1 Automated

- **Unit tests** — codec selection, state machine transitions, packet parsing, SPAKE2 vector tests from RFC 9382.
- **Integration tests on real hardware** — CI cluster of 4 boxes (Win11 + RTX 4060, Win11 + iGPU, Ubuntu 24 + RX 7600, Ubuntu 24 + iGPU), each tethered USB-C to an iPad Pro M2. 60 s smoke + 5 min soak per stack, nightly + per release branch.
- **Synthetic latency CI test** — WebRTC loopback (host encodes its own timecoded frame, iPad decodes, sends back via DataChannel, host computes round-trip). Coarse-grained but catches regressions; fails build if budget regresses by > 2 ms.

### 10.2 Bench-rig (manual / weekly)

- **External 240 fps camera** (iPhone slo-mo or dedicated bench rig) pointed at both host monitor and iPad screen simultaneously. OCRs both timecodes, computes true wall-clock photon-to-photon delta. The synthetic CI test can't measure this end-to-end; only physical optics can.
- **8-hour soak** before every release. Charts: latency p50/p95/p99, encoder bitrate, dropped frames, reconnects. Bar: zero unrecovered disconnects in 8 h, p99 stays under target.

### 10.3 Pencil feel test (subjective, structured)

10 reviewers, 3 apps each (Photoshop, Procreate via Sidecar reference, Krita). 5-point Likert scale on stroke smoothness, latency, pressure response. Bar: ≥ 4.0 average; no individual app < 3.5.

---

## 11. Observability

- **iPad rolling-window stats panel** — already in design (latency sparkline in Settings). Live RTT, jitter, codec, bitrate, frames decoded/dropped.
- **Host log file** — JSON-line, 100 MB rotation, local only.
- **Crash reports** — opt-in, self-hosted Sentry. Panic backtrace + codec/GPU/OS versions only. No PII.
- **No telemetry by default.** Anything that leaves the device is explicit user opt-in.

---

## 12. iExtend.html — visual deliverable

A single self-contained HTML page rendering the full Figma-style design canvas — 15 artboards across 4 sections (Onboarding, Connected, Settings & errors, Toolbar variants) with the live Tweaks panel (dark mode, toolbar position, density, connection state).

**Deliverable rules:**

- Single `iExtend.html` at the repo root.
- Inlines all 7 source files from the Claude Design bundle (`design-canvas.jsx`, `tweaks-panel.jsx`, `frames.jsx`, `icons.jsx`, `scenes-ipad.jsx`, `scenes-pc.jsx`, `app.jsx`) into one document. No separate `.jsx` fetches; no CORS surface.
- React + Babel from unpkg with the integrity hashes from the bundle.
- Pan, zoom, drag-reorder, fullscreen-focus, tweaks panel — all preserved.
- `window.omelette?.writeFile` calls inside `design-canvas.jsx` no-op gracefully without the host bridge — no behavior change needed.
- Source: copy verbatim from the design bundle. We do **not** rebuild visually — Claude Design produced it pixel-perfectly already; our job is mechanical inlining + light cleanup.

This page is what someone opens to see the whole product visually before any Rust/Swift code exists. It is **not** the runtime application — it's the design artifact.

---

## 13. Repo layout (planned)

```
iExtend/
├── iExtend.html                        # the visual deliverable (§12)
├── docs/
│   └── superpowers/specs/              # this spec
├── host/                               # Rust workspace
│   ├── crates/
│   │   ├── iextendd/                   # daemon
│   │   ├── iextend-tray/               # GUI shell
│   │   ├── ix-codec/                   # encoder trait + impls
│   │   ├── ix-rtc/                     # WebRTC peer
│   │   ├── ix-discover/                # mDNS + pair
│   │   ├── ix-display-windows/         # IddCx + DXGI
│   │   ├── ix-display-linux/           # evdi + PipeWire
│   │   └── ix-input/                   # vhf.sys + uinput injection
│   └── installer/
│       ├── windows/                    # MSIX + signed driver
│       └── linux/                      # .deb + .rpm + DKMS
└── ipad/                               # Swift package + Xcode project
    ├── iExtendKit/
    ├── iExtendUI/
    ├── iExtendInput/
    └── iExtend.xcodeproj
```

---

## 14. Open questions

None blocking. Items deferred to v1.1 or later, in priority:

1. Audio routing to iPad (virtual audio device on host).
2. Multi-iPad sessions.
3. Biometric (Face ID) gate for auto-connect.
4. Frame pacing with full motion-to-photon prediction (not just cursor reprojection).
5. macOS host (Apple already has Sidecar, so low priority).

---

## 15. References

- Claude Design bundle (visual source for §12): hash `-2njFxK8daApykJmGf-3jg`
- IddCx: `learn.microsoft.com/en-us/windows-hardware/drivers/display/indirect-display-driver-model-overview`
- evdi: `github.com/DisplayLink/evdi`
- WebRTC HEVC RTP payload: RFC 7798
- WebRTC AV1 RTP payload: RFC 9335
- SPAKE2: RFC 9382
