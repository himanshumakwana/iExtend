# Plan 6 of 10 — iPad Swift App Shell

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the iExtend iPad app shell — a single iPadOS 17+ target organized as three Swift Package Manager modules (`iExtendKit`, `iExtendUI`, `iExtendInput`) that together can pair with a Plan 5 host, decode an HEVC video stream over WebRTC, render it at 120 Hz on a `CAMetalLayer`, and capture touch/Pencil events ready to be sent back. Reprojection (Plan 8) and SPAKE2 (Plan 7) are stubbed; the shell is wired to call them at the right places so those plans drop in cleanly.

**Architecture:** One Xcode project (`ipad/iExtend.xcodeproj`) with three local Swift Package Manager subprojects, linked as targets of a single iPadOS app. The app launches into `iExtendUI` (SwiftUI), which owns an `IExtendSession` actor from `iExtendKit`. The session owns the connection state machine and exposes a `@Observable` view-model for SwiftUI to bind. The Live screen is special: it hosts a `UIViewControllerRepresentable` whose view is a bare `CAMetalLayer` driven by `CADisplayLink`, with a SwiftUI overlay for the floating toolbar.

**Tech Stack:**
- Swift 5.10+ (Swift 6 strict concurrency for new code, sticking with `actor`-isolation, no global mutable state)
- iPadOS 17 SDK; iPadOS 17 deployment target
- Xcode 15.4+ (project format 56)
- Swift Package Manager (local packages, not remote — keeps build hermetic)
- Google's WebRTC.framework as an `XCFramework` binary checked in at `ipad/Frameworks/WebRTC.xcframework` (pinned to a specific GitHub release)
- VideoToolbox + Metal + CoreVideo + CoreMedia (system frameworks)
- Network.framework (`NWBrowser` for mDNS discovery; `NWConnection` only for synthetic loopback test in Task 10)
- swift-atomics 1.2.0 (lock-free SPSC ring buffer)
- swift-crypto 3.2.0 (used by Plan 7 for SPAKE2; pulled in here so the dependency closure is settled)

**Plan scope:** This is **Plan 6 of 10**. Independent of Plans 2–5 in pure code terms (the iPad app talks to anything that speaks our control protocol), but Task 10 below requires Plan 5 to have produced `smoke_loopback.rs` to talk to. If Plan 5 isn't done yet, Task 10 reduces to a unit test against a Swift-side mock peer.

**Out of scope for this plan (handled elsewhere):**
- Plan 7: SPAKE2 PIN-based pairing handshake. `PairingFlow.swift` here is a placeholder that accepts any 4-digit PIN and pretends to derive a key.
- Plan 8: Cursor reprojection compute pass + Apple Pencil pressure/tilt forwarding payload. `MetalRenderer.swift` here just blits the decoded `MTLTexture` to the drawable. `EventCapture.swift` captures the events and serializes the packet, but the wire format payload variant for `PENCIL_*` packets uses a stub Q16 encoder until Plan 8 finalizes the bit layout.
- Plan 9: Code signing, provisioning profiles, App Store Connect / TestFlight.

---

## Source-of-truth references

While implementing, keep these open:

- **Spec §8** — the iPad app architecture section. Threading model in §8.4 is mandatory.
- **Spec §6** — input wire format (32-byte fixed packet); §6.1 is canonical.
- **Spec §9** — connection state machine. Names and trigger conditions must match.
- **`iExtend.html`** in the repo root — open this in Safari at `http://localhost:8080/iExtend.html` (run `npm run view`) and pin it side-by-side. Each SwiftUI screen this plan creates must visually match a specific artboard. The artboard label is given in the task description.

---

## File Structure

This plan creates the following tree under `/home/tops/Projects/iExtend/ipad/`:

```
ipad/
├── iExtend.xcodeproj/                        # Xcode project (Task 1)
│   └── project.pbxproj
├── iExtend/                                  # the iPadOS app target (Task 1)
│   ├── iExtendApp.swift                      # @main App, root NavigationStack
│   ├── Info.plist
│   ├── Assets.xcassets/
│   │   ├── AppIcon.appiconset/
│   │   └── AccentColor.colorset/
│   └── Preview Content/
├── Frameworks/
│   └── WebRTC.xcframework/                   # binary, ~280 MB, see Task 2
├── iExtendKit/                               # SPM package (Task 1)
│   ├── Package.swift
│   ├── Sources/iExtendKit/
│   │   ├── IExtendSession.swift              # the public actor + state machine (~180 lines)
│   │   ├── ConnectionState.swift             # enum mirroring spec §9 (~60 lines)
│   │   ├── Settings.swift                    # @Observable settings struct (~60 lines)
│   │   ├── FrameStats.swift                  # @Observable rolling stats (~80 lines)
│   │   ├── Connection/
│   │   │   ├── PeerConnection.swift          # wraps RTCPeerConnection (~250 lines)
│   │   │   ├── Signaling.swift               # mDNS browse + cert pinning (~180 lines)
│   │   │   ├── PairingFlow.swift             # placeholder; Plan 7 fills in (~220 lines)
│   │   │   └── ControlChannel.swift          # heartbeat + control msg dispatch (~100 lines)
│   │   ├── Decode/
│   │   │   ├── DecodeSession.swift           # VTDecompressionSession (~150 lines)
│   │   │   ├── FrameQueue.swift              # lock-free SPSC (~80 lines)
│   │   │   └── CodecCaps.swift               # AV1 M4 probe, HEVC always-on (~50 lines)
│   │   └── Render/
│   │       ├── MetalRenderer.swift           # CAMetalLayer + CADisplayLink (~200 lines)
│   │       └── BlitShader.metal              # minimum-viable identity shader (~30 lines)
│   └── Tests/iExtendKitTests/
│       ├── StateMachineTests.swift           # state transitions per spec §9
│       ├── FrameQueueTests.swift             # SPSC under contention
│       └── CodecCapsTests.swift              # M4 detection logic
├── iExtendUI/                                # SPM package (Task 1)
│   ├── Package.swift
│   ├── Sources/iExtendUI/
│   │   ├── RootView.swift                    # NavigationStack switching on session state (~80 lines)
│   │   ├── Theme.swift                       # iPadOS color tokens — system blue, indigo, etc. (~60 lines)
│   │   ├── Onboarding/
│   │   │   ├── WelcomeView.swift             # "iPad · Welcome" artboard (~220 lines)
│   │   │   ├── DiscoverView.swift            # "iPad · Discover" artboard (~220 lines)
│   │   │   └── PairView.swift                # "iPad · Pair" artboard (~250 lines)
│   │   ├── Live/
│   │   │   ├── LiveView.swift                # "iPad · Live (extended)" — Metal host (~120 lines)
│   │   │   ├── MetalLayerHost.swift          # UIViewControllerRepresentable (~80 lines)
│   │   │   ├── FloatingToolbar.swift         # glass pill toolbar (~180 lines)
│   │   │   ├── ConnectingOverlay.swift       # "iPad · Connecting" artboard (~80 lines)
│   │   │   └── DisconnectedOverlay.swift     # "iPad · Disconnected" artboard (~120 lines)
│   │   ├── Settings/
│   │   │   ├── SettingsView.swift            # "iPad · Settings" sidebar host (~140 lines)
│   │   │   ├── ConnectionPane.swift          # Connection group (~150 lines)
│   │   │   ├── DisplayPane.swift             # Resolution / Scaling / Color / HDR (~120 lines)
│   │   │   ├── PencilTouchPane.swift         # placeholder (~60 lines)
│   │   │   ├── PerformancePane.swift         # placeholder (~60 lines)
│   │   │   └── DiagnosticsPane.swift         # placeholder (~60 lines)
│   │   └── Components/
│   │       ├── PillButton.swift              # primary/ghost CTA pill (~60 lines)
│   │       ├── DeviceRow.swift               # used by DiscoverView (~80 lines)
│   │       ├── PINBoxes.swift                # used by PairView (~80 lines)
│   │       ├── LatencySpark.swift            # SwiftUI shape sparkline (~80 lines)
│   │       └── Toggle.swift                  # iOS-style switch matching spec (~30 lines)
│   └── Tests/iExtendUITests/
│       ├── SnapshotTests.swift               # one snapshot per artboard
│       └── StateBindingTests.swift
├── iExtendInput/                             # SPM package (Task 1)
│   ├── Package.swift
│   ├── Sources/iExtendInput/
│   │   ├── EventCapture.swift                # UITouch / UIPencilInteraction / UIKey capture (~200 lines)
│   │   ├── PacketEncoder.swift               # 32-byte packet builder + Q16 helpers (~120 lines)
│   │   ├── EventKind.swift                   # enum mirroring spec §6.1 (~30 lines)
│   │   └── EventSink.swift                   # protocol the Kit DataChannel implements (~30 lines)
│   └── Tests/iExtendInputTests/
│       ├── PacketEncoderTests.swift          # Q16 round-trip; 32-byte length invariant
│       └── EventCaptureTests.swift           # UIKit-event → packet mapping
└── README.md                                 # iPad-side dev workflow (~80 lines)
```

