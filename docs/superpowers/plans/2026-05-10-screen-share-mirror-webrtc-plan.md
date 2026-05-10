# Screen Share — Mirror Mode over WebRTC (Plan A)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stream the host's primary display to the iPad over WebRTC at 30fps with sub-100ms glass-to-glass latency. Mirror mode only — Extend mode (virtual display driver) is deferred to Plan B.

**Architecture:** DXGI Desktop Duplication captures host frames → OpenH264 software encoder produces Annex-B NAL units → WebRTC RTPSender wraps them as RTP packets → DTLS-SRTP transport → iPad's RTPReceiver → VideoToolbox decoder → AVSampleBufferDisplayLayer renders to screen.

**Tech Stack:** Rust (host capture + encode + WebRTC), `webrtc-rs` crate, OpenH264 software encoder, Swift / WebRTC.framework (iPad), `AVSampleBufferDisplayLayer` for decode rendering.

**Spec/context:** Original Plan 5 (`docs/superpowers/plans/2026-05-08-plan-5-webrtc-codec.md`) covers the full architecture; this plan is the executable slice that gets to first-pixel.

---

## Milestone M1: Host capture produces YUV420P frames

**Files:**
- Modify: `host/crates/ix-display-windows/src/frame_pump.rs`
- Modify: `host/crates/ix-display-windows/src/lib.rs`
- Modify: `host/crates/ix-display-windows/tests/dxgi_capture_smoke.rs` (new)

- [ ] **Step 1:** Read existing scaffold in `frame_pump.rs` and `iddcx_bindings.rs` to understand what's there. Report which DXGI APIs are already bound vs. need adding.
- [ ] **Step 2:** Wire `IDXGIOutputDuplication::AcquireNextFrame` into a tokio task that pumps `Frame { yuv420p: Vec<u8>, width: u32, height: u32, pts_us: u64 }` into an `mpsc::channel`. Convert from BGRA via libyuv-rs (add to Cargo.toml).
- [ ] **Step 3:** Add a smoke test that runs the capture for 1 second and asserts at least 20 frames arrived. Skip on non-Windows.
- [ ] **Step 4:** Commit. `feat(ix-display-windows): wire DXGI capture into mpsc channel`.

**Done when:** running `cargo test -p ix-display-windows --target x86_64-pc-windows-msvc` (in CI) prints "captured 30 frames at avg 30.1 fps".

---

## Milestone M2: Encoder produces decodable NALs

**Files:**
- Modify: `host/crates/ix-codec/src/x264_sw.rs` (already scaffolded — wire the actual OpenH264 calls)
- Modify: `host/crates/ix-codec/Cargo.toml` (add `openh264` crate)
- Create: `host/crates/ix-codec/tests/encode_decode_roundtrip.rs`

- [ ] **Step 1:** Replace `x264_sw.rs` scaffold with a real `OpenH264Encoder` that takes `Frame { yuv420p, w, h, pts }` and returns `Vec<NalUnit>`. Use OpenH264's `usage_type = SCREEN_CONTENT_REAL_TIME` + `rate_control_mode = RC_BITRATE_MODE` + target bitrate from settings.
- [ ] **Step 2:** Roundtrip test: feed a synthetic frame through the encoder, decode the output NALs back via OpenH264's decoder API, assert PSNR > 35 dB. This is plan-validation, not just a smoke test.
- [ ] **Step 3:** Commit. `feat(ix-codec): real OpenH264 software encoder + roundtrip test`.

**Done when:** roundtrip test passes and the NALs hex-dump shows valid SPS/PPS/IDR sequence.

---

## Milestone M3: Signaling channel between daemon and iPad

**Files:**
- Modify: `host/proto/iextend.proto` — add `Signaling` service with `Negotiate` bi-di stream
- Create: `host/crates/iextendd/src/signaling.rs`
- Create: `ipad/iExtendKit/Sources/iExtendKit/Connection/SignalingClient.swift`

