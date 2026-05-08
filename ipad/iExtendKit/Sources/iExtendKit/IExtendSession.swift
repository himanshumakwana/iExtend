// IExtendSession — the actor that owns the connection lifecycle.
// Plan 6 establishes the state machine + lifecycle skeleton. Plan 7 fills in
// SPAKE2 pairing, Plan 5/8 fill in the encode/transport details.

import Foundation
import Network

/// Public connection state, mirrored 1:1 with the Rust host's enum
/// (see `iextend.proto SessionState`). Both sides agree by exchanging
/// these on the control DataChannel — never inferred from packet flow.
public enum SessionState: Sendable, Equatable {
    case idle
    case pairing(progress: PairingProgress)
    case connecting(host: HostInfo)
    case live(stats: LiveStats)
    case degraded(reason: DegradedReason, stats: LiveStats)
    case disconnected(reason: DisconnectReason)
    case failed(error: SessionError)
}

public struct PairingProgress: Sendable, Equatable {
    public let step: Step
    public enum Step: Sendable, Equatable {
        case browsingMDNS
        case discoveredHost(HostInfo)
        case enteringPin
        case spake2Handshake
        case exchangingCert
    }
}

public struct HostInfo: Sendable, Equatable, Hashable {
    public let displayName: String        // e.g. "Aman's PC"
    public let ipAddress:   String        // e.g. "192.168.1.42"
    public let pubkeyThumbprint: String   // base64-truncated host pubkey
    public let osHint:      String        // e.g. "Windows 11" (cosmetic only)
    public init(displayName: String, ipAddress: String, pubkeyThumbprint: String, osHint: String) {
        self.displayName = displayName; self.ipAddress = ipAddress
        self.pubkeyThumbprint = pubkeyThumbprint; self.osHint = osHint
    }
}

public struct LiveStats: Sendable, Equatable {
    public let rttMs:       Double
    public let bitrateMbps: Double
    public let fps:         Double
    public let codec:       String   // "av1" | "hevc" | "h264"
    public let droppedFrames: Int
    public init(rttMs: Double, bitrateMbps: Double, fps: Double, codec: String, droppedFrames: Int) {
        self.rttMs = rttMs; self.bitrateMbps = bitrateMbps
        self.fps = fps; self.codec = codec; self.droppedFrames = droppedFrames
    }
    public static let zero = LiveStats(rttMs: 0, bitrateMbps: 0, fps: 0, codec: "—", droppedFrames: 0)
}

public enum DegradedReason: Sendable, Equatable {
    case highRtt           // > 80 ms p95
    case highLoss          // > 3% for 2 s
    case bitrateFloored    // encoder hit the 6 Mbps floor
}

public enum DisconnectReason: Sendable, Equatable {
    case userRequested
    case heartbeatTimeout      // 4 missed heartbeats (1 s detection)
    case dtlsAlert(code: Int)
    case wifiLost
    case backgroundedTooLong   // iOS background grace exceeded
}

public enum SessionError: Error, Sendable, Equatable {
    case mdnsBrowseFailed(message: String)
    case pinIncorrect
    case pairingTimeout
    case spake2VerifierMismatch
    case certPinningMismatch
    case codecExhausted        // entire fallback chain failed
    case retriesExhausted
    case unknown(message: String)
}

// MARK: - Session actor

/// Sole owner of the connection lifecycle. UI binds to `stateStream` for
/// updates. Commands are async fns; concurrency is enforced by the actor.
public actor IExtendSession {
    private(set) public var state: SessionState = .idle
    private var stateContinuations: [AsyncStream<SessionState>.Continuation] = []
    private var pinnedHosts: Set<HostInfo> = []      // cert-pinned (after pairing)

    public init() {}

    /// Subscribe to state changes. Cancels when the consumer drops the
    /// AsyncStream. Multiple subscribers are supported.
    public func stateStream() -> AsyncStream<SessionState> {
        AsyncStream { continuation in
            self.stateContinuations.append(continuation)
            continuation.yield(self.state)
            continuation.onTermination = { @Sendable [weak self] _ in
                Task { await self?.dropContinuation(continuation) }
            }
        }
    }

    private func dropContinuation(_ c: AsyncStream<SessionState>.Continuation) {
        // identity-compare not possible on Continuation; in practice subscribers
        // are short-lived (per-screen) and we just leave terminated continuations
        // in the array — yields are no-ops.
        _ = c
    }

    private func transition(to newState: SessionState) {
        self.state = newState
        for c in stateContinuations { c.yield(newState) }
    }

    // MARK: Public commands (called from UI)

    public func startBrowsing() async {
        transition(to: .pairing(progress: .init(step: .browsingMDNS)))
        // Plan 7: Signaling.swift drives this — for Plan 6 we just place the marker.
    }

    public func discovered(host: HostInfo) async {
        transition(to: .pairing(progress: .init(step: .discoveredHost(host))))
    }

    public func awaitPin() async {
        transition(to: .pairing(progress: .init(step: .enteringPin)))
    }

    public func submitPin(_ pin: String) async {
        // Plan 7: invokes PairingFlow.runSpake2(pin:). For Plan 6 a stub.
        transition(to: .pairing(progress: .init(step: .spake2Handshake)))
        // Simulate the next step in the lifecycle — exchanging cert.
        transition(to: .pairing(progress: .init(step: .exchangingCert)))
    }

    public func connect(to host: HostInfo) async {
        transition(to: .connecting(host: host))
        // Plan 5: PeerConnection.swift connects WebRTC. For Plan 6 the next
        // legitimate transition (.live) is driven by the smoke peer in Task 10.
    }

    public func wentLive(stats: LiveStats) async {
        transition(to: .live(stats: stats))
    }

    public func degraded(_ reason: DegradedReason, stats: LiveStats) async {
        transition(to: .degraded(reason: reason, stats: stats))
    }

    public func disconnect(reason: DisconnectReason) async {
        transition(to: .disconnected(reason: reason))
    }

    public func reset() async {
        transition(to: .idle)
    }

    public func fail(_ err: SessionError) async {
        transition(to: .failed(error: err))
    }

    // MARK: Pinned-host store (Plan 7 expands this)

    public func remember(host: HostInfo) { pinnedHosts.insert(host) }
    public func forget(host: HostInfo) { pinnedHosts.remove(host) }
    public func isPinned(_ host: HostInfo) -> Bool { pinnedHosts.contains(host) }
}