**Why this structure:**
- Three SPM packages keep concerns separate and force the dependency direction `App → UI → Kit + Input`. UI never imports Input directly; Input never imports UI; Kit never imports either of the others.
- Files are sized to fit comfortably in a context window (~80–250 lines each). Anything growing past 300 is a signal to split.
- Tests live next to the package, not in the app target, so the Kit + Input packages can be unit-tested without booting the full app.

---

### Task 1: Bootstrap the Xcode project + three SPM packages

**Files:**
- Create: `/home/tops/Projects/iExtend/ipad/iExtend.xcodeproj/project.pbxproj`
- Create: `/home/tops/Projects/iExtend/ipad/iExtend/iExtendApp.swift`
- Create: `/home/tops/Projects/iExtend/ipad/iExtend/Info.plist`
- Create: `/home/tops/Projects/iExtend/ipad/iExtendKit/Package.swift`
- Create: `/home/tops/Projects/iExtend/ipad/iExtendUI/Package.swift`
- Create: `/home/tops/Projects/iExtend/ipad/iExtendInput/Package.swift`
- Create: `/home/tops/Projects/iExtend/ipad/README.md`

- [ ] **Step 1: Create the directory tree**

```bash
cd /home/tops/Projects/iExtend
mkdir -p ipad/iExtend/{Assets.xcassets/AppIcon.appiconset,Assets.xcassets/AccentColor.colorset,Preview\ Content}
mkdir -p ipad/iExtendKit/Sources/iExtendKit/{Connection,Decode,Render}
mkdir -p ipad/iExtendKit/Tests/iExtendKitTests
mkdir -p ipad/iExtendUI/Sources/iExtendUI/{Onboarding,Live,Settings,Components}
mkdir -p ipad/iExtendUI/Tests/iExtendUITests
mkdir -p ipad/iExtendInput/Sources/iExtendInput
mkdir -p ipad/iExtendInput/Tests/iExtendInputTests
mkdir -p ipad/Frameworks
```

- [ ] **Step 2: Write `iExtendKit/Package.swift`**

Create `/home/tops/Projects/iExtend/ipad/iExtendKit/Package.swift`:

```swift
// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "iExtendKit",
    platforms: [.iOS(.v17)],
    products: [
        .library(name: "iExtendKit", targets: ["iExtendKit"]),
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-atomics.git", from: "1.2.0"),
        .package(url: "https://github.com/apple/swift-crypto.git", from: "3.2.0"),
    ],
    targets: [
        .target(
            name: "iExtendKit",
            dependencies: [
                .product(name: "Atomics", package: "swift-atomics"),
                .product(name: "Crypto", package: "swift-crypto"),
                "WebRTC",
            ],
            resources: [.process("Render/BlitShader.metal")],
            swiftSettings: [.enableExperimentalFeature("StrictConcurrency")]
        ),
        .binaryTarget(
            name: "WebRTC",
            path: "../Frameworks/WebRTC.xcframework"
        ),
        .testTarget(name: "iExtendKitTests", dependencies: ["iExtendKit"]),
    ]
)
```

- [ ] **Step 3: Write `iExtendUI/Package.swift`**

Create `/home/tops/Projects/iExtend/ipad/iExtendUI/Package.swift`:

```swift
// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "iExtendUI",
    platforms: [.iOS(.v17)],
    products: [
        .library(name: "iExtendUI", targets: ["iExtendUI"]),
    ],
    dependencies: [
        .package(path: "../iExtendKit"),
    ],
    targets: [
        .target(
            name: "iExtendUI",
            dependencies: ["iExtendKit"],
            swiftSettings: [.enableExperimentalFeature("StrictConcurrency")]
        ),
        .testTarget(name: "iExtendUITests", dependencies: ["iExtendUI"]),
    ]
)
```

- [ ] **Step 4: Write `iExtendInput/Package.swift`**

Create `/home/tops/Projects/iExtend/ipad/iExtendInput/Package.swift`:

```swift
// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "iExtendInput",
    platforms: [.iOS(.v17)],
    products: [
        .library(name: "iExtendInput", targets: ["iExtendInput"]),
    ],
    targets: [
        .target(
            name: "iExtendInput",
            swiftSettings: [.enableExperimentalFeature("StrictConcurrency")]
        ),
        .testTarget(name: "iExtendInputTests", dependencies: ["iExtendInput"]),
    ]
)
```

- [ ] **Step 5: Write `iExtendApp.swift`**

Create `/home/tops/Projects/iExtend/ipad/iExtend/iExtendApp.swift`:

```swift
import SwiftUI
import iExtendUI

@main
struct iExtendApp: App {
    var body: some Scene {
        WindowGroup {
            RootView()
        }
        .defaultSize(width: 1194, height: 834) // iPad Pro 11" landscape
    }
}
```

- [ ] **Step 6: Write `Info.plist`**

Create `/home/tops/Projects/iExtend/ipad/iExtend/Info.plist` with these keys:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDisplayName</key><string>iExtend</string>
    <key>CFBundleIdentifier</key><string>com.iextend.app</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>CFBundleShortVersionString</key><string>0.1.0</string>
    <key>UISupportedInterfaceOrientations~ipad</key>
    <array>
        <string>UIInterfaceOrientationLandscapeLeft</string>
        <string>UIInterfaceOrientationLandscapeRight</string>
    </array>
    <key>UIRequiredDeviceCapabilities</key>
    <array><string>arm64</string><string>metal</string></array>
    <key>UIRequiresFullScreen</key><true/>
    <key>NSBonjourServices</key>
    <array><string>_iextend._tcp</string></array>
    <key>NSLocalNetworkUsageDescription</key>
    <string>iExtend uses your local Wi-Fi to discover and connect to your computer.</string>
    <key>NSCameraUsageDescription</key>
    <string>Scan a QR code shown by your computer to pair iExtend.</string>
</dict>
</plist>
```

- [ ] **Step 7: Generate the `.xcodeproj` with XcodeGen**

The simplest reproducible way to create an Xcode project that links three local SPM packages plus a binary `XCFramework` is XcodeGen. Install if needed:

```bash
brew install xcodegen
```

Create `/home/tops/Projects/iExtend/ipad/project.yml`:

```yaml
name: iExtend
options:
  bundleIdPrefix: com.iextend
  deploymentTarget:
    iOS: "17.0"
  developmentLanguage: en
packages:
  iExtendKit:    { path: iExtendKit }
  iExtendUI:     { path: iExtendUI }
  iExtendInput:  { path: iExtendInput }
targets:
  iExtend:
    type: application
    platform: iOS
    sources: [ iExtend ]
    info:
      path: iExtend/Info.plist
    settings:
      base:
        TARGETED_DEVICE_FAMILY: "2"   # iPad only
        SWIFT_VERSION: "5.10"
        DEVELOPMENT_TEAM: ""           # set in Plan 9
    dependencies:
      - package: iExtendKit
      - package: iExtendUI
      - package: iExtendInput
```

Then:

```bash
cd /home/tops/Projects/iExtend/ipad
xcodegen generate
```

- [ ] **Step 8: Verify the workspace builds (without WebRTC yet — it's loaded in Task 2)**

Stub the WebRTC binary target out for this step by commenting the binary target and the `"WebRTC"` dependency in `iExtendKit/Package.swift`. Then:

```bash
cd /home/tops/Projects/iExtend/ipad
xcodebuild -project iExtend.xcodeproj -scheme iExtend -sdk iphonesimulator -destination 'platform=iOS Simulator,name=iPad Pro 11-inch (M4)' build
```

Expected: build succeeds with the empty app launching to a blank `RootView` (which we'll fill in Task 8).

Re-enable the WebRTC target after Task 2 is done.

- [ ] **Step 9: Write `ipad/README.md`**

Create `/home/tops/Projects/iExtend/ipad/README.md`:

```markdown
# iExtend iPad app

iPadOS 17+ client. Three SPM packages: `iExtendKit` (no UI), `iExtendUI` (SwiftUI), `iExtendInput` (event capture).

## Build

```bash
brew install xcodegen
cd ipad
xcodegen generate
open iExtend.xcodeproj
```

Pick an iPad Pro 11" simulator or a physical device (signing setup in Plan 9).

## WebRTC binary

`Frameworks/WebRTC.xcframework` is the Google build, pinned. See Task 2 of Plan 6 to refresh it.

## Tests

