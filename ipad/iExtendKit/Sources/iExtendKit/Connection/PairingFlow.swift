// PairingFlow.swift
//
// simple-pair-v0 client for the iPad onboarding flow.
//
// SECURITY NOTE:
//   This implementation uses the plaintext simple-pair-v0 protocol (PSimpleHello /
//   PSimpleAck). The PIN and device public key are transmitted in clear JSON over TCP.
//   Any passive observer with LAN access during the 60-second pairing window can
//   capture the PIN and replay it to a new session (replay attack). A MitM attacker
//   who can intercept the TCP stream can substitute their own public key.
//
//   This is acceptable for MVP / dev / demo on a personal router or hotspot segment.
//   The production upgrade path is to port the Rust SPAKE2 implementation to Swift
//   via xcframework (tracked as future work). Once SPAKE2 lands, the iPad switches
//   to the PStart / PResponse / PCertReq / PCertOk flow and simple-pair-v0 is
//   deprecated.
//
// SPAKE2 stub:
//   The original PairingFlow.run() stub is kept at the bottom under `PairingFlowSPAKE2`
//   so the Plan 7 SPAKE2 contract is preserved. fake-ipad (Rust) continues using the
//   SPAKE2 path unchanged.

import Foundation
import Network
#if canImport(Crypto)
import Crypto
#endif
#if canImport(Security)
import Security
#endif

// MARK: - Public result types

/// Returned to the UI on successful pairing.
public struct PairResult {
    /// UUID assigned by the host daemon.
    public let pairId: String
    /// Host Ed25519 verifying key (base64-encoded, 32 bytes decoded).
    public let hostPubkeyB64: String
    /// The display name we sent during the handshake.
    public let displayName: String
    /// Unix-epoch timestamp of the pairing.
    public let pairedAt: Date
}

/// Errors that can surface during pairing.
public enum PairFlowError: Error, LocalizedError {
    case networkFailed(Error)
    case wireError(PairWireError)
    case serverRejected(String)
    case malformedResponse(String)
    case keychainError(OSStatus)
    case timeout

    public var errorDescription: String? {
        switch self {
        case .networkFailed(let e):     return "Network error: \(e.localizedDescription)"
        case .wireError(let e):         return "Wire protocol error: \(e)"
        case .serverRejected(let r):    return "Server rejected pairing: \(r)"
        case .malformedResponse(let r): return "Malformed server response: \(r)"
        case .keychainError(let s):     return "Keychain error: \(s)"
        case .timeout:                  return "Connection timed out"
        }
    }
}

// MARK: - PairingFlow (simple-pair-v0)

/// Runs the simple-pair-v0 handshake over a plain TCP connection.
///
/// Usage:
/// ```swift
/// let result = try await PairingFlow.pair(
///     host: "192.168.1.10", port: 12345,
///     pin: "1234", displayName: "iPad of Bob")
/// ```
///
/// On success the host's `pair_id` and `host_pubkey_b64` are stored to Keychain
/// via `PairKeychain` so subsequent reconnects can verify the host.
public struct PairingFlow {

    // MARK: Main entry point

    /// Run the simple-pair-v0 handshake. Throws `PairFlowError` on failure.
    public static func pair(
        host: String,
        port: UInt16,
        pin: String,
        displayName: String
    ) async throws -> PairResult {

        // 1. Generate a fresh Curve25519 signing key. We'll store the seed on
        //    success; for now we just need the 32-byte public key to send.
        let privateKey = Curve25519.Signing.PrivateKey()
        let publicKeyBytes = privateKey.publicKey.rawRepresentation // 32 bytes
        let publicKeyB64 = publicKeyBytes.base64EncodedString()

        // 2. Build the PSimpleHello JSON body.
        let helloPayload: [String: String] = [
            "pin": pin,
            "client_pubkey_b64": publicKeyB64,
            "display_name": displayName,
        ]
        guard let helloBody = try? JSONEncoder().encode(helloPayload) else {
            throw PairFlowError.malformedResponse("could not encode hello payload")
        }
        let helloFrame = PairWire(kind: .pSimpleHello, body: helloBody)

        // 3. Dial the host and run the single round-trip.
        let ackFrame = try await performHandshake(
            host: host,
            port: port,
            helloFrame: helloFrame
        )

        // 4. Parse the PSimpleAck body.
        guard let ackDict = try? JSONDecoder().decode(
            [String: AnyCodable].self,
            from: ackFrame.body
        ) else {
            throw PairFlowError.malformedResponse("PSimpleAck body is not valid JSON")
        }

        // 5. Check ok flag.
        if let okVal = ackDict["ok"]?.value as? Bool, !okVal {
            let errMsg = (ackDict["error"]?.value as? String) ?? "unknown error"
            throw PairFlowError.serverRejected(errMsg)
        }

        guard let pairId = ackDict["pair_id"]?.value as? String,
              let hostPubkeyB64 = ackDict["host_pubkey_b64"]?.value as? String
        else {
            throw PairFlowError.malformedResponse("PSimpleAck missing pair_id or host_pubkey_b64")
        }

        let pairedAt = Date()

        // 6. Store seed + metadata to Keychain.
        let keychainEntry = PairKeychainEntry(
            pairId: pairId,
            hostPubkeyB64: hostPubkeyB64,
            displayName: displayName,
            pairedAtUnix: Int64(pairedAt.timeIntervalSince1970),
            clientPrivkeySeed: privateKey.rawRepresentation
        )
        let saveStatus = PairKeychain.save(keychainEntry)
        if saveStatus != errSecSuccess && saveStatus != errSecDuplicateItem {
            throw PairFlowError.keychainError(saveStatus)
        }

        return PairResult(
            pairId: pairId,
            hostPubkeyB64: hostPubkeyB64,
            displayName: displayName,
            pairedAt: pairedAt
        )
    }

