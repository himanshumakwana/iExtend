# iExtend — iPad App

iPadOS 17+ second-screen client. Receives an HEVC/AV1 video stream over
WebRTC from the Plan 5 Rust host, renders it at up to 120 Hz on a `CAMetalLayer`,
and sends touch / Apple Pencil events back over a WebRTC DataChannel.

## Architecture

Three Swift Package Manager modules:

| Package | Purpose | Dependencies |
|---------|---------|-------------|
| `iExtendKit` | Connection (WebRTC/mDNS), decode (VideoToolbox), render (Metal) — no UI | WebRTC.xcframework, swift-atomics, swift-crypto |
| `iExtendUI` | SwiftUI screens and view models | iExtendKit |
| `iExtendInput` | UIKit touch/Pencil event capture, 32-byte packet encoder | — |

Dependency direction: `App → UI → Kit + Input`. UI never imports Input; Input never imports UI.

## Prerequisites

| Tool | Minimum version |
|------|-----------------|
| Xcode | 16.0+ |
| macOS | 14 (Sonoma) |
| iOS deployment target | iPadOS 17.0 |
| Device | iPad Pro with ProMotion (A12X+) |
| XcodeGen | 2.40+ (`brew install xcodegen`) |

## First-time setup

### 1. Clone and install Git LFS

```bash
git lfs install
git lfs pull   # pulls WebRTC.xcframework (~280 MB)
```

### 2. Download WebRTC.xcframework (if not using LFS)

```bash
cd ipad/Frameworks
curl -L -o WebRTC.xcframework.zip \
  https://github.com/stasel/WebRTC/releases/download/127.0.0/WebRTC-M127.xcframework.zip
unzip WebRTC.xcframework.zip && rm WebRTC.xcframework.zip
```

### 3. Generate the Xcode project

```bash
brew install xcodegen   # skip if already installed
cd ipad
xcodegen generate
```

### 4. Open and build

```bash
open iExtend.xcodeproj
```

Select the `iPad Pro 11-inch (M4)` simulator and press **⌘R**.

> **Code signing**: Leave `DEVELOPMENT_TEAM` empty for simulator builds.
> Plan 9 configures provisioning for device deployment.

## Build from command line

```bash
cd ipad

# Simulator build
xcodebuild \
  -project iExtend.xcodeproj \
  -scheme iExtend \
  -sdk iphonesimulator \
  -destination 'platform=iOS Simulator,name=iPad Pro 11-inch (M4)' \
  build

# Device build (requires signing — Plan 9)
xcodebuild \
  -project iExtend.xcodeproj \
  -scheme iExtend \
  -sdk iphoneos \
  -destination 'generic/platform=iOS' \
  build
```

## Running the tests

```bash
cd ipad

# iExtendKit unit tests (state machine, SPSC ring, codec probe)
xcodebuild test \
  -project iExtend.xcodeproj \
  -scheme iExtendKit \
  -destination 'platform=iOS Simulator,name=iPad Pro 11-inch (M4)'

# iExtendInput unit tests (packet encoder, event mapping)
xcodebuild test \
  -project iExtend.xcodeproj \
  -scheme iExtendInput \
  -destination 'platform=iOS Simulator,name=iPad Pro 11-inch (M4)'

# iExtendUI snapshot tests
xcodebuild test \
  -project iExtend.xcodeproj \
  -scheme iExtendUI \
  -destination 'platform=iOS Simulator,name=iPad Pro 11-inch (M4)'
```

## Plan status

| Plan | Feature | Status |
|------|---------|--------|
| 6 | App shell, Metal renderer, mDNS discover, UI screens | This plan |
| 7 | SPAKE2 PIN pairing, DTLS cert pinning | `PairingFlow.swift` stub |
| 8 | Reprojection, cursor mask, Pencil pressure | `Reproject.swift` stub |
| 9 | Code signing, TestFlight, App Store Connect | — |

## Key files

- `iExtend/ContentView.swift` — root state-machine navigator
- `iExtendKit/Sources/iExtendKit/IExtendSession.swift` — the connection actor
- `iExtendKit/Sources/iExtendKit/Render/MetalRenderer.swift` — 120 Hz render loop
- `iExtendUI/Sources/iExtendUI/Live/LiveView.swift` — CAMetalLayer host + toolbar overlay
- `iExtendUI/Sources/iExtendUI/Settings/SettingsView.swift` — iPadOS sidebar settings

## Visual reference

Open `iExtend.html` (repo root) in a browser to see the design canvas.
Each SwiftUI screen is a 1:1 port of the corresponding artboard.