```bash
cd ipad
xcodebuild test -project iExtend.xcodeproj -scheme iExtendKit -destination 'platform=iOS Simulator,name=iPad Pro 11-inch (M4)'
xcodebuild test -project iExtend.xcodeproj -scheme iExtendUI -destination 'platform=iOS Simulator,name=iPad Pro 11-inch (M4)'
xcodebuild test -project iExtend.xcodeproj -scheme iExtendInput -destination 'platform=iOS Simulator,name=iPad Pro 11-inch (M4)'
```
```

- [ ] **Step 10: Commit**

```bash
cd /home/tops/Projects/iExtend
git add ipad/
git commit -m "chore(ipad): bootstrap Xcode project + three SPM packages"
```

---

### Task 2: Pin Google's WebRTC.framework

**Files:**
- Add: `/home/tops/Projects/iExtend/ipad/Frameworks/WebRTC.xcframework/` (binary, ~280 MB)
- Add: `/home/tops/Projects/iExtend/ipad/Frameworks/WebRTC.version` (text, the pinned tag)

WebRTC.framework is too large to live in a normal git repo. Use Git LFS, but only for files under `ipad/Frameworks/`.

- [ ] **Step 1: Initialize Git LFS for the Frameworks path**

```bash
cd /home/tops/Projects/iExtend
git lfs install
git lfs track "ipad/Frameworks/**"
git add .gitattributes
```

- [ ] **Step 2: Pick a pinned WebRTC release**

Use the Stasel/WebRTC pre-built XCFrameworks. As of writing, the current pin is **M127** (commit hash recorded in `WebRTC.version`).

```bash
cd /home/tops/Projects/iExtend/ipad/Frameworks
curl -L -o WebRTC.xcframework.zip \
  https://github.com/stasel/WebRTC/releases/download/127.0.0/WebRTC-M127.xcframework.zip
unzip WebRTC.xcframework.zip
rm WebRTC.xcframework.zip
echo "M127 (stasel/WebRTC release 127.0.0)" > WebRTC.version
```

If the URL has rotated, find the latest M-release at `https://github.com/stasel/WebRTC/releases` and update `WebRTC.version` accordingly.

- [ ] **Step 3: Re-enable the binary target in `iExtendKit/Package.swift`**

Uncomment the `.binaryTarget` and `"WebRTC"` dependency you stubbed in Task 1 Step 8.

- [ ] **Step 4: Sanity-build and import-test**

```bash
cd /home/tops/Projects/iExtend/ipad
xcodebuild -project iExtend.xcodeproj -scheme iExtend -sdk iphonesimulator -destination 'platform=iOS Simulator,name=iPad Pro 11-inch (M4)' build
```

Then add a one-line throwaway test in `iExtendKitTests/CodecCapsTests.swift`:

```swift
import XCTest
import WebRTC
@testable import iExtendKit

final class WebRTCImportTest: XCTestCase {
    func testWebRTCFrameworkLinks() {
        let factory = RTCPeerConnectionFactory()
        XCTAssertNotNil(factory)
    }
}
```

Run the iExtendKit test scheme; the test must pass.

- [ ] **Step 5: Commit**

```bash
git add ipad/Frameworks/ ipad/iExtendKit/
git commit -m "feat(ipad): pin Google WebRTC.framework M127 via Git LFS"
```

---

### Task 3: `IExtendSession` actor with state-machine stubs

**Files:**
- Create: `ipad/iExtendKit/Sources/iExtendKit/IExtendSession.swift`
- Create: `ipad/iExtendKit/Sources/iExtendKit/ConnectionState.swift`
- Create: `ipad/iExtendKit/Sources/iExtendKit/Settings.swift`
- Create: `ipad/iExtendKit/Sources/iExtendKit/FrameStats.swift`
- Test: `ipad/iExtendKit/Tests/iExtendKitTests/StateMachineTests.swift`

- [ ] **Step 1: Write the failing state-machine test**

Create `iExtendKitTests/StateMachineTests.swift`:

```swift
import XCTest
@testable import iExtendKit

final class StateMachineTests: XCTestCase {
    func testInitialStateIsIdle() async {
        let session = IExtendSession()
        let state = await session.state
        XCTAssertEqual(state, .idle)
    }

    func testIdleToPairingTransition() async throws {
        let session = IExtendSession()
        try await session.beginPairing(host: .stub("test-host"))
        let state = await session.state
        XCTAssertEqual(state, .pairing)
    }

    func testPairingTimesOutAfter30Seconds() async throws {
        // Fast-clock the timer; see Step 5 for how the clock is injected.
        let clock = TestClock()
        let session = IExtendSession(clock: clock)
        try await session.beginPairing(host: .stub("test-host"))
        await clock.advance(by: .seconds(31))
        let state = await session.state
        XCTAssertEqual(state, .failed(reason: .pairingTimeout))
    }
}
```

Run: `xcodebuild test -project iExtend.xcodeproj -scheme iExtendKit -destination 'platform=iOS Simulator,name=iPad Pro 11-inch (M4)' -only-testing:iExtendKitTests/StateMachineTests`

Expected: FAIL — types don't exist.

- [ ] **Step 2: Write `ConnectionState.swift`**

Mirror spec §9 exactly:

```swift
import Foundation

public enum ConnectionState: Equatable, Sendable {
    case idle
    case pairing
    case connecting
    case live
    case degraded(reason: DegradeReason)
    case disconnected(retriesRemaining: Int)
    case failed(reason: FailureReason)

    public enum DegradeReason: String, Sendable { case highRTT, packetLoss, bitrateFloored }

    public enum FailureReason: Equatable, Sendable {
        case pairingTimeout
        case connectingTimeout
        case retriesExhausted
        case unpaired
        case codecExhausted
    }
}
```

- [ ] **Step 3: Write `Settings.swift`**

```swift
import Foundation
import Observation

@Observable
public final class Settings: @unchecked Sendable {
    public var preferredMode: Mode = .extend
    public var resolution: Resolution = .fullHD120
    public var hdrEnabled: Bool = false
    public var autoConnectOnLaunch: Bool = false

    public enum Mode: String, CaseIterable, Sendable { case extend, mirror, drawingTablet }
    public enum Resolution: String, CaseIterable, Sendable {
        case fullHD120 = "1920x1080@120"
        case wuxga120  = "1920x1200@120"
        case qhd60     = "2560x1440@60"
    }
    public init() {}
}
```

- [ ] **Step 4: Write `FrameStats.swift`**

```swift
import Foundation
import Observation

@Observable
public final class FrameStats: @unchecked Sendable {
    public private(set) var rttMs: Double = 0
    public private(set) var lossPct: Double = 0
    public private(set) var bitrateMbps: Double = 0
    public private(set) var fps: Double = 0
    public private(set) var rttHistory: [Double] = []   // last 20 samples for sparkline

    public init() {}

    public func update(rtt: Double, loss: Double, bitrate: Double, fps: Double) {
        self.rttMs = rtt
        self.lossPct = loss
        self.bitrateMbps = bitrate
        self.fps = fps
        rttHistory.append(rtt)
        if rttHistory.count > 20 { rttHistory.removeFirst() }
    }
}
```

- [ ] **Step 5: Write `IExtendSession.swift`**

```swift
import Foundation
import Observation

public protocol Clock: Sendable {
    func sleep(for duration: Duration) async throws
}

public struct SystemClock: Clock {
    public init() {}
    public func sleep(for duration: Duration) async throws {
        try await Task.sleep(for: duration)
    }
}

public struct PeerHandle: Sendable, Equatable {
    public let serviceName: String
    public static func stub(_ name: String) -> PeerHandle { .init(serviceName: name) }
}

public actor IExtendSession {
    public private(set) var state: ConnectionState = .idle
    public let settings = Settings()
    public let stats = FrameStats()
    private let clock: any Clock
    private var pairingTask: Task<Void, Error>?

    public init(clock: any Clock = SystemClock()) {
        self.clock = clock
    }

    public func beginPairing(host: PeerHandle) throws {
        guard state == .idle else { throw SessionError.invalidTransition }
        state = .pairing
        pairingTask = Task { [clock] in
            try await clock.sleep(for: .seconds(30))
            await self.failPairingIfStillPending()
        }
    }

    private func failPairingIfStillPending() {
        if state == .pairing {
            state = .failed(reason: .pairingTimeout)
        }
    }

    public func disconnect() {
        pairingTask?.cancel()
        state = .idle
    }
}

public enum SessionError: Error { case invalidTransition }
```

- [ ] **Step 6: Add a `TestClock` to the test target**

Append to `StateMachineTests.swift`:

```swift
final actor TestClock: Clock {
    private var continuations: [(deadline: Duration, cont: CheckedContinuation<Void, Error>)] = []
    private var now: Duration = .zero

    func sleep(for duration: Duration) async throws {
        let deadline = now + duration
        try await withCheckedThrowingContinuation { cont in
            continuations.append((deadline, cont))
        }
    }

    func advance(by duration: Duration) {
        now += duration
        let due = continuations.filter { $0.deadline <= now }
        continuations.removeAll { $0.deadline <= now }
        for entry in due { entry.cont.resume() }
    }
}
```

- [ ] **Step 7: Run tests, verify pass**

Run: `xcodebuild test -project iExtend.xcodeproj -scheme iExtendKit -destination 'platform=iOS Simulator,name=iPad Pro 11-inch (M4)' -only-testing:iExtendKitTests/StateMachineTests`

Expected: 3 passed.

- [ ] **Step 8: Commit**

