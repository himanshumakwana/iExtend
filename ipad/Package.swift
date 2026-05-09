// swift-tools-version:5.9
// Plan 6 of 10 — iPad Swift app shell.
// Three SPM packages live under one umbrella so the Xcode project can pull
// each as a target and Plan 7/8 implementers can extend them in isolation.

import PackageDescription

let package = Package(
    name: "iExtend",
    platforms: [
        .iOS(.v17),
        // macOS is declared so `swift test` on the macOS-15 CI runner can
        // build cross-platform code (Network framework, AsyncStream, Task)
        // that needs macOS 10.15+. UIKit/iOS-only files are gated behind
        // `#if canImport(UIKit)` so they're excluded on macOS host builds.
        .macOS(.v14),
    ],
    products: [
        .library(name: "iExtendKit",   targets: ["iExtendKit"]),
        .library(name: "iExtendUI",    targets: ["iExtendUI"]),
        .library(name: "iExtendInput", targets: ["iExtendInput"]),
    ],
    dependencies: [
        // swift-atomics: lock-free SPSC frame queue (FrameQueue.swift)
        .package(url: "https://github.com/apple/swift-atomics.git", from: "1.2.0"),
        // swift-crypto: SPAKE2 PAKE in Plan 7 (PairingFlow.swift)
        .package(url: "https://github.com/apple/swift-crypto.git", from: "3.0.0"),
    ],
    targets: [
        .target(
            name: "iExtendKit",
            dependencies: [
                .product(name: "Atomics",  package: "swift-atomics"),
                .product(name: "Crypto",   package: "swift-crypto"),
            ],
            path: "iExtendKit/Sources/iExtendKit"
        ),
        .target(
            name: "iExtendUI",
            dependencies: ["iExtendKit"],
            path: "iExtendUI/Sources/iExtendUI"
        ),
        .target(
            name: "iExtendInput",
            dependencies: ["iExtendKit"],
            path: "iExtendInput/Sources/iExtendInput"
        ),
        .testTarget(
            name: "iExtendKitTests",
            dependencies: ["iExtendKit"],
            path: "iExtendKit/Tests/iExtendKitTests"
        ),
        .testTarget(
            name: "iExtendInputTests",
            dependencies: ["iExtendInput"],
            path: "iExtendInput/Tests/iExtendInputTests",
            resources: [.copy("Fixtures")]
        ),
    ]
)

// Note: Google's WebRTC.framework is a binary target that ships as an XCFramework.
// It's not added here because SPM binary targets need a checksum — the macOS
// engineer pins the version + checksum during initial Xcode integration.
// See ipad/Frameworks/README.md for the drop-in instructions.