    // MARK: USB entry point

    /// Variant of `pair(host:port:pin:displayName:)` that reuses an
    /// already-accepted `NWConnection` rather than dialing a host. Used by
    /// `USBPairingListener` once the laptop's daemon has opened a tunneled
    /// TCP socket through usbmuxd.
    ///
    /// The connection passed in MUST have been `.start()`ed by the caller
    /// (NWListener-accepted connections start in `.preparing` state and
    /// transition to `.ready` once started). This helper waits for `.ready`
    /// before sending.
    public static func pairOverExistingConnection(
        connection: NWConnection,
        pin: String,
        displayName: String
    ) async throws -> PairResult {

        // Same as `pair`: generate a fresh Curve25519 signing key.
        let privateKey = Curve25519.Signing.PrivateKey()
        let publicKeyBytes = privateKey.publicKey.rawRepresentation
        let publicKeyB64 = publicKeyBytes.base64EncodedString()

        let helloPayload: [String: String] = [
            "pin": pin,
            "client_pubkey_b64": publicKeyB64,
            "display_name": displayName,
        ]
        guard let helloBody = try? JSONEncoder().encode(helloPayload) else {
            throw PairFlowError.malformedResponse("could not encode hello payload")
        }
        let helloFrame = PairWire(kind: .pSimpleHello, body: helloBody)

        let ackFrame = try await sendAndAwaitAck(
            connection: connection,
            helloFrame: helloFrame
        )

        guard let ackDict = try? JSONDecoder().decode(
            [String: AnyCodable].self,
            from: ackFrame.body
        ) else {
            throw PairFlowError.malformedResponse("PSimpleAck body is not valid JSON")
        }
        if let okVal = ackDict["ok"]?.value as? Bool, !okVal {
            let errMsg = (ackDict["error"]?.value as? String) ?? "unknown error"
            throw PairFlowError.serverRejected(errMsg)
        }
        guard let pairId = ackDict["pair_id"]?.value as? String,
              let hostPubkeyB64 = ackDict["host_pubkey_b64"]?.value as? String
        else {
            throw PairFlowError.malformedResponse("PSimpleAck missing pair_id or host_pubkey_b64")
        }

        let pairedAt = Date()
        let keychainEntry = PairKeychainEntry(
            pairId: pairId,
            hostPubkeyB64: hostPubkeyB64,
            displayName: displayName,
            pairedAtUnix: Int64(pairedAt.timeIntervalSince1970),
            clientPrivkeySeed: privateKey.rawRepresentation
        )
        let saveStatus = PairKeychain.save(keychainEntry)
        if saveStatus != errSecSuccess && saveStatus != errSecDuplicateItem {
            throw PairFlowError.keychainError(saveStatus)
        }

        return PairResult(
            pairId: pairId,
            hostPubkeyB64: hostPubkeyB64,
            displayName: displayName,
            pairedAt: pairedAt
        )
    }

    /// Wait for an existing NWConnection to become `.ready`, then send
    /// `helloFrame` and read back one frame. Mirrors `performHandshake`'s
    /// behavior but doesn't dial a host.
    private static func sendAndAwaitAck(
        connection: NWConnection,
        helloFrame: PairWire
    ) async throws -> PairWire {
        try await withCheckedThrowingContinuation { cont in
            var resolved = false

            func finish(_ result: Result<PairWire, Error>) {
                guard !resolved else { return }
                resolved = true
                cont.resume(with: result)
            }

            // The connection may already be .ready when we attach the
            // handler; if so we still get a .ready callback once.
            connection.stateUpdateHandler = { state in
                switch state {
                case .ready:
                    do {
                        let encoded = try helloFrame.encode()
                        connection.send(
                            content: encoded,
                            completion: .contentProcessed { sendErr in
                                if let e = sendErr {
                                    finish(.failure(PairFlowError.networkFailed(e)))
                                    return
                                }
                                readFrame(connection: connection) { result in
                                    finish(result)
                                }
                            }
                        )
                    } catch {
                        finish(.failure(PairFlowError.networkFailed(error)))
                    }
                case .failed(let e):
                    finish(.failure(PairFlowError.networkFailed(e)))
                case .cancelled:
                    if !resolved {
                        finish(.failure(PairFlowError.timeout))
                    }
                default:
                    break
                }
            }

            // 30-second handshake timeout matches the Wi-Fi flow.
            DispatchQueue.global().asyncAfter(deadline: .now() + 30) {
                if !resolved {
                    finish(.failure(PairFlowError.timeout))
                }
            }
        }
    }

