// PeerConnection.swift
// Wraps Google's RTCPeerConnection to drive the WebRTC lifecycle.
// Plan 6 supplies the adapter layer; Plan 5 smoke_loopback.rs is the first
// real remote peer. Plan 7 wires cert-pinning; Plan 8 adds the video track.
//
// Threading model (Spec §8.4):
//   - All RTCPeerConnection callbacks arrive on an arbitrary libWebRTC thread.
//   - We re-dispatch every callback onto a Swift actor before touching any
//     shared state, so the actor guarantees exclusion.
//
// Dependencies: WebRTC.xcframework (Stasel build, M127+)

import Foundation
#if canImport(WebRTC)
import WebRTC
#endif

// MARK: - Error

public enum PeerConnectionError: Error, Sendable {
    case alreadyConnected
    case offerFailed(String)
    case sdpSetFailed(String)
    case iceFailed
    case dtlsAlert(Int)
    case noVideoTrack
    case dataChannelSetupFailed
    case unexpectedState(String)
}

// MARK: - Delegate protocol (actor-isolated callbacks)

public protocol PeerConnectionDelegate: AnyObject, Sendable {
    func peerConnectionDidChangeState(_ state: RTCPeerConnectionState) async
    func peerConnectionDidReceiveFrame(_ pixelBuffer: CVPixelBuffer, pts: CMTime) async
    func peerConnectionDidOpenControlChannel() async
    func peerConnectionDidReceiveControlMessage(_ data: Data) async
    func peerConnectionFailed(_ error: PeerConnectionError) async
}

// MARK: - RTCPeerConnectionState shim (stub when WebRTC not importable)

#if !canImport(WebRTC)
// Allow compiling on Linux CI (no WebRTC available).
@objc public enum RTCPeerConnectionState: Int {
    case new, connecting, connected, disconnected, failed, closed
}
public typealias RTCConfiguration = AnyObject
public typealias RTCDataChannel = AnyObject
public typealias RTCSessionDescription = AnyObject
public typealias RTCIceCandidate = AnyObject
#endif

// MARK: - PeerConnection actor

/// Wraps RTCPeerConnection. Lifetime is one connection attempt; discard and
/// create a new instance to reconnect.
public actor PeerConnection {

    // MARK: Public state
    public private(set) var connectionState: RTCPeerConnectionState = .new

    // MARK: Private
#if canImport(WebRTC)
    private var pc: RTCPeerConnection?
    private var controlChannel: RTCDataChannel?
    private var factory: RTCPeerConnectionFactory?
#endif

    private weak var delegate: (any PeerConnectionDelegate)?
    private let iceServers: [String]

    // MARK: Init

    public init(iceServers: [String] = [], delegate: any PeerConnectionDelegate) {
        self.iceServers = iceServers
        self.delegate = delegate
    }

    // MARK: - Lifecycle

    /// Create the RTCPeerConnection and the control DataChannel, then generate
    /// an SDP offer. The returned offer must be signaled to the host.
    public func createOffer() async throws -> String {
#if canImport(WebRTC)
        RTCInitializeSSL()
        let videoEncoderFactory = RTCDefaultVideoDecoderFactory()
        let videoDecoderFactory = RTCDefaultVideoEncoderFactory()
        let f = RTCPeerConnectionFactory(
            encoderFactory: videoDecoderFactory,
            decoderFactory: videoEncoderFactory
        )
        self.factory = f

        var iceServerObjects: [RTCIceServer] = []
        for url in iceServers {
            iceServerObjects.append(RTCIceServer(urlStrings: [url]))
        }

        let config = RTCConfiguration()
        config.iceServers = iceServerObjects
        config.sdpSemantics = .unifiedPlan
        config.continualGatheringPolicy = .gatherContinually
        config.candidateNetworkPolicy = .all

        let pcConstraints = RTCMediaConstraints(
            mandatoryConstraints: nil,
            optionalConstraints: ["DtlsSrtpKeyAgreement": "true"]
        )

        let bridge = PeerConnectionBridge(parent: self)
        guard let newPc = f.peerConnection(
            with: config,
            constraints: pcConstraints,
            delegate: bridge
        ) else {
            throw PeerConnectionError.offerFailed("RTCPeerConnection creation returned nil")
        }
        self.pc = newPc

        // Data channel for control messages (heartbeat, resize, etc.)
        let dcConfig = RTCDataChannelConfiguration()
        dcConfig.isOrdered = true
        dcConfig.isNegotiated = false
        guard let dc = newPc.dataChannel(forLabel: "iextend-control", configuration: dcConfig) else {
            throw PeerConnectionError.dataChannelSetupFailed
        }
        self.controlChannel = dc

        // Add video transceiver (recv-only: iPad only decodes).
        let videoTransceiverInit = RTCRtpTransceiverInit()
        videoTransceiverInit.direction = .recvOnly
        newPc.addTransceiver(of: .video, init: videoTransceiverInit)

        // Generate offer.
        let constraints = RTCMediaConstraints(
            mandatoryConstraints: ["OfferToReceiveVideo": "true"],
            optionalConstraints: nil
        )

        return try await withCheckedThrowingContinuation { continuation in
            newPc.offer(for: constraints) { sdp, error in
                if let error {
                    continuation.resume(throwing: PeerConnectionError.offerFailed(error.localizedDescription))
                    return
                }
                guard let sdp else {
                    continuation.resume(throwing: PeerConnectionError.offerFailed("nil SDP"))
                    return
                }
                newPc.setLocalDescription(sdp) { err in
                    if let err {
                        continuation.resume(throwing: PeerConnectionError.sdpSetFailed(err.localizedDescription))
                    } else {
                        continuation.resume(returning: sdp.sdp)
                    }
                }
            }
        }
#else
        throw PeerConnectionError.offerFailed("WebRTC not available on this platform")
#endif
    }

    /// Apply the remote SDP answer received from the host via signaling.
    public func applyAnswer(_ sdpString: String) async throws {
#if canImport(WebRTC)
        guard let pc else { throw PeerConnectionError.unexpectedState("applyAnswer before createOffer") }
        let sdp = RTCSessionDescription(type: .answer, sdp: sdpString)
        try await withCheckedThrowingContinuation { (cont: CheckedContinuation<Void, Error>) in
            pc.setRemoteDescription(sdp) { error in
                if let error {
                    cont.resume(throwing: PeerConnectionError.sdpSetFailed(error.localizedDescription))
                } else {
                    cont.resume()
                }
            }
        }
#endif
    }

    /// Add an ICE candidate received from the host.
    public func addICECandidate(sdp: String, sdpMid: String?, sdpMLineIndex: Int32) async {
#if canImport(WebRTC)
        guard let pc else { return }
        let candidate = RTCIceCandidate(sdp: sdp, sdpMLineIndex: sdpMLineIndex, sdpMid: sdpMid)
        pc.add(candidate) { _ in }
#endif
    }

    /// Send a control message to the host over the DataChannel.
    public func sendControl(_ data: Data) {
#if canImport(WebRTC)
        guard let dc = controlChannel else { return }
        let buffer = RTCDataBuffer(data: data, isBinary: true)
        dc.sendData(buffer)
#endif
    }

    /// Cleanly close the peer connection.
    public func close() {
#if canImport(WebRTC)
        controlChannel?.close()
        pc?.close()
        pc = nil
        controlChannel = nil
        factory = nil
#endif
        connectionState = .closed
    }

    // MARK: Internal callbacks (called from PeerConnectionBridge)

    func _onStateChange(_ state: RTCPeerConnectionState) async {
        connectionState = state
        await delegate?.peerConnectionDidChangeState(state)
        if state == .failed {
            await delegate?.peerConnectionFailed(.iceFailed)
        }
    }

    func _onControlMessage(_ data: Data) async {
        await delegate?.peerConnectionDidReceiveControlMessage(data)
    }

    func _onControlChannelOpened() async {
        await delegate?.peerConnectionDidOpenControlChannel()
    }
}

