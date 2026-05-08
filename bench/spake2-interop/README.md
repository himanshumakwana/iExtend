# SPAKE2 cross-language interop test

This bench runs the host's SPAKE2 server (Rust, RustCrypto `spake2`) against
the iPad's SPAKE2 client (Swift, `swift-crypto` SPAKE2-P-256) and asserts
they derive the same shared secret. It's the load-bearing CI gate for the
pairing flow — if these drift, no iPad will ever pair to any host.

## What this verifies

- **Group agreement**: both sides on `SPAKE2-P-256-SHA-256-HKDF-HMAC-SHA-256`.
- **Identity strings**: both sides agree `id_a = "iextend.ipad"` and
  `id_b = "iextend.host"` byte-for-byte.
- **Password encoding**: PINs are passed as raw UTF-8 bytes, no normalization
  on either side. (4-digit numeric PINs are ASCII; this is fine.)
- **Wire bytes**: a SPAKE2 element produced by the Rust crate is decodable by
  swift-crypto and vice versa.
- **Derived secret**: 32 bytes, identical between sides.

## Prerequisites

- Rust host: builds on Linux/macOS. Plan 2 + Plan 7 toolchain (Rust 1.90.0).
- iPad client: macOS with Xcode 16+ targeting iPadOS 17+. swift-crypto pinned
  to a version that exposes SPAKE2-P-256.

## Procedure

1. Build the Rust harness:

   ```bash
   cd /home/tops/Projects/iExtend/host
   cargo build -p ix-rtc --release --example spake2_interop_server
   ```

2. Run it; it binds 127.0.0.1:5353 and prints its element B in hex:

   ```bash
   ./target/release/examples/spake2_interop_server --pin 4729
   ```

3. Build and run the Swift harness from Xcode:

   ```bash
   cd /home/tops/Projects/iExtend/ipad
   xcodebuild test -scheme iExtendKit-Tests -destination 'platform=iOS Simulator,name=iPad Pro 11-inch'
   ```

   The `Spake2InteropTests.swift` file connects to 127.0.0.1:5353, runs the
   client side, and asserts:
   - Both sides agree on a 32-byte shared secret.
   - HKDF-SHA256 over the secret produces the same `K_session`.
   - AES-256-GCM encryption with the same nonce/key/plaintext on both sides
     produces byte-identical ciphertext.

## CI status

Pre-Plan 9: this is a manual run before each release branch. The hardware
prerequisite (Apple Silicon Mac) makes auto-CI awkward; we bench-rig it
weekly per Plan 10 §10.2.

## Failure modes

If the test fails with "shared secrets differ", check in this order:
1. Did either crate update? Pin the major version.
2. Did the identity strings drift? `grep -r 'iextend.ipad' host/ ipad/`.
3. Did the PIN encoding change? Both sides must pass raw UTF-8 bytes.
4. Is the iPad simulator using its own random nonce? (Yes — that's expected;
   only the *derived secret* should match, not the wire bytes.)