```bash
git add ipad/iExtendKit/
git commit -m "feat(ipad): IExtendSession actor with state-machine stubs"
```

---

### Task 4: `Signaling.swift` — mDNS browse via `NWBrowser`

**Files:**
- Create: `ipad/iExtendKit/Sources/iExtendKit/Connection/Signaling.swift`
- Test: `ipad/iExtendKit/Tests/iExtendKitTests/SignalingTests.swift`

- [ ] **Step 1: Write the failing test**

```swift
import XCTest
@testable import iExtendKit

final class SignalingTests: XCTestCase {
    func testServiceTypeMatchesSpec() {
        XCTAssertEqual(Signaling.serviceType, "_iextend._tcp")
    }

    func testBrowserReturnsResultsViaAsyncSequence() async throws {
        // Headless test against a NWListener publishing _iextend._tcp on loopback.
        let listener = try NWListener(using: .tcp)
        listener.service = NWListener.Service(name: "test-host", type: Signaling.serviceType)
        listener.start(queue: .global())
        defer { listener.cancel() }

        let signaling = Signaling()
        var seen: Set<String> = []
        for await peer in signaling.browse() {
            seen.insert(peer.serviceName)
            if seen.contains("test-host") { break }
        }
        XCTAssertTrue(seen.contains("test-host"))
    }
}
```

- [ ] **Step 2: Implement `Signaling.swift`**

```swift
import Foundation
import Network

public final class Signaling: @unchecked Sendable {
    public static let serviceType = "_iextend._tcp"

    public init() {}

    public func browse() -> AsyncStream<PeerHandle> {
        AsyncStream { continuation in
            let descriptor = NWBrowser.Descriptor.bonjour(type: Signaling.serviceType, domain: nil)
            let browser = NWBrowser(for: descriptor, using: .tcp)
            browser.browseResultsChangedHandler = { results, _ in
                for result in results {
                    if case let .service(name, _, _, _) = result.endpoint {
                        continuation.yield(PeerHandle(serviceName: name))
                    }
                }
            }
            browser.start(queue: .global(qos: .userInitiated))
            continuation.onTermination = { _ in browser.cancel() }
        }
    }

    /// Verifies the host's TLS certificate against a pinned public key.
    /// Pinned keys live in the iOS keychain under "iextend.pinned.<peer-id>".
    public func validatePinnedCertificate(_ certificate: SecCertificate, peerID: String) -> Bool {
        // Implementation deferred to Plan 7 — this stub returns true so the rest
        // of the pipeline can wire up. Plan 7 replaces with real keychain lookup.
        return true
    }
}
```

- [ ] **Step 3: Run tests; expect SignalingTests pass**

- [ ] **Step 4: Commit**

```bash
git add ipad/iExtendKit/Sources/iExtendKit/Connection/Signaling.swift ipad/iExtendKit/Tests/iExtendKitTests/SignalingTests.swift
git commit -m "feat(ipad): Signaling.swift — mDNS browse + cert-pinning hook"
```

---

### Task 5: `PeerConnection.swift` — wraps `RTCPeerConnection`

**Files:**
- Create: `ipad/iExtendKit/Sources/iExtendKit/Connection/PeerConnection.swift`
- Create: `ipad/iExtendKit/Sources/iExtendKit/Connection/ControlChannel.swift`
- Test: `ipad/iExtendKit/Tests/iExtendKitTests/PeerConnectionTests.swift`

- [ ] **Step 1: Write `PeerConnection.swift`**

```swift
import Foundation
import WebRTC

public actor PeerConnection {
    public enum Event: Sendable {
        case stateChanged(RTCIceConnectionState)
        case videoFrame(RTCVideoFrame)
        case dataReceived(channel: ChannelKind, payload: Data)
        case error(Error)
    }
    public enum ChannelKind: String, Sendable { case input, control }

    private let factory: RTCPeerConnectionFactory
    private var peer: RTCPeerConnection?
    private var inputChannel: RTCDataChannel?
    private var controlChannel: RTCDataChannel?
    private let observer = PeerObserver()

    public let events: AsyncStream<Event>
    private let eventContinuation: AsyncStream<Event>.Continuation

    public init() {
        RTCInitializeSSL()
        let encoderFactory = RTCDefaultVideoEncoderFactory()
        let decoderFactory = RTCDefaultVideoDecoderFactory()
        self.factory = RTCPeerConnectionFactory(encoderFactory: encoderFactory, decoderFactory: decoderFactory)
        var continuation: AsyncStream<Event>.Continuation!
        self.events = AsyncStream { continuation = $0 }
        self.eventContinuation = continuation
    }

    public func connect(toRemoteSDP remoteSDP: String) async throws -> String {
        let config = RTCConfiguration()
        config.iceServers = [] // host candidates only — same-LAN only, no STUN
        config.iceTransportPolicy = .all
        config.bundlePolicy = .maxBundle
        config.rtcpMuxPolicy = .require
        config.sdpSemantics = .unifiedPlan
        let constraints = RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)
        guard let peer = factory.peerConnection(with: config, constraints: constraints, delegate: observer) else {
            throw PeerError.factoryFailed
        }
        self.peer = peer
        observer.attach(continuation: eventContinuation)

        // Receive-only video; we never send video from the iPad
        peer.addTransceiver(of: .video, init: .init().with { $0.direction = .recvOnly })

        // Two DataChannels — input (unreliable) + control (reliable)
        let inputCfg = RTCDataChannelConfiguration()
        inputCfg.isOrdered = false
        inputCfg.maxRetransmits = 0
        inputCfg.protocol = "iextend.input"
        self.inputChannel = peer.dataChannel(forLabel: "input", configuration: inputCfg)

        let controlCfg = RTCDataChannelConfiguration()
        controlCfg.isOrdered = true
        controlCfg.protocol = "iextend.control"
        self.controlChannel = peer.dataChannel(forLabel: "control", configuration: controlCfg)

        try await peer.setRemoteDescription(.init(type: .offer, sdp: remoteSDP))
        let answer = try await peer.answer(for: constraints)
        try await peer.setLocalDescription(answer)
        return answer.sdp
    }

    public func sendInput(_ packet: Data) {
        let buffer = RTCDataBuffer(data: packet, isBinary: true)
        inputChannel?.sendData(buffer)
    }

    public func sendControl(_ payload: Data) {
        let buffer = RTCDataBuffer(data: payload, isBinary: true)
        controlChannel?.sendData(buffer)
    }

    public func close() {
        peer?.close()
        peer = nil
        eventContinuation.finish()
    }
}

public enum PeerError: Error { case factoryFailed }

private extension RTCRtpTransceiverInit {
    func with(_ mut: (RTCRtpTransceiverInit) -> Void) -> RTCRtpTransceiverInit { mut(self); return self }
}

/// Bridges RTCPeerConnectionDelegate (Obj-C, main-thread-callback) to AsyncStream.
final class PeerObserver: NSObject, RTCPeerConnectionDelegate, @unchecked Sendable {
    private var continuation: AsyncStream<PeerConnection.Event>.Continuation?
    func attach(continuation: AsyncStream<PeerConnection.Event>.Continuation) {
        self.continuation = continuation
    }
    func peerConnection(_ peerConnection: RTCPeerConnection, didChange newState: RTCIceConnectionState) {
        continuation?.yield(.stateChanged(newState))
    }
    // Stub the rest of the protocol as no-ops; overridden as needed.
    func peerConnection(_ peerConnection: RTCPeerConnection, didChange stateChanged: RTCSignalingState) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didAdd stream: RTCMediaStream) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didRemove stream: RTCMediaStream) {}
    func peerConnectionShouldNegotiate(_ peerConnection: RTCPeerConnection) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didChange newState: RTCIceGatheringState) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didGenerate candidate: RTCIceCandidate) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didRemove candidates: [RTCIceCandidate]) {}
    func peerConnection(_ peerConnection: RTCPeerConnection, didOpen dataChannel: RTCDataChannel) {}
}
```

- [ ] **Step 2: Write `ControlChannel.swift`**

```swift
import Foundation

public struct HeartbeatMessage: Codable, Sendable {
    public let seq: UInt32
    public let timestamp: UInt64
}

public actor ControlChannel {
    private weak var peer: PeerConnection?
    private var heartbeatTask: Task<Void, Never>?
    private var seq: UInt32 = 0

    public init(peer: PeerConnection) { self.peer = peer }

    /// Spec §9: 250 ms heartbeat cadence; 4 missed heartbeats = 1 s detection window.
    public func startHeartbeats() {
        heartbeatTask = Task { [weak self] in
            while !Task.isCancelled {
                await self?.sendOne()
                try? await Task.sleep(for: .milliseconds(250))
            }
        }
    }

    public func stopHeartbeats() { heartbeatTask?.cancel() }

    private func sendOne() async {
        seq &+= 1
        let msg = HeartbeatMessage(seq: seq, timestamp: UInt64(Date().timeIntervalSince1970 * 1000))
        if let data = try? JSONEncoder().encode(msg), let peer {
            await peer.sendControl(data)
        }
    }
}
```

