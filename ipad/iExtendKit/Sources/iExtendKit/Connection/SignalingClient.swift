// SignalingClient.swift
//
// Mirror of `host/crates/iextendd/src/signaling.rs`. Opens a TCP
// connection to the daemon's signaling listener (default port 7783) and
// exchanges WebRTC SDP/ICE messages as length-prefixed JSON frames.
//
// Wire format: 4-byte big-endian length + UTF-8 JSON, where JSON shape is
//   {"type":"offer","sdp":"..."}
//   {"type":"answer","sdp":"..."}
//   {"type":"ice","candidate":"..."}
//   {"type":"bye"}
// Tagged unions keep parsing trivial on both sides.
//
// Usage:
//   let client = SignalingClient(host: "192.168.1.10", port: 7783)
//   try client.start()
//   try await client.send(.offer(sdp: localOffer))
//   for await msg in client.incoming { ... }
//   client.stop()

#if canImport(Foundation)
import Foundation
import Network

public enum SignalMsg: Codable, Equatable {
    case offer(sdp: String)
    case answer(sdp: String)
    case ice(candidate: String)
    case bye

    private enum CodingKeys: String, CodingKey { case type, sdp, candidate }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let type = try c.decode(String.self, forKey: .type)
        switch type {
        case "offer":  self = .offer(sdp: try c.decode(String.self, forKey: .sdp))
        case "answer": self = .answer(sdp: try c.decode(String.self, forKey: .sdp))
        case "ice":    self = .ice(candidate: try c.decode(String.self, forKey: .candidate))
        case "bye":    self = .bye
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .type, in: c,
                debugDescription: "unknown signaling type: \(type)"
            )
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .offer(let sdp):
            try c.encode("offer", forKey: .type)
            try c.encode(sdp, forKey: .sdp)
        case .answer(let sdp):
            try c.encode("answer", forKey: .type)
            try c.encode(sdp, forKey: .sdp)
        case .ice(let cand):
            try c.encode("ice", forKey: .type)
            try c.encode(cand, forKey: .candidate)
        case .bye:
            try c.encode("bye", forKey: .type)
        }
    }
}

public enum SignalingError: Error, LocalizedError {
    case notConnected
    case sendFailed(Error)
    case receiveFailed(Error)
    case decodeFailed(String)
    case frameTooLarge(Int)

    public var errorDescription: String? {
        switch self {
        case .notConnected:           return "signaling: not connected"
        case .sendFailed(let e):      return "signaling: send failed: \(e.localizedDescription)"
        case .receiveFailed(let e):   return "signaling: receive failed: \(e.localizedDescription)"
        case .decodeFailed(let s):    return "signaling: decode failed: \(s)"
        case .frameTooLarge(let n):   return "signaling: frame too large (\(n) bytes)"
        }
    }
}

public final class SignalingClient {
    public typealias OnMessage = (SignalMsg) -> Void
    public typealias OnError = (SignalingError) -> Void

    private let host: String
    private let port: UInt16
    private let queue = DispatchQueue(label: "iextend.signaling")
    private var connection: NWConnection?

    public var onMessage: OnMessage?
    public var onError: OnError?

    public init(host: String, port: UInt16 = 7783) {
        self.host = host
        self.port = port
    }

    /// Open the TCP connection. Returns immediately; readiness is signalled
    /// via the first received message or via `onError` if the dial fails.
    public func start() {
        let endpoint = NWEndpoint.hostPort(
            host: NWEndpoint.Host(host),
            port: NWEndpoint.Port(rawValue: port)!
        )
        let conn = NWConnection(to: endpoint, using: .tcp)
        conn.stateUpdateHandler = { [weak self] state in
            guard let self else { return }
            switch state {
            case .ready:
                self.startReading()
            case .failed(let err):
                self.onError?(.receiveFailed(err))
            default:
                break
            }
        }
        conn.start(queue: queue)
        self.connection = conn
    }

    public func stop() {
        // Best-effort polite close.
        Task { try? await self.send(.bye) }
        connection?.cancel()
        connection = nil
    }

    /// Send one signaling message. Awaits the OS write completion.
    public func send(_ msg: SignalMsg) async throws {
        guard let conn = connection else { throw SignalingError.notConnected }
        let body: Data
        do {
            body = try JSONEncoder().encode(msg)
        } catch {
            throw SignalingError.sendFailed(error)
        }
        if body.count > 64 * 1024 {
            throw SignalingError.frameTooLarge(body.count)
        }
        var len = UInt32(body.count).bigEndian
        var framed = Data(bytes: &len, count: 4)
        framed.append(body)

        try await withCheckedThrowingContinuation { (cont: CheckedContinuation<Void, Error>) in
            conn.send(content: framed, completion: .contentProcessed { err in
                if let err {
                    cont.resume(throwing: SignalingError.sendFailed(err))
                } else {
                    cont.resume()
                }
            })
        }
    }

    /// Pull one frame off the wire and dispatch onMessage. Recurses to keep
    /// reading until cancelled.
    private func startReading() {
        guard let conn = connection else { return }
        // Read the 4-byte length prefix.
        conn.receive(minimumIncompleteLength: 4, maximumLength: 4) {
            [weak self] data, _, _, err in
            guard let self else { return }
            if let err {
                self.onError?(.receiveFailed(err))
                return
            }
            guard let data, data.count == 4 else {
                self.onError?(.decodeFailed("truncated length prefix"))
                return
            }
            let len = data.withUnsafeBytes { ptr -> UInt32 in
                let raw = ptr.load(as: UInt32.self)
                return UInt32(bigEndian: raw)
            }
            if len > 64 * 1024 {
                self.onError?(.frameTooLarge(Int(len)))
                return
            }

            conn.receive(minimumIncompleteLength: Int(len), maximumLength: Int(len)) {
                bodyData, _, _, bodyErr in
                if let bodyErr {
                    self.onError?(.receiveFailed(bodyErr))
                    return
                }
                guard let bodyData else {
                    self.onError?(.decodeFailed("truncated body"))
                    return
                }
                do {
                    let msg = try JSONDecoder().decode(SignalMsg.self, from: bodyData)
                    self.onMessage?(msg)
                } catch {
                    self.onError?(.decodeFailed("\(error)"))
                }
                // Continue reading the next frame.
                self.startReading()
            }
        }
    }
}
#endif