    // MARK: TCP handshake

    /// Opens a TCP connection, sends `helloFrame`, and reads back one frame.
    private static func performHandshake(
        host: String,
        port: UInt16,
        helloFrame: PairWire
    ) async throws -> PairWire {

        let endpoint = NWEndpoint.hostPort(
            host: NWEndpoint.Host(host),
            port: NWEndpoint.Port(rawValue: port)!
        )
        let connection = NWConnection(to: endpoint, using: .tcp)

        // Use a continuation to bridge NWConnection's callback-based API.
        return try await withCheckedThrowingContinuation { cont in
            var resolved = false

            func finish(_ result: Result<PairWire, Error>) {
                guard !resolved else { return }
                resolved = true
                connection.cancel()
                cont.resume(with: result)
            }

            connection.stateUpdateHandler = { state in
                switch state {
                case .ready:
                    // Connected — send the hello frame.
                    do {
                        let encoded = try helloFrame.encode()
                        connection.send(
                            content: encoded,
                            completion: .contentProcessed { sendErr in
                                if let e = sendErr {
                                    finish(.failure(PairFlowError.networkFailed(e)))
                                    return
                                }
                                // After sending, read the response header (8 bytes) then body.
                                readFrame(connection: connection) { result in
                                    finish(result)
                                }
                            }
                        )
                    } catch {
                        finish(.failure(PairFlowError.networkFailed(error)))
                    }

                case .failed(let e):
                    finish(.failure(PairFlowError.networkFailed(e)))

                case .cancelled:
                    if !resolved {
                        finish(.failure(PairFlowError.timeout))
                    }

                default:
                    break
                }
            }

            connection.start(queue: .global(qos: .userInitiated))

            // 30-second overall timeout.
            DispatchQueue.global().asyncAfter(deadline: .now() + 30) {
                if !resolved {
                    finish(.failure(PairFlowError.timeout))
                }
            }
        }
    }

    /// Read one complete PairWire frame from `connection` and call `handler`.
    private static func readFrame(
        connection: NWConnection,
        handler: @escaping (Result<PairWire, Error>) -> Void
    ) {
        // Read the 8-byte header first.
        connection.receive(
            minimumIncompleteLength: PairWire.headerLen,
            maximumLength: PairWire.headerLen
        ) { headerData, _, _, headerErr in
            if let e = headerErr {
                handler(.failure(PairFlowError.networkFailed(e)))
                return
            }
            guard let headerData, headerData.count >= PairWire.headerLen else {
                handler(.failure(PairFlowError.malformedResponse("truncated header")))
                return
            }
            // Parse body length from header bytes 6..7 (big-endian u16).
            let bodyLen = Int(
                UInt16(headerData[headerData.startIndex + 6]) << 8 |
                UInt16(headerData[headerData.startIndex + 7])
            )
            if bodyLen == 0 {
                // No body — decode immediately.
                do {
                    let frame = try PairWire.decode(headerData)
                    handler(.success(frame))
                } catch let e as PairWireError {
                    handler(.failure(PairFlowError.wireError(e)))
                } catch {
                    handler(.failure(PairFlowError.networkFailed(error)))
                }
                return
            }
            // Read the body.
            connection.receive(
                minimumIncompleteLength: bodyLen,
                maximumLength: bodyLen
            ) { bodyData, _, _, bodyErr in
                if let e = bodyErr {
                    handler(.failure(PairFlowError.networkFailed(e)))
                    return
                }
                guard let bodyData, bodyData.count >= bodyLen else {
                    handler(.failure(PairFlowError.malformedResponse("truncated body")))
                    return
                }
                var fullFrame = headerData
                fullFrame.append(bodyData)
                do {
                    let frame = try PairWire.decode(fullFrame)
                    handler(.success(frame))
                } catch let e as PairWireError {
                    handler(.failure(PairFlowError.wireError(e)))
                } catch {
                    handler(.failure(PairFlowError.networkFailed(error)))
                }
            }
        }
    }
}

// MARK: - PairKeychain

