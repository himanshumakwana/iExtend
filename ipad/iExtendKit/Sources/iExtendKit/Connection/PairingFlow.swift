// PairingFlow.swift
//
// SPAKE2-P256 client mirroring host/crates/ix-rtc/src/pairing.rs.
// Cipher suite (must match host exactly):
//   SPAKE2-P256-SHA256-HKDF-HMAC-SHA256, AES-256-GCM, Ed25519 device keys.
//
// Status: Plan 7 stub. The SPAKE2 client uses swift-crypto's SPAKE2 primitive
// (resolves at compile-time on macOS / iPadOS). The TCP transport uses
// Network.framework. End-to-end interop with the Rust host is a manual
// XCTest run on macOS — see `bench/spake2-interop/README.md`.

import Foundation
#if canImport(Crypto)
import Crypto
#endif

public enum PairingError: Error {
    case spake2(String)
    case wire(PairWireError)
    case aeadFail
    case rateLimited
    case io(Error)
}

public struct PairingFlow {
    public let pin: String
    public let host: String
    public let port: UInt16

    public init(pin: String, host: String, port: UInt16) {
        self.pin = pin
        self.host = host
        self.port = port
    }

    /// Run the full SPAKE2 + cert-exchange flow. On success, the host's
    /// Ed25519 pubkey has been pinned to the keychain and the iPad's pubkey
    /// is signed into a long-lived device cert returned to the caller.
    ///
    /// Stub for Plan 7. Real implementation:
    ///
    /// 1. NWConnection to (host, port) over TCP.
    /// 2. Build SPAKE2 P-256 element A (using swift-crypto's
    ///    `SPAKE2-P-256-SHA-256-HKDF-HMAC-SHA-256` group).
    /// 3. Send `PStart { spake_a }` via PairWire.
    /// 4. Receive `PResponse { spake_b, hk_verifier }`.
    /// 5. Run SPAKE2 finish; HKDF-SHA256 derive K_session.
    /// 6. Verify hk_verifier (HMAC-SHA256 over derived secret).
    /// 7. Generate iPad Ed25519 keypair if needed; AES-GCM-wrap and send
    ///    `PCertReq { ipad_pubkey || display_name || iPadOS_version }`.
    /// 8. Receive `PCertOk { aead(host_pubkey || cert) }`; store host_pubkey
    ///    in keychain and `cert` in the iExtend app group.
    /// 9. Close TCP. Subsequent connects use the cert via DTLS in Plan 5.
    public func run() async throws -> PairingResult {
        throw PairingError.spake2("Plan 7 implementation pending macOS test")
    }
}

/// Returned to UI on success: the pinned host pubkey + the signed iPad cert.
public struct PairingResult {
    public let hostPublicKey: Data    // 32-byte Ed25519
    public let pairId: String         // UUID v4
    public let signedDeviceCert: Data // Ed25519 sig + payload
}
