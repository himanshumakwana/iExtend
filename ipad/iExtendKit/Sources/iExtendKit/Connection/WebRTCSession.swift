// WebRTCSession.swift
//
// Orchestrates the iPad's side of the screen-share session:
// 1. Opens a `SignalingClient` to the host daemon (port 7783).
// 2. Constructs a `PeerConnection` and creates a local SDP offer.
// 3. Sends the offer through signaling.
// 4. Awaits the daemon's Answer; applies it to the peer.
// 5. Forwards ICE candidates in both directions.
// 6. Surfaces the daemon's remote video track (via `attachRenderer`)
//    so the SwiftUI `RemoteVideoView` can render it.
//
// Lifetime: one WebRTCSession per "Connect" press. Closing it tears down
// the peer + signaling channel cleanly.

#if canImport(UIKit)
import Foundation
import Combine
#if canImport(WebRTC)
import WebRTC
#endif

/// Public state mirrored to the SwiftUI layer.
public enum WebRTCSessionState: Equatable, Sendable {
    case idle
    case signaling
    case offering
    case waitingForAnswer
    case negotiating
    case connected
    case failed(reason: String)
    case closed
}

@MainActor
public final class WebRTCSession: ObservableObject {

    @Published public private(set) var state: WebRTCSessionState = .idle

    public let peer: PeerConnection
    private let signaling: SignalingClient

    public init(host: String, signalingPort: UInt16 = 7783, iceServers: [String] = ["stun:stun.l.google.com:19302"]) {
        self.signaling = SignalingClient(host: host, port: signalingPort)
        self.peer = PeerConnection(iceServers: iceServers, delegate: SessionDelegateBridge.shared)

        // Wire signaling's callbacks. We hold a strong ref so the bridge
        // outlives the closures.
        signaling.onMessage = { [weak self] msg in
            guard let self else { return }
            Task { @MainActor in
                await self.handleSignal(msg)
            }
        }
        signaling.onError = { [weak self] err in
            guard let self else { return }
            Task { @MainActor in
                self.state = .failed(reason: "signaling: \(err.localizedDescription)")
            }
        }

        // The bridge fans delegate calls back into this WebRTCSession instance.
        SessionDelegateBridge.shared.attach(self)
    }

    /// Start the session: open signaling, create the offer, send it.
    public func start() async {
        state = .signaling
        signaling.start()

        do {
            let offerSDP = try await peer.createOffer()
            state = .offering
            try await signaling.send(.offer(sdp: offerSDP))
            state = .waitingForAnswer
        } catch {
            state = .failed(reason: "createOffer: \(error.localizedDescription)")
        }
    }

    /// Close everything cleanly.
    public func stop() async {
        signaling.stop()
        await peer.close()
        state = .closed
    }

    // MARK: Inbound signaling messages

    private func handleSignal(_ msg: SignalMsg) async {
        switch msg {
        case .answer(let sdp):
            do {
                state = .negotiating
                try await peer.applyAnswer(sdp)
            } catch {
                state = .failed(reason: "applyAnswer: \(error.localizedDescription)")
            }
        case .ice(let candidate):
            // Daemon's outbound ICE candidates carry only the candidate
            // string. sdpMid/sdpMLineIndex aren't transported in our minimal
            // wire format; for a single-mline video-only offer we use
            // ("0", 0) which matches what webrtc-rs and WebRTC.framework
            // both emit by default.
            await peer.addICECandidate(sdp: candidate, sdpMid: "0", sdpMLineIndex: 0)
        case .offer:
            // Daemon shouldn't be sending us offers in the iPad-initiated
            // flow. If we see one, ignore — the iPad is the offerer here.
            state = .failed(reason: "unexpected Offer from daemon")
        case .bye:
            await stop()
        }
    }

    // MARK: Forwarding from the PeerConnection delegate

    fileprivate func onPeerStateChange(_ s: RTCPeerConnectionState) async {
#if canImport(WebRTC)
        switch s {
        case .connected: state = .connected
        case .failed:    state = .failed(reason: "peer connection failed")
        case .closed:    state = .closed
        default:         break
        }
#endif
    }

    fileprivate func onLocalCandidate(_ candidate: String) async {
        do {
            try await signaling.send(.ice(candidate: candidate))
        } catch {
            // ICE candidate send failure is non-fatal — webrtc keeps
            // gathering and other candidates may still complete the
            // handshake. Just log.
            print("WebRTCSession: failed to send local ICE candidate: \(error)")
        }
    }
}

/// `PeerConnectionDelegate` is `Sendable`-bounded and `AnyObject`, but a
/// `@MainActor` class can't directly conform without splitting types. The
/// bridge below is the workaround: a singleton plain class that takes
/// callbacks and re-dispatches them onto the right `WebRTCSession`.
private final class SessionDelegateBridge: PeerConnectionDelegate, @unchecked Sendable {
    static let shared = SessionDelegateBridge()
    private weak var session: WebRTCSession?

    func attach(_ session: WebRTCSession) {
        self.session = session
    }

    func peerConnectionDidChangeState(_ state: RTCPeerConnectionState) async {
        await session?.onPeerStateChange(state)
    }

    func peerConnectionDidReceiveFrame(_ pixelBuffer: CVPixelBuffer, pts: CMTime) async {
        // We render via RTCMTLVideoView attached as a renderer instead of
        // routing CVPixelBuffers through the delegate, so this is a no-op
        // for the Plan A first-pixel path.
    }

    func peerConnectionDidOpenControlChannel() async {}

    func peerConnectionDidReceiveControlMessage(_ data: Data) async {}

    func peerConnectionFailed(_ error: PeerConnectionError) async {
        await MainActor.run {
            session?.state = .failed(reason: "\(error)")
        }
    }

    func peerConnectionDidGenerateLocalCandidate(_ candidate: String, sdpMid: String?, sdpMLineIndex: Int32) async {
        await session?.onLocalCandidate(candidate)
    }

    func peerConnectionDidReceiveRemoteVideoTrack() async {
        // The view layer attaches its renderer via session.peer.attachVideoRenderer
        // when it appears on screen. Nothing to do here.
    }
}

// MARK: CoreVideo / CoreMedia imports for the protocol signature.
import CoreVideo
import CoreMedia
#endif