- [ ] **Step 3: Test the SDP plumbing**

Without a real peer, we can only smoke-test that calling `connect(toRemoteSDP:)` with a valid offer SDP doesn't throw immediately. Add to `PeerConnectionTests.swift`:

```swift
func testConnectAcceptsValidSDP() async throws {
    let pc = PeerConnection()
    let sampleOffer = """
    v=0
    o=- 0 0 IN IP4 127.0.0.1
    s=-
    t=0 0
    m=video 9 UDP/TLS/RTP/SAVPF 96
    c=IN IP4 0.0.0.0
    a=rtpmap:96 H264/90000
    a=recvonly
    """
    let answer = try await pc.connect(toRemoteSDP: sampleOffer)
    XCTAssertTrue(answer.contains("v=0"))
    await pc.close()
}
```

- [ ] **Step 4: Commit**

```bash
git add ipad/iExtendKit/Sources/iExtendKit/Connection/
git commit -m "feat(ipad): PeerConnection actor wrapping RTCPeerConnection + ControlChannel"
```

---

### Task 6: `DecodeSession.swift` — VideoToolbox HEVC + AV1

**Files:**
- Create: `ipad/iExtendKit/Sources/iExtendKit/Decode/DecodeSession.swift`
- Create: `ipad/iExtendKit/Sources/iExtendKit/Decode/FrameQueue.swift`
- Create: `ipad/iExtendKit/Sources/iExtendKit/Decode/CodecCaps.swift`
- Test: `ipad/iExtendKit/Tests/iExtendKitTests/DecodeSessionTests.swift`
- Test: `ipad/iExtendKit/Tests/iExtendKitTests/FrameQueueTests.swift`
- Test: `ipad/iExtendKit/Tests/iExtendKitTests/CodecCapsTests.swift`

- [ ] **Step 1: Write `CodecCaps.swift`**

```swift
import Foundation
import VideoToolbox

public struct CodecCaps: Sendable {
    public let supportsHEVC: Bool
    public let supportsHEVC10Bit: Bool
    public let supportsAV1: Bool
    public let isMSeries: Bool

    public static func detect() -> CodecCaps {
        let hevc = VTIsHardwareDecodeSupported(kCMVideoCodecType_HEVC)
        let hevc10 = VTIsHardwareDecodeSupported(kCMVideoCodecType_HEVC) // 10-bit checked at session creation
        // AV1 hardware decode is M4-only as of writing.
        let av1: Bool
        if #available(iOS 18, *) {
            av1 = VTIsHardwareDecodeSupported(kCMVideoCodecType_AV1)
        } else {
            av1 = false
        }
        let m = isAppleSiliconMSeries()
        return CodecCaps(supportsHEVC: hevc, supportsHEVC10Bit: hevc10, supportsAV1: av1, isMSeries: m)
    }

    private static func isAppleSiliconMSeries() -> Bool {
        // Heuristic: any iPad with `.machine` ∈ {iPad13,*, iPad14,*, iPad16,*}
        // is M-series. Read sysctlbyname("hw.machine").
        var size = 0
        sysctlbyname("hw.machine", nil, &size, nil, 0)
        var bytes = [CChar](repeating: 0, count: size)
        sysctlbyname("hw.machine", &bytes, &size, nil, 0)
        let machine = String(cString: bytes)
        return machine.hasPrefix("iPad13,") || machine.hasPrefix("iPad14,") || machine.hasPrefix("iPad16,")
    }
}
```

- [ ] **Step 2: Write `FrameQueue.swift` (lock-free SPSC ring)**

```swift
import Foundation
import Atomics
import CoreVideo

public final class FrameQueue: @unchecked Sendable {
    private let capacity: Int
    private var slots: [CVPixelBuffer?]
    private let head = ManagedAtomic<Int>(0)  // producer
    private let tail = ManagedAtomic<Int>(0)  // consumer

    public init(capacity: Int = 4) {
        self.capacity = capacity
        self.slots = .init(repeating: nil, count: capacity)
    }

    @discardableResult
    public func enqueue(_ frame: CVPixelBuffer) -> Bool {
        let h = head.load(ordering: .relaxed)
        let t = tail.load(ordering: .acquiring)
        if (h + 1) % capacity == t { return false }   // full — drop
        slots[h] = frame
        head.store((h + 1) % capacity, ordering: .releasing)
        return true
    }

    public func dequeue() -> CVPixelBuffer? {
        let t = tail.load(ordering: .relaxed)
        let h = head.load(ordering: .acquiring)
        if h == t { return nil }
        let frame = slots[t]
        slots[t] = nil
        tail.store((t + 1) % capacity, ordering: .releasing)
        return frame
    }
}
```

- [ ] **Step 3: Write `DecodeSession.swift`**

```swift
import Foundation
import VideoToolbox
import CoreMedia
import CoreVideo

public actor DecodeSession {
    private var session: VTDecompressionSession?
    private var formatDescription: CMVideoFormatDescription?
    private let queue: FrameQueue
    private let caps: CodecCaps

    public init(queue: FrameQueue, caps: CodecCaps = .detect()) {
        self.queue = queue
        self.caps = caps
    }

    public func configure(codec: CMVideoCodecType, width: Int32, height: Int32) throws {
        let extensions: [String: Any] = [:]
        var format: CMVideoFormatDescription?
        let status = CMVideoFormatDescriptionCreate(
            allocator: kCFAllocatorDefault,
            codecType: codec,
            width: width,
            height: height,
            extensions: extensions as CFDictionary,
            formatDescriptionOut: &format
        )
        guard status == noErr, let format else { throw DecodeError.formatDescriptionFailed }
        self.formatDescription = format

        let attrs: [CFString: Any] = [
            kCVPixelBufferPixelFormatTypeKey: kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
            kCVPixelBufferIOSurfacePropertiesKey: [:],   // request IOSurface backing for zero-copy Metal
            kCVPixelBufferMetalCompatibilityKey: true,
        ]
        var callback = VTDecompressionOutputCallbackRecord(
            decompressionOutputCallback: { ctx, _, status, _, image, _, _ in
                guard status == noErr, let image, let ctx else { return }
                let session = Unmanaged<DecodeSession>.fromOpaque(ctx).takeUnretainedValue()
                Task { await session.handleDecoded(image) }
            },
            decompressionOutputRefCon: Unmanaged.passUnretained(self).toOpaque()
        )
        var session: VTDecompressionSession?
        let st = VTDecompressionSessionCreate(
            allocator: kCFAllocatorDefault,
            formatDescription: format,
            decoderSpecification: nil,
            imageBufferAttributes: attrs as CFDictionary,
            outputCallback: &callback,
            decompressionSessionOut: &session
        )
        guard st == noErr, let session else { throw DecodeError.sessionCreateFailed(st) }
        self.session = session
    }

    public func decode(sampleBuffer: CMSampleBuffer) throws {
        guard let session else { throw DecodeError.notConfigured }
        let flags: VTDecodeFrameFlags = ._EnableAsynchronousDecompression
        var infoFlags = VTDecodeInfoFlags()
        VTDecompressionSessionDecodeFrame(session, sampleBuffer: sampleBuffer, flags: flags, frameRefcon: nil, infoFlagsOut: &infoFlags)
    }

    private func handleDecoded(_ image: CVImageBuffer) {
        queue.enqueue(image as CVPixelBuffer)
    }

    public func tearDown() {
        if let session { VTDecompressionSessionInvalidate(session) }
        session = nil
    }
}

public enum DecodeError: Error {
    case formatDescriptionFailed
    case sessionCreateFailed(OSStatus)
    case notConfigured
}
```

- [ ] **Step 4: Tests for FrameQueue (round-trip + drop-on-full)**

```swift
import XCTest
import CoreVideo
@testable import iExtendKit

final class FrameQueueTests: XCTestCase {
    func testRoundTrip() {
        let q = FrameQueue(capacity: 4)
        let pb = makePixelBuffer()
        XCTAssertTrue(q.enqueue(pb))
        XCTAssertNotNil(q.dequeue())
        XCTAssertNil(q.dequeue())
    }

    func testDropsWhenFull() {
        let q = FrameQueue(capacity: 4)
        let pb = makePixelBuffer()
        // capacity-1 = 3 effective slots in our ring
        XCTAssertTrue(q.enqueue(pb))
        XCTAssertTrue(q.enqueue(pb))
        XCTAssertTrue(q.enqueue(pb))
        XCTAssertFalse(q.enqueue(pb))
    }

    private func makePixelBuffer() -> CVPixelBuffer {
        var pb: CVPixelBuffer?
        CVPixelBufferCreate(kCFAllocatorDefault, 16, 16, kCVPixelFormatType_32BGRA, nil, &pb)
        return pb!
    }
}
```

- [ ] **Step 5: Codec-caps test**