- [ ] **Step 1:** Define a 2nd gRPC service `Signaling` with one bidirectional streaming RPC `Negotiate(stream SignalMsg) returns (stream SignalMsg)`. SignalMsg has variants: `Offer`, `Answer`, `IceCandidate`, `Bye`. Use a fresh listener on the daemon (port 7782 by default) so iPad clients connect after pair without going through the tray UDS.
- [ ] **Step 2:** Daemon side: `signaling.rs` accepts a Negotiate stream, validates the iPad's pubkey against PinStore (only paired iPads can signal), then forwards SignalMsgs to/from the WebRTC peer connection.
- [ ] **Step 3:** iPad side: `SignalingClient.swift` opens a gRPC stream to the daemon's port 7782 over the same transport (Wi-Fi for now), sends/receives SignalMsg.
- [ ] **Step 4:** Smoke test: integration test on host that simulates an iPad client sending an Offer and asserting the daemon's stream returns an Answer.
- [ ] **Step 5:** Commit. `feat(daemon): WebRTC signaling RPC for SDP/ICE exchange`.

**Done when:** integration test exchanges a fake offer and gets a fake answer back through the gRPC stream.

---

## Milestone M4: WebRTC peer connection negotiated end-to-end

**Files:**
- Modify: `host/crates/ix-rtc/src/peer.rs` (existing scaffold — wire `webrtc-rs`)
- Modify: `host/crates/ix-rtc/Cargo.toml` (add `webrtc = "0.10"`)
- Modify: `ipad/iExtendKit/Sources/iExtendKit/Connection/PeerConnection.swift` (existing — wire actual WebRTC.framework calls)

- [ ] **Step 1:** Host `peer.rs`: wire `webrtc-rs::api::APIBuilder` to create an RTCPeerConnection. Add the encoder's NAL output as an RTP track via `TrackLocalStaticSample`. Bind ICE candidates to the signaling channel.
- [ ] **Step 2:** iPad `PeerConnection.swift`: replace stubs with `RTCPeerConnectionFactory` calls, add a video receiver, route incoming frames to a frame queue.
- [ ] **Step 3:** Run end-to-end on a single machine: daemon binds, iPad simulator (or real device on same Wi-Fi) opens signaling → exchange SDP → DTLS handshake completes → connection state hits `Connected` on both sides.
- [ ] **Step 4:** Commit. `feat(rtc): wire webrtc-rs peer connection end-to-end`.

**Done when:** both daemon and iPad log `"connection state: Connected"` after running for 5 seconds.

---

## Milestone M5: First pixel on iPad ⭐

**Files:**
- Modify: `ipad/iExtendKit/Sources/iExtendKit/Render/MetalRenderer.swift` (or new `SimpleVideoLayer.swift`)
- Modify: `ipad/iExtendUI/Sources/iExtendUI/Live/LiveView.swift`

- [ ] **Step 1:** iPad: route incoming WebRTC video frames to an `AVSampleBufferDisplayLayer` (no Metal needed for v1). Layer is sized to the screen and added to LiveView.
- [ ] **Step 2:** Daemon: wire the capture → encode → peer-connection pipeline together so a connected peer starts receiving real frames.
- [ ] **Step 3:** Manual test: laptop screen visible on iPad's LiveView. Smoke test: one frame received within 5 seconds of session start.
- [ ] **Step 4:** Commit. `feat(ipad): render received WebRTC frames to AVSampleBufferDisplayLayer`.

**Done when:** you see your laptop's screen on the iPad. This is THE milestone.

---

## Milestone M6: Sustained 30fps Mirror

- [ ] Tune encoder parameters (GOP, bitrate, B-frames) for low-latency screen content.
- [ ] Add stats overlay on iPad (fps, ms, kbps).
- [ ] Verify 60-second sustained 30fps without dropouts.
- [ ] Commit. `perf(rtc): sustain 30fps + add stats overlay`.

**Done when:** iPad shows 30 fps reliably for 60 seconds with the existing latency budgets met.

---

## Milestone M7: Adaptive bitrate + codec fallback

- [ ] Wire `ix-rtc/bitrate_controller.rs` to actually adjust the encoder bitrate based on RTCP feedback.
- [ ] If OpenH264 is too slow, fall back to lower resolution (720p → 480p).
- [ ] Tray UI shows current codec/bitrate.
- [ ] Commit. `feat(rtc): adaptive bitrate via RTCP feedback`.

**Done when:** simulating bandwidth drop (use `tc qdisc` on Linux or NetLimiter on Windows) drops bitrate gracefully without freezing.

---

## Out of scope (Plan B / future)

- Extend mode (IddCx virtual display driver)
- Linux capture (PipeWire / DRM)
- HDR pipeline
- Hardware encoders (NVENC, AMF, QSV) — software-first
- Audio routing
- Multi-iPad