/// Data persisted to Keychain after a successful pairing.
public struct PairKeychainEntry: Codable {
    public let pairId: String
    public let hostPubkeyB64: String
    public let displayName: String
    public let pairedAtUnix: Int64
    /// Raw 32-byte Curve25519 private key seed. The corresponding public key
    /// was sent to the host during PSimpleHello.
    public let clientPrivkeySeed: Data
}

/// Thin Keychain wrapper for pairing credentials.
///
/// Items are stored under the service name `"iextend.pair.v0"` keyed by
/// `pairId`. Multiple paired hosts are supported (one item per host).
public struct PairKeychain {

    private static let service = "iextend.pair.v0"

    /// Save a pairing entry. Returns `errSecSuccess` or `errSecDuplicateItem`
    /// (caller may treat duplicate as a no-op or call `update` instead).
    @discardableResult
    public static func save(_ entry: PairKeychainEntry) -> OSStatus {
        guard let data = try? JSONEncoder().encode(entry) else { return errSecParam }
        let query: [String: Any] = [
            kSecClass as String:            kSecClassGenericPassword,
            kSecAttrService as String:      service,
            kSecAttrAccount as String:      entry.pairId,
            kSecValueData as String:        data,
            kSecAttrAccessible as String:   kSecAttrAccessibleAfterFirstUnlock,
        ]
        // Delete any stale item first so we can upsert cleanly.
        SecItemDelete(query as CFDictionary)
        return SecItemAdd(query as CFDictionary, nil)
    }

    /// Load a pairing entry by pair_id. Returns nil if not found.
    public static func load(pairId: String) -> PairKeychainEntry? {
        let query: [String: Any] = [
            kSecClass as String:       kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: pairId,
            kSecReturnData as String:  true,
            kSecMatchLimit as String:  kSecMatchLimitOne,
        ]
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        guard status == errSecSuccess,
              let data = item as? Data,
              let entry = try? JSONDecoder().decode(PairKeychainEntry.self, from: data)
        else { return nil }
        return entry
    }

    /// List all stored pairing entries.
    public static func listAll() -> [PairKeychainEntry] {
        let query: [String: Any] = [
            kSecClass as String:       kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecReturnData as String:  true,
            kSecMatchLimit as String:  kSecMatchLimitAll,
        ]
        var items: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &items)
        guard status == errSecSuccess,
              let dataArray = items as? [Data]
        else { return [] }
        return dataArray.compactMap {
            try? JSONDecoder().decode(PairKeychainEntry.self, from: $0)
        }
    }

    /// Remove all stored pairing entries (e.g. "unpair all").
    @discardableResult
    public static func deleteAll() -> OSStatus {
        let query: [String: Any] = [
            kSecClass as String:       kSecClassGenericPassword,
            kSecAttrService as String: service,
        ]
        return SecItemDelete(query as CFDictionary)
    }
}

// MARK: - AnyCodable (minimal inline helper)

/// A minimal Codable wrapper for heterogeneous JSON values (String, Bool, etc.).
/// We keep this private so it doesn't pollute the public API.
private struct AnyCodable: Codable {
    let value: Any

    init(_ value: Any) { self.value = value }

    init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if let b = try? c.decode(Bool.self)   { value = b; return }
        if let i = try? c.decode(Int.self)    { value = i; return }
        if let d = try? c.decode(Double.self) { value = d; return }
        if let s = try? c.decode(String.self) { value = s; return }
        value = NSNull()
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        switch value {
        case let b as Bool:   try c.encode(b)
        case let i as Int:    try c.encode(i)
        case let d as Double: try c.encode(d)
        case let s as String: try c.encode(s)
        default:              try c.encodeNil()
        }
    }
}

// MARK: - Legacy SPAKE2 stub (Plan 7 contract — do not remove)

/// The original Plan 7 SPAKE2 stub is retained here so the host-side
/// `fake-ipad` SPAKE2 test path remains the documented canonical protocol.
/// The iPad production flow uses `PairingFlow.pair(…)` above until SPAKE2
/// is ported to Swift via xcframework.
public struct PairingFlowSPAKE2 {
    public let pin: String
    public let host: String
    public let port: UInt16

    public init(pin: String, host: String, port: UInt16) {
        self.pin = pin
        self.host = host
        self.port = port
    }

    public enum PairingError: Error {
        case spake2(String)
        case wire(PairWireError)
        case aeadFail
        case rateLimited
        case io(Error)
    }

    public func run() async throws -> PairingResult {
        throw PairingError.spake2("Plan 7 SPAKE2 implementation pending Swift xcframework port")
    }
}

/// SPAKE2 result type (Plan 7 contract).
public struct PairingResult {
    public let hostPublicKey: Data    // 32-byte Ed25519
    public let pairId: String         // UUID v4
    public let signedDeviceCert: Data // Ed25519 sig + payload
}
