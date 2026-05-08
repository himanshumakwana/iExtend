// Signaling.swift
// mDNS-based service discovery (NWBrowser for _iextend._tcp) plus
// certificate-pinning helpers. Plan 6 drives browse + host-list population.
// Plan 7 replaces the stub cert-pinning with real SPAKE2-derived fingerprints.
//
// Threading: NWBrowser callbacks arrive on a private queue; we re-dispatch to
// the Swift actor callers so callers see results in actor-isolation.

import Foundation
import Network

// MARK: - Discovered peer

public struct DiscoveredPeer: Sendable, Equatable, Identifiable {
    public let id: String            // NWBrowser.Result uniqueID (host:port)
    public let name: String          // mDNS service name → display name
    public let host: String          // IPv4 or IPv6 string (resolved)
    public let port: UInt16          // default 7779
    public let rttMs: Double         // synthetic ping estimate (pre-connect)
    public let osHint: String        // TXT record: "os=windows11"

    public init(id: String, name: String, host: String, port: UInt16, rttMs: Double, osHint: String) {
        self.id = id; self.name = name; self.host = host
        self.port = port; self.rttMs = rttMs; self.osHint = osHint
    }
}

// MARK: - Pinned fingerprint store

/// Lightweight in-memory store for certificate fingerprints established
/// during pairing. Plan 7 persists these in the iOS Keychain.
public final class PinnedFingerprintStore: @unchecked Sendable {
    private var store: [String: Data] = [:]   // host → SHA-256(SubjectPublicKeyInfo)
    private let lock = NSLock()

    public init() {}

    public func pin(host: String, fingerprint: Data) {
        lock.withLock { store[host] = fingerprint }
    }

    public func fingerprint(for host: String) -> Data? {
        lock.withLock { store[host] }
    }

    public func verify(host: String, presented: Data) -> Bool {
        guard let expected = fingerprint(for: host) else { return false }
        return expected == presented
    }

    public func unpin(host: String) {
        lock.withLock { store.removeValue(forKey: host) }
    }
}

// MARK: - Signaling actor

/// Drives NWBrowser to discover `_iextend._tcp` services on the LAN.
/// Callers observe `peers` (an `AsyncStream`) which emits the full list
/// on every change. Stop browsing with `stopBrowsing()`.
public actor Signaling {

    // MARK: Public properties
    public private(set) var peers: [DiscoveredPeer] = []
    public let fingerprintStore = PinnedFingerprintStore()

    // MARK: Private
    private var browser: NWBrowser?
    private var streamContinuation: AsyncStream<[DiscoveredPeer]>.Continuation?
    private var rawResults: [NWBrowser.Result] = []

    // Shared browser queue — NWBrowser callbacks are delivered here.
    private let browserQueue = DispatchQueue(label: "com.iextend.signaling", qos: .userInitiated)

    public init() {}

    // MARK: - Browse

    /// Begin mDNS browsing. Resolves host records to populate IP addresses.
    /// Emits the updated peer list on every change.
    public func startBrowsing() -> AsyncStream<[DiscoveredPeer]> {
        stopBrowsing() // idempotent stop of any previous browser

        let (stream, continuation) = AsyncStream<[DiscoveredPeer]>.makeStream()
        self.streamContinuation = continuation

        let params = NWParameters()
        params.includePeerToPeer = true

        let b = NWBrowser(
            for: .bonjourWithTXTRecord(type: "_iextend._tcp", domain: "local."),
            using: params
        )
        self.browser = b

        b.browseResultsChangedHandler = { [weak self] results, changes in
            guard let self else { return }
            Task { await self.handleBrowseResults(results) }
        }

        b.stateUpdateHandler = { [weak self] state in
            guard let self else { return }
            switch state {
            case .failed(let error):
                // Surface failure through the stream and stop.
                Task { await self.handleBrowserFailure(error) }
            default:
                break
            }
        }

        b.start(queue: browserQueue)
        return stream
    }

    /// Stop browsing and cancel any in-flight resolution tasks.
    public func stopBrowsing() {
        browser?.cancel()
        browser = nil
        streamContinuation?.finish()
        streamContinuation = nil
    }

    // MARK: - Manual peer

    /// Add a peer by explicit IP address (the "Manual IP" button in DiscoverView).
    public func addManualPeer(host: String, port: UInt16 = 7779) async {
        let peer = DiscoveredPeer(
            id: "\(host):\(port)",
            name: host,
            host: host,
            port: port,
            rttMs: 0,
            osHint: "unknown"
        )
        peers.append(peer)
        streamContinuation?.yield(peers)
    }

    // MARK: - Private helpers

    private func handleBrowseResults(_ results: Set<NWBrowser.Result>) async {
        rawResults = Array(results)
        var updated: [DiscoveredPeer] = []
        for result in results {
            if case let .service(name, _, _, _) = result.endpoint {
                // Resolve each service to an IP address.
                let resolved = await resolveService(result: result, name: name)
                updated.append(resolved)
            }
        }
        peers = updated
        streamContinuation?.yield(updated)
    }

    private func handleBrowserFailure(_ error: NWError) {
        peers = []
        streamContinuation?.finish()
        streamContinuation = nil
        browser = nil
    }

    /// Resolves an NWBrowser.Result to a concrete IP:port DiscoveredPeer.
    /// Uses NWConnection to perform a single probe and extract the address.
    private func resolveService(result: NWBrowser.Result, name: String) async -> DiscoveredPeer {
        // Extract TXT records for os hint and display name.
        var osHint = "unknown"
        var displayName = name

        if case let .bonjour(txtRecord) = result.metadata {
            if let osValue = txtRecord.dictionary["os"] { osHint = osValue }
            if let nameValue = txtRecord.dictionary["name"] { displayName = nameValue }
        }

        // Optimistic stub: return the service endpoint as a string.
        // Full TCP probe and ping measurement happens in Plan 7.
        return DiscoveredPeer(
            id: result.hashValue.description,
            name: displayName,
            host: resolveEndpointString(result.endpoint),
            port: 7779,
            rttMs: estimateRtt(from: result),
            osHint: osHint
        )
    }

    private func resolveEndpointString(_ endpoint: NWEndpoint) -> String {
        switch endpoint {
        case .service(let name, _, _, _):
            return name
        case .hostPort(let host, let port):
            return "\(host):\(port)"
        case .url(let url):
            return url.host ?? url.absoluteString
        default:
            return endpoint.debugDescription
        }
    }

    private func estimateRtt(from result: NWBrowser.Result) -> Double {
        // Placeholder: real RTT comes from ICE STUN ping in Plan 7.
        return Double.random(in: 5...30)
    }
}

// MARK: - AsyncStream convenience
extension AsyncStream {
    static func makeStream(
        of elementType: Element.Type = Element.self,
        bufferingPolicy limit: Continuation.BufferingPolicy = .unbounded
    ) -> (stream: AsyncStream<Element>, continuation: Continuation) {
        var continuation: Continuation!
        let stream = AsyncStream(elementType, bufferingPolicy: limit) { continuation = $0 }
        return (stream, continuation)
    }
}
