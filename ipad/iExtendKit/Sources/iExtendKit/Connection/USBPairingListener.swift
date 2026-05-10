// USBPairingListener.swift
//
// Listens on 127.0.0.1:7780 for incoming connections from the laptop
// daemon over Apple's usbmuxd USB tunnel. The wire protocol is identical
// to the Wi-Fi pair flow (simple-pair-v0); only the transport direction
// inverts — laptop is the TCP client, iPad is the TCP server.
//
// UX:
//   1. App launches → listener.start() binds 127.0.0.1:7780 (loopback only).
//   2. User plugs iPad into laptop. Laptop daemon detects via usbmuxd and
//      opens a TCP-shaped tunnel to (udid, 7780) — that arrives here.
//   3. Listener publishes `pendingConnection`; UI observes and presents
//      the PIN-entry sheet.
//   4. User types the PIN displayed on the laptop tray and submits.
//      UI calls `listener.completePair(pin:displayName:)` which sends
//      PSimpleHello and reads PSimpleAck on the existing socket.
//   5. PairResult flows out via the published `lastResult`.

#if canImport(UIKit)
import Foundation
import Network
#if canImport(Crypto)
import Crypto
#endif
#if canImport(Security)
import Security
#endif

@MainActor
public final class USBPairingListener: ObservableObject {

    /// A connection received from the laptop's USB tunnel that hasn't been
    /// completed yet. UI observes this to present the PIN-entry sheet.
    /// Setting to nil after a pair / cancel cleans up the underlying socket.
    @Published public private(set) var pendingConnection: USBPendingConnection?

    /// Most recent USB pair result, success or failure. Mirrors the
    /// equivalent published value in IExtendSession's Wi-Fi pair flow so
    /// the UI can react uniformly.
    @Published public private(set) var lastResult: USBPairOutcome?

    /// Whether the loopback listener is currently bound.
    @Published public private(set) var isListening: Bool = false

    private let port: NWEndpoint.Port = 7780
    private var listener: NWListener?
    private let queue = DispatchQueue(label: "iextend.usb-pair-listener")

    public init() {}

    /// Bind the listener. Idempotent — calling twice while already bound is
    /// a no-op. Throws if the port is unavailable.
    public func start() throws {
        guard listener == nil else { return }
        let params = NWParameters.tcp
        params.allowLocalEndpointReuse = true
        let listener = try NWListener(using: params, on: port)
        listener.newConnectionHandler = { [weak self] connection in
            Task { @MainActor in
                self?.handleNewConnection(connection)
            }
        }
        listener.stateUpdateHandler = { [weak self] state in
            Task { @MainActor in
                switch state {
                case .ready:
                    self?.isListening = true
                case .failed, .cancelled:
                    self?.isListening = false
                default:
                    break
                }
            }
        }
        listener.start(queue: queue)
        self.listener = listener
    }

    /// Stop accepting new connections and tear down any pending one.
    public func stop() {
        listener?.cancel()
        listener = nil
        if let pending = pendingConnection {
            pending.connection.cancel()
            pendingConnection = nil
        }
        isListening = false
    }

    /// Drop the in-flight pending connection without pairing. Called by the
    /// UI when the user cancels the PIN-entry sheet.
    public func cancelPending() {
        pendingConnection?.connection.cancel()
        pendingConnection = nil
    }

    /// Complete the simple-pair-v0 handshake on the pending connection.
    ///
    /// Sends `PSimpleHello { pin, client_pubkey_b64, display_name }`, reads
    /// `PSimpleAck`, persists to Keychain on success.
    public func completePair(pin: String, displayName: String) async {
        guard let pending = pendingConnection else {
            lastResult = .failure("no USB connection pending")
            return
        }
        // Hand the connection over to the handshake; clear pending so the UI
        // sheet dismisses and so a fresh connection can arrive concurrently
        // without colliding with the in-flight handshake.
        pendingConnection = nil

        do {
            let result = try await PairingFlow.pairOverExistingConnection(
                connection: pending.connection,
                pin: pin,
                displayName: displayName
            )
            lastResult = .success(result)
        } catch {
            lastResult = .failure("\(error)")
        }
    }

    private func handleNewConnection(_ connection: NWConnection) {
        // If a previous pending connection is still around, cancel it — the
        // user's about to interact with the new one.
        if let stale = pendingConnection {
            stale.connection.cancel()
        }
        let pending = USBPendingConnection(connection: connection)
        pendingConnection = pending

        // Start the connection so it transitions to .ready. The actual
        // send/receive happens when completePair runs.
        connection.start(queue: queue)
    }
}

/// Wrapper around an in-flight NWConnection so the UI can show a stable
/// identity even while the underlying connection state changes.
public struct USBPendingConnection: Identifiable, Equatable {
    public let id = UUID()
    public let connection: NWConnection

    public static func == (lhs: USBPendingConnection, rhs: USBPendingConnection) -> Bool {
        lhs.id == rhs.id
    }
}

public enum USBPairOutcome: Equatable {
    case success(PairResult)
    case failure(String)

    public static func == (lhs: USBPairOutcome, rhs: USBPairOutcome) -> Bool {
        switch (lhs, rhs) {
        case (.success(let a), .success(let b)): return a.pairId == b.pairId
        case (.failure(let a), .failure(let b)): return a == b
        default: return false
        }
    }
}
#endif