```swift
final class CodecCapsTests: XCTestCase {
    func testHEVCAlwaysSupportedOnTestDevice() {
        let caps = CodecCaps.detect()
        XCTAssertTrue(caps.supportsHEVC)
    }

    func testAV1OnlyOnM4OrSimulator18() {
        // Just assert the field exists; runtime value depends on hardware.
        _ = CodecCaps.detect().supportsAV1
    }
}
```

- [ ] **Step 6: Commit**

```bash
git add ipad/iExtendKit/Sources/iExtendKit/Decode/ ipad/iExtendKit/Tests/iExtendKitTests/{FrameQueueTests,CodecCapsTests}.swift
git commit -m "feat(ipad): VTDecompressionSession + lock-free SPSC frame queue"
```

---

### Task 7: `MetalRenderer.swift` — minimum-viable blit pipeline

**Files:**
- Create: `ipad/iExtendKit/Sources/iExtendKit/Render/MetalRenderer.swift`
- Create: `ipad/iExtendKit/Sources/iExtendKit/Render/BlitShader.metal`

The full reprojection compute pass is Plan 8. For Plan 6, the renderer is the simplest thing that converts a `CVPixelBuffer` into an `MTLTexture` via `CVMetalTextureCache` (zero-copy thanks to IOSurface) and blits it to the next `CAMetalLayer` drawable on a `CADisplayLink` callback.

- [ ] **Step 1: Write `BlitShader.metal`**

```metal
#include <metal_stdlib>
using namespace metal;

struct VertexOut {
    float4 position [[position]];
    float2 uv;
};

vertex VertexOut blit_vertex(uint vid [[vertex_id]]) {
    float2 positions[4] = { float2(-1, -1), float2(1, -1), float2(-1, 1), float2(1, 1) };
    float2 uvs[4] = { float2(0, 1), float2(1, 1), float2(0, 0), float2(1, 0) };
    VertexOut out;
    out.position = float4(positions[vid], 0, 1);
    out.uv = uvs[vid];
    return out;
}

fragment float4 blit_fragment(VertexOut in [[stage_in]],
                              texture2d<float> yTex [[texture(0)]],
                              texture2d<float> cbcrTex [[texture(1)]]) {
    constexpr sampler s(address::clamp_to_edge, filter::linear);
    float y = yTex.sample(s, in.uv).r;
    float2 cbcr = cbcrTex.sample(s, in.uv).rg - float2(0.5);
    // BT.709 limited-range to RGB
    float3 rgb;
    rgb.r = y + 1.402 * cbcr.y;
    rgb.g = y - 0.344 * cbcr.x - 0.714 * cbcr.y;
    rgb.b = y + 1.772 * cbcr.x;
    return float4(rgb, 1);
}
```

- [ ] **Step 2: Write `MetalRenderer.swift`**

```swift
import Foundation
import Metal
import MetalKit
import QuartzCore
import CoreVideo

public final class MetalRenderer: NSObject {
    private let device: MTLDevice
    private let commandQueue: MTLCommandQueue
    private let pipelineState: MTLRenderPipelineState
    private let textureCache: CVMetalTextureCache
    private let layer: CAMetalLayer
    private let queue: FrameQueue
    private var displayLink: CADisplayLink?

    public init(layer: CAMetalLayer, queue: FrameQueue) throws {
        guard let device = MTLCreateSystemDefaultDevice() else { throw RendererError.noDevice }
        self.device = device
        self.commandQueue = device.makeCommandQueue()!
        self.queue = queue
        self.layer = layer
        layer.device = device
        layer.pixelFormat = .bgra8Unorm
        layer.framebufferOnly = true

        var cache: CVMetalTextureCache?
        CVMetalTextureCacheCreate(kCFAllocatorDefault, nil, device, nil, &cache)
        self.textureCache = cache!

        let bundleURL = Bundle.module.url(forResource: "BlitShader", withExtension: "metal")!
        let library = try device.makeLibrary(source: try String(contentsOf: bundleURL), options: nil)
        let descriptor = MTLRenderPipelineDescriptor()
        descriptor.vertexFunction = library.makeFunction(name: "blit_vertex")
        descriptor.fragmentFunction = library.makeFunction(name: "blit_fragment")
        descriptor.colorAttachments[0].pixelFormat = .bgra8Unorm
        self.pipelineState = try device.makeRenderPipelineState(descriptor: descriptor)
        super.init()
    }

    public func start() {
        let link = CADisplayLink(target: self, selector: #selector(tick))
        // Match ProMotion 120 Hz; spec §3 Wi-Fi target is 120 Hz lock.
        link.preferredFrameRateRange = CAFrameRateRange(minimum: 60, maximum: 120, preferred: 120)
        link.add(to: .main, forMode: .common)
        self.displayLink = link
    }

    public func stop() {
        displayLink?.invalidate()
        displayLink = nil
    }

    @objc private func tick(_ link: CADisplayLink) {
        guard let pixelBuffer = queue.dequeue() else { return }
        guard let drawable = layer.nextDrawable() else { return }
        guard let (yTex, cbcrTex) = makeTextures(from: pixelBuffer) else { return }

        let descriptor = MTLRenderPassDescriptor()
        descriptor.colorAttachments[0].texture = drawable.texture
        descriptor.colorAttachments[0].loadAction = .clear
        descriptor.colorAttachments[0].clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 1)
        descriptor.colorAttachments[0].storeAction = .store

        guard let buffer = commandQueue.makeCommandBuffer(),
              let encoder = buffer.makeRenderCommandEncoder(descriptor: descriptor) else { return }
        encoder.setRenderPipelineState(pipelineState)
        encoder.setFragmentTexture(yTex, index: 0)
        encoder.setFragmentTexture(cbcrTex, index: 1)
        encoder.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 4)
        encoder.endEncoding()
        buffer.present(drawable)
        buffer.commit()
    }

    private func makeTextures(from pb: CVPixelBuffer) -> (MTLTexture, MTLTexture)? {
        let w = CVPixelBufferGetWidth(pb), h = CVPixelBufferGetHeight(pb)
        var yMetalTex: CVMetalTexture?
        var cbcrMetalTex: CVMetalTexture?
        CVMetalTextureCacheCreateTextureFromImage(kCFAllocatorDefault, textureCache, pb, nil, .r8Unorm, w, h, 0, &yMetalTex)
        CVMetalTextureCacheCreateTextureFromImage(kCFAllocatorDefault, textureCache, pb, nil, .rg8Unorm, w / 2, h / 2, 1, &cbcrMetalTex)
        guard let y = yMetalTex.flatMap(CVMetalTextureGetTexture),
              let c = cbcrMetalTex.flatMap(CVMetalTextureGetTexture) else { return nil }
        return (y, c)
    }
}

public enum RendererError: Error { case noDevice }
```

- [ ] **Step 3: Smoke test the renderer with a synthetic queue**

Create `ipad/iExtendKit/Tests/iExtendKitTests/MetalRendererSmoke.swift`:

```swift
import XCTest
@testable import iExtendKit
import QuartzCore

final class MetalRendererSmoke: XCTestCase {
    func testRendererBootsAndDoesNotCrashOnEmptyQueue() throws {
        let layer = CAMetalLayer()
        layer.frame = CGRect(x: 0, y: 0, width: 320, height: 240)
        let queue = FrameQueue()
        let renderer = try MetalRenderer(layer: layer, queue: queue)
        renderer.start()
        let exp = expectation(description: "ticks survive empty queue")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
            renderer.stop()
            exp.fulfill()
        }
        wait(for: [exp], timeout: 2)
    }
}
```

- [ ] **Step 4: Commit**

```bash
git add ipad/iExtendKit/Sources/iExtendKit/Render/ ipad/iExtendKit/Tests/iExtendKitTests/MetalRendererSmoke.swift
git commit -m "feat(ipad): minimum-viable Metal blit pipeline (reprojection deferred to Plan 8)"
```

---

### Task 8: SwiftUI screens — 1:1 with the design canvas

**Files:** all under `ipad/iExtendUI/Sources/iExtendUI/`

For each screen, open `iExtend.html` in Safari (run `cd /home/tops/Projects/iExtend && npm run view`) and pin the matching artboard side-by-side with Xcode. Color tokens, font weights, spacing, and corner radii must match within 2 px / 1 pt.

This task is broken into sub-steps per screen so each can be reviewed independently.

- [ ] **Step 1: Write `Theme.swift`**

```swift
import SwiftUI

public enum Theme {
    public static let systemBlue   = Color(red: 0.039, green: 0.518, blue: 1.0)   // #0a84ff
    public static let systemIndigo = Color(red: 0.369, green: 0.361, blue: 0.902) // #5e5ce6
    public static let systemOrange = Color(red: 1.0,   green: 0.624, blue: 0.039) // #ff9f0a
    public static let systemGreen  = Color(red: 0.188, green: 0.820, blue: 0.345) // #30d158
    public static let systemRed    = Color(red: 1.0,   green: 0.271, blue: 0.227) // #ff453a

    public static let bgDark       = Color(red: 0.059, green: 0.067, blue: 0.082) // #0f1115
    public static let inkDark      = Color.white
    public static let inkDimDark   = Color.white.opacity(0.6)
}
```

