# Frameworks/

This directory is where Google's prebuilt **WebRTC.xcframework** lives.

The binary is ~280 MB and is tracked via Git LFS (see `.gitattributes`):

```
ipad/Frameworks/**  filter=lfs diff=lfs merge=lfs -text
```

## Download / refresh

The current pin is **M127** from the [stasel/WebRTC](https://github.com/stasel/WebRTC/releases/tag/127.0.0) releases:

```bash
cd ipad/Frameworks
curl -L -o WebRTC.xcframework.zip \
  https://github.com/stasel/WebRTC/releases/download/127.0.0/WebRTC-M127.xcframework.zip
unzip WebRTC.xcframework.zip
rm WebRTC.xcframework.zip
```

The file `WebRTC.version` records the pinned tag and commit hash.

## Why not Swift Package Manager remote?

Google's official WebRTC SPM package does not yet publish a signed XCFramework
with Apple Silicon simulator slices. The stasel build provides:
- `ios-arm64` (device)
- `ios-arm64_x86_64-simulator` (Simulator, including M-chip native)

Link it as a binary target in `iExtendKit/Package.swift`:

```swift
.binaryTarget(
    name: "WebRTC",
    path: "../Frameworks/WebRTC.xcframework"
)
```

## Git LFS setup (first clone)

```bash
git lfs install
git lfs pull
```