// MARK: - Objective-C bridge

// A plain NSObject that implements RTCPeerConnectionDelegate and re-dispatches
// callbacks onto the PeerConnection actor. Necessary because RTCPeerConnectionDelegate
// is an @objc protocol; Swift actors cannot directly conform to it.
#if canImport(WebRTC)
private final class PeerConnectionBridge: NSObject, RTCPeerConnectionDelegate, RTCDataChannelDelegate, @unchecked Sendable {

    private let parent: PeerConnection

    init(parent: PeerConnection) {
        self.parent = parent
    }

    // MARK: RTCPeerConnectionDelegate

    func peerConnection(_ peerConnection: RTCPeerConnection,
                        didChange stateChanged: RTCSignalingState) {}

    func peerConnection(_ peerConnection: RTCPeerConnection,
                        didAdd stream: RTCMediaStream) {}

    func peerConnection(_ peerConnection: RTCPeerConnection,
                        didRemove stream: RTCMediaStream) {}

    func peerConnectionShouldNegotiate(_ peerConnection: RTCPeerConnection) {}

    func peerConnection(_ peerConnection: RTCPeerConnection,
                        didChange newState: RTCIceConnectionState) {}

    func peerConnection(_ peerConnection: RTCPeerConnection,
                        didChange newState: RTCIceGatheringState) {}

    func peerConnection(_ peerConnection: RTCPeerConnection,
                        didGenerate candidate: RTCIceCandidate) {
        // In production, signal this candidate to the host via Signaling.swift.
        // For Plan 6 (LAN-only), continual gathering collects all candidates and
        // the offer SDP already has host candidates embedded.
    }

    func peerConnection(_ peerConnection: RTCPeerConnection,
                        didRemove candidates: [RTCIceCandidate]) {}

    func peerConnection(_ peerConnection: RTCPeerConnection,
                        didOpen dataChannel: RTCDataChannel) {
        dataChannel.delegate = self
        Task { await parent._onControlChannelOpened() }
    }

    func peerConnection(_ peerConnection: RTCPeerConnection,
                        didChange state: RTCPeerConnectionState) {
        Task { await parent._onStateChange(state) }
    }

    // MARK: RTCDataChannelDelegate

    func dataChannelDidChangeState(_ dataChannel: RTCDataChannel) {}

    func dataChannel(_ dataChannel: RTCDataChannel,
                     didReceiveMessageWith buffer: RTCDataBuffer) {
        let data = buffer.data
        Task { await parent._onControlMessage(data) }
    }
}
#endif