- [ ] **Step 2: `RootView.swift` — switch on session state**

```swift
import SwiftUI
import iExtendKit

public struct RootView: View {
    @State private var session = IExtendSession()
    @State private var state: ConnectionState = .idle

    public init() {}

    public var body: some View {
        Group {
            switch state {
            case .idle:                WelcomeView(session: session)
            case .pairing:             PairView(session: session)
            case .connecting:          LiveView(session: session, overlay: .connecting)
            case .live:                LiveView(session: session, overlay: nil)
            case .degraded:            LiveView(session: session, overlay: .degraded)
            case .disconnected:        LiveView(session: session, overlay: .disconnected)
            case .failed:              DisconnectedOverlay(session: session)
            }
        }
        .preferredColorScheme(.dark)
        .task {
            // Poll the actor's state. In Plan 8 we'll switch to an AsyncSequence.
            while !Task.isCancelled {
                state = await session.state
                try? await Task.sleep(for: .milliseconds(100))
            }
        }
    }
}
```

- [ ] **Step 3: Build `WelcomeView.swift` matching the "iPad · Welcome" artboard**

Reference: `iExtend.html` → Section "Onboarding" → Artboard "iPad · Welcome". Two-column 1.05:1 grid; left has eyebrow chip, gradient hero headline ("Your iPad, a second screen."), 320pt-max paragraph, two PillButtons, three feature chips. Right has a 270×340 glass card listing three modes with one selected. The exact color values, padding, and typography are documented inline in `design-source/project/src/scenes-ipad.jsx` lines 64–151.

Implement exactly that in SwiftUI. Use `LinearGradient` for the headline gradient, `.background(.ultraThinMaterial)` for the glass card, and `Theme` colors. Cap the file at 250 lines.

- [ ] **Step 4: Build `DiscoverView.swift` matching the "iPad · Discover" artboard**

Reference: `scenes-ipad.jsx` lines 153–242. Step indicator chip, hero "Looking for your computer…", 4-row device list (one selected with Connect pill), Rescan + Manual IP buttons. Wire the row taps to `await session.beginPairing(host: ...)` to advance state.

- [ ] **Step 5: Build `PairView.swift` matching the "iPad · Pair" artboard**

Reference: `scenes-ipad.jsx` lines 244–335. Two-column grid: left card has the QR code (use `CIFilter.qrCodeGenerator` to render an actual QR for `iextend://pair/<token>`), right card has 4 PIN boxes + numeric pad. PIN entry calls a Plan 7 stub (any 4-digit PIN advances to `.connecting`).

- [ ] **Step 6: Build `LiveView.swift` + `MetalLayerHost.swift`**

`MetalLayerHost.swift` is a `UIViewControllerRepresentable` that creates a `UIViewController` whose `view.layer` is replaced by a `CAMetalLayer`. It instantiates a `MetalRenderer` against that layer and a `FrameQueue` exposed via the session.

`LiveView.swift` overlays the floating toolbar via `ZStack` and handles the connecting/disconnected overlays.

- [ ] **Step 7: Build `FloatingToolbar.swift` matching the floating toolbar artboards**

Reference: `scenes-ipad.jsx` lines 467–528 (the `FloatingToolbar` component). Glass pill with 7 items: mode (extend icon), res (monitor icon), latency dot ("8 ms"), pencil, hand, gear, end (red). Position: bottom (default), top, or left — driven by `session.settings`.

- [ ] **Step 8: Build the connecting and disconnected overlays**

Reference: `scenes-ipad.jsx` lines 530–582. Modal cards centered over a blurred backdrop. Spinner ring uses `Animation.linear(duration: 1).repeatForever(autoreverses: false)` rotation.

- [ ] **Step 9: Build `SettingsView.swift` + the panes**

Reference: `scenes-ipad.jsx` lines 597–757. Sidebar + detail layout (`NavigationSplitView`). Sidebar groups: Connection (selected), Display, Pencil & Touch, Performance, General, Diagnostics. Detail pane shows grouped lists. The Latency row uses `LatencySpark` (a simple SwiftUI `Path` shape).

- [ ] **Step 10: Snapshot tests**

For each screen, capture a snapshot at 1194×834 (iPad Pro 11" landscape). Use `swift-snapshot-testing` or write a thin XCTest helper using `UIGraphicsImageRenderer`. Compare against committed reference PNGs. Fail if pixel diff > 1%.

```swift
final class SnapshotTests: XCTestCase {
    func testWelcomeMatchesArtboard() throws { try assertSnapshot(of: WelcomeView(session: .init()), named: "welcome") }
    // ... one per screen
}
```

- [ ] **Step 11: Commit per screen, not per task**

Each of the screens above is committed separately so the diff per commit is reviewable:

```bash
# After Step 1–2:
git add ipad/iExtendUI/Sources/iExtendUI/{Theme.swift,RootView.swift}
git commit -m "feat(ipad-ui): theme tokens + RootView state switch"

# After Step 3:
git add ipad/iExtendUI/Sources/iExtendUI/Onboarding/WelcomeView.swift
git commit -m "feat(ipad-ui): WelcomeView matching iPad · Welcome artboard"

# ... and so on per screen
```

---

### Task 9: `EventCapture.swift` + `PacketEncoder.swift`

**Files:**
- Create: `ipad/iExtendInput/Sources/iExtendInput/EventKind.swift`
- Create: `ipad/iExtendInput/Sources/iExtendInput/PacketEncoder.swift`
- Create: `ipad/iExtendInput/Sources/iExtendInput/EventSink.swift`
- Create: `ipad/iExtendInput/Sources/iExtendInput/EventCapture.swift`
- Test: `ipad/iExtendInput/Tests/iExtendInputTests/PacketEncoderTests.swift`

- [ ] **Step 1: `EventKind.swift`**

```swift
import Foundation

public enum EventKind: UInt8, Sendable {
    case touchBegin = 0x01
    case touchMove  = 0x02
    case touchEnd   = 0x03
    case pencilBegin = 0x10
    case pencilMove  = 0x11
    case pencilEnd   = 0x12
    case keyDown    = 0x20
    case keyUp      = 0x21
    case modifier   = 0x22
}
```

- [ ] **Step 2: `PacketEncoder.swift`**

Spec §6.1: 32-byte fixed-size packet — `kind(1) | time(8) | seq(4) | flags(1) | payload(18)`.

```swift
import Foundation

public struct InputPacket: Sendable {
    public let kind: EventKind
    public let time: UInt64
    public let seq: UInt32
    public let flags: UInt8
    public let payload: Data   // exactly 18 bytes

    public func encode() -> Data {
        precondition(payload.count == 18, "payload must be exactly 18 bytes")
        var data = Data(capacity: 32)
        data.append(kind.rawValue)
        withUnsafeBytes(of: time.bigEndian)  { data.append(contentsOf: $0) }
        withUnsafeBytes(of: seq.bigEndian)   { data.append(contentsOf: $0) }
        data.append(flags)
        data.append(payload)
        precondition(data.count == 32)
        return data
    }
}

public enum Q16 {
    /// Convert a Float in [0.0, 1.0] to UInt16 Q16 fixed-point.
    public static func encode(_ value: Float) -> UInt16 {
        let clamped = max(0, min(1, value))
        return UInt16(clamped * Float(UInt16.max))
    }

    public static func encodeSigned(_ value: Float) -> Int16 {
        let clamped = max(-1, min(1, value))
        return Int16(clamped * Float(Int16.max))
    }
}
```

- [ ] **Step 3: `EventSink.swift`**

```swift
import Foundation

public protocol EventSink: AnyObject, Sendable {
    func send(_ packet: InputPacket)
}
```

- [ ] **Step 4: `EventCapture.swift`**

```swift
import UIKit

@MainActor
public final class EventCapture {
    public let sink: any EventSink
    private var seq: UInt32 = 0

    public init(sink: any EventSink) { self.sink = sink }

    public func touchesMoved(_ touches: Set<UITouch>, in view: UIView) {
        for t in touches {
            let p = t.location(in: view)
            seq &+= 1
            let payload = encodeTouchPayload(x: p.x, y: p.y, force: t.force, viewSize: view.bounds.size)
            let pkt = InputPacket(
                kind: t.type == .pencil ? .pencilMove : .touchMove,
                time: UInt64(mach_absolute_time()),
                seq: seq, flags: 0, payload: payload
            )
            sink.send(pkt)
        }
    }

    private func encodeTouchPayload(x: CGFloat, y: CGFloat, force: CGFloat, viewSize: CGSize) -> Data {
        var payload = Data(repeating: 0, count: 18)
        let xQ = Q16.encode(Float(x / viewSize.width))
        let yQ = Q16.encode(Float(y / viewSize.height))
        let fQ = Q16.encode(Float(force))
        payload.replaceSubrange(0..<2,  with: withUnsafeBytes(of: xQ.bigEndian) { Data($0) })
        payload.replaceSubrange(2..<4,  with: withUnsafeBytes(of: yQ.bigEndian) { Data($0) })
        payload.replaceSubrange(4..<6,  with: withUnsafeBytes(of: fQ.bigEndian) { Data($0) })
        // Bytes 6..18: pencil tilt/azimuth/twist/buttons/hover. Plan 8 wires those in.
        return payload
    }
}
```

- [ ] **Step 5: Tests**

```swift
import XCTest
@testable import iExtendInput

final class PacketEncoderTests: XCTestCase {
    func testEncodedLengthIsExactly32Bytes() {
        let packet = InputPacket(kind: .touchMove, time: 0, seq: 0, flags: 0, payload: Data(repeating: 0, count: 18))
        XCTAssertEqual(packet.encode().count, 32)
    }

    func testQ16RoundTrip() {
        XCTAssertEqual(Q16.encode(0.0), 0)
        XCTAssertEqual(Q16.encode(1.0), UInt16.max)
        XCTAssertEqual(Q16.encode(0.5), UInt16.max / 2)
    }

    func testKindByteIsAtPositionZero() {
        let packet = InputPacket(kind: .pencilMove, time: 0, seq: 0, flags: 0, payload: Data(repeating: 0, count: 18))
        XCTAssertEqual(packet.encode()[0], EventKind.pencilMove.rawValue)
    }
}
```

- [ ] **Step 6: Commit**

```bash
git add ipad/iExtendInput/
git commit -m "feat(ipad-input): EventCapture + 32-byte InputPacket encoder"
```

---

### Task 10: End-to-end smoke against Plan 5's `smoke_loopback.rs`

**Files:**
- Create: `ipad/iExtendKit/Tests/iExtendKitTests/EndToEndSmoke.swift`

Plan 5 produces a `smoke_loopback` binary that publishes mDNS `_iextend._tcp`, accepts an SDP answer, and streams a fake 120 Hz coloured-square HEVC video for 60 seconds. This task wires the iPad app to that peer and asserts:

1. Discovery: the iPad's `Signaling.browse()` sees `smoke-loopback`.
2. Handshake: the WebRTC offer/answer completes within 5 seconds.
3. Stream: at least 7000 frames decoded over 60 seconds (≥ 120 fps × 60 s × 0.97 tolerance for jitter).
4. No frame queue overflow: queue dropped < 10 frames over the run.

- [ ] **Step 1: Run `smoke_loopback` from Plan 5 on the host machine**

```bash
cd /path/to/iextend-host
cargo run --bin smoke_loopback --release
```

If Plan 5 isn't done yet, skip this task and add a `// TODO(plan-5)` comment in the test file.

- [ ] **Step 2: Write the test**

```swift
import XCTest
@testable import iExtendKit

final class EndToEndSmoke: XCTestCase {
    func testSmokeLoopback() async throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["IEXTEND_E2E"] == "1",
                          "Set IEXTEND_E2E=1 and run smoke_loopback on the host first")

        let signaling = Signaling()
        var foundHost: PeerHandle?
        for await peer in signaling.browse() {
            if peer.serviceName == "smoke-loopback" { foundHost = peer; break }
        }
        let host = try XCTUnwrap(foundHost)

        // Plan 7 stubs PIN exchange — for smoke_loopback the host accepts any PIN
        let session = IExtendSession()
        try await session.beginPairing(host: host)
        // ... (full handshake driven by IExtendSession; details emerge from Plan 5/7 as they land)

        let queue = FrameQueue(capacity: 4)
        let decoder = DecodeSession(queue: queue)
        try await decoder.configure(codec: kCMVideoCodecType_HEVC, width: 1920, height: 1080)

        var framesDecoded = 0
        let deadline = Date().addingTimeInterval(60)
        while Date() < deadline {
            if let _ = queue.dequeue() { framesDecoded += 1 }
            try await Task.sleep(for: .milliseconds(8))
        }

        XCTAssertGreaterThanOrEqual(framesDecoded, 7000, "Should decode ≥ 7000 frames in 60 s")
    }
}
```

- [ ] **Step 3: Document in `ipad/README.md`**

Add a section explaining how to run the E2E smoke test against `smoke_loopback`.

- [ ] **Step 4: Commit**

```bash
git add ipad/iExtendKit/Tests/iExtendKitTests/EndToEndSmoke.swift ipad/README.md
git commit -m "test(ipad): end-to-end smoke against smoke_loopback (gated on IEXTEND_E2E=1)"
```

---

### Task 11: TestFlight-ready archive (signing deferred to Plan 9)

**Files:** none directly; this is a build-and-verify gate.

- [ ] **Step 1: Archive build**

```bash
cd /home/tops/Projects/iExtend/ipad
xcodebuild archive \
  -project iExtend.xcodeproj \
  -scheme iExtend \
  -destination 'generic/platform=iOS' \
  -archivePath ./build/iExtend.xcarchive \
  CODE_SIGN_STYLE=Manual \
  CODE_SIGNING_REQUIRED=NO \
  CODE_SIGNING_ALLOWED=NO
```

The archive must build successfully without a signing identity. Plan 9 adds the real signing identity + provisioning profiles.

- [ ] **Step 2: Verify the archive contains the WebRTC framework**

```bash
ls /home/tops/Projects/iExtend/ipad/build/iExtend.xcarchive/Products/Applications/iExtend.app/Frameworks/
```

Expected: `WebRTC.framework`.

- [ ] **Step 3: No commit needed (build artifact only).**

---

### Task 12: README + plan-status update

**Files:**
- Modify: `/home/tops/Projects/iExtend/README.md`

- [ ] **Step 1: Add iPad-specific build instructions to repo root README**

Append:

```markdown
## iPad app

The iPad client lives under `ipad/`. See `ipad/README.md` for the build workflow.

Quick start:

```bash
brew install xcodegen
cd ipad
xcodegen generate
open iExtend.xcodeproj
```
```

- [ ] **Step 2: Update plan-status checkbox**

Change the "Plan 6" line in the existing plan-status list from `[ ]` to `[x]`.

- [ ] **Step 3: Final test run**

```bash
cd /home/tops/Projects/iExtend/ipad
xcodebuild test -project iExtend.xcodeproj -scheme iExtendKit -destination 'platform=iOS Simulator,name=iPad Pro 11-inch (M4)'
xcodebuild test -project iExtend.xcodeproj -scheme iExtendUI -destination 'platform=iOS Simulator,name=iPad Pro 11-inch (M4)'
xcodebuild test -project iExtend.xcodeproj -scheme iExtendInput -destination 'platform=iOS Simulator,name=iPad Pro 11-inch (M4)'
```

All three test schemes must pass.

- [ ] **Step 4: Commit and tag**

```bash
git add README.md
git commit -m "docs: mark Plan 6 complete and link iPad app build instructions"
git tag -a plan-6-complete -m "Plan 6 of 10 complete: iPad Swift app shell shipped"
```

---

## Done criteria

All of the following must be true to consider this plan complete:

1. `ipad/iExtend.xcodeproj` opens cleanly in Xcode 15.4+ and builds for the iPad Pro 11" (M4) simulator without warnings.
2. `iExtendKit`, `iExtendUI`, `iExtendInput` are three independent Swift packages with no circular dependencies.
3. All three packages' test schemes pass.
4. The app launches into `WelcomeView`; tapping "Get started" advances through the discover/pair/connecting/live state machine using stubs (full pairing + decode arrives via Plans 5, 7).
5. Each SwiftUI screen visually matches its corresponding `iExtend.html` artboard at 1194×834 within 1% pixel tolerance (snapshot tests committed).
6. `MetalRenderer` blits a `CVPixelBuffer` from `FrameQueue` to a `CAMetalLayer` drawable on a `CADisplayLink` callback — no crashes on empty queue.
7. `EventCapture` produces 32-byte `InputPacket`s for touch and pencil events; `PacketEncoderTests` passes.
8. The end-to-end smoke test against `smoke_loopback` decodes ≥ 7000 HEVC frames in 60 s (gated on Plan 5).
9. Tag `plan-6-complete` exists.

## Honest scope note

The SwiftUI screens (Task 8) are by far the largest chunk in this plan — roughly 10 screens × 100–250 lines each = ~1500 lines of SwiftUI to write and visually match against the design canvas. Recommend running `npm run view` and pinning Safari to the matching artboard while implementing each screen. Snapshot tests catch regressions but cannot tell you the design is wrong on first pass — that's a human eyeballing job.

## Out of scope (handled by later plans)

- **Plan 7**: SPAKE2 PIN exchange replaces the `PairingFlow.swift` stub.
- **Plan 8**: Cursor reprojection compute pass + finalized Pencil pressure/tilt/azimuth payload.
- **Plan 9**: Code signing, provisioning profiles, App Store Connect, TestFlight rollout.
- **Plan 10**: Bench rig integration — a real iPad on a known network with a Phantom-style external camera measuring photon-to-photon latency.
