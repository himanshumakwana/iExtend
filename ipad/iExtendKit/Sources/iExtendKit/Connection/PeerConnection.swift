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
import CoreVideo
import CoreMedia
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

    /// Called whenever WebRTC's local ICE agent gathers a new candidate.
    /// The orchestrator forwards it over the signaling channel so the
    /// remote peer can call `addICECandidate` with the same string.
    func peerConnectionDidGenerateLocalCandidate(_ candidate: String, sdpMid: String?, sdpMLineIndex: Int32) async

    /// Called when the daemon's outbound video track arrives. The
    /// orchestrator attaches an `RTCMTLVideoView` (or any renderer) via
    /// `PeerConnection.attachVideoRenderer(_:)` to start drawing frames.
    func peerConnectionDidReceiveRemoteVideoTrack() async
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
public typealias RTCVideoRenderer = AnyObject
public typealias RTCVideoTrack = AnyObject
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
    /// Holds the most recently delivered remote video track so an
    /// `RTCMTLVideoView` (or any renderer) can be attached after the fact.
    /// SwiftUI views can't always be ready at the instant the track arrives,
    /// so we stash + replay rather than racing.
    private var remoteVideoTrack: RTCVideoTrack?
    private var pendingRenderers: [RTCVideoRenderer] = []
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
        remoteVideoTrack = nil
        pendingRenderers.removeAll()
#endif
        connectionState = .closed
    }

    /// Attach a renderer (e.g. `RTCMTLVideoView`) to the remote video track.
    /// If the track hasn't arrived yet, the renderer is queued and attached
    /// once `peerConnectionDidReceiveRemoteVideoTrack` fires.
    public func attachVideoRenderer(_ renderer: RTCVideoRenderer) {
#if canImport(WebRTC)
        if let track = remoteVideoTrack {
            track.add(renderer)
        } else {
            pendingRenderers.append(renderer)
        }
#endif
    }

    /// Detach a previously-attached renderer. Safe to call even if the
    /// renderer was never attached or the track is gone.
    public func detachVideoRenderer(_ renderer: RTCVideoRenderer) {
#if canImport(WebRTC)
        remoteVideoTrack?.remove(renderer)
        pendingRenderers.removeAll { $0 === renderer }
#endif
    }

    // MARK: Internal callbacks (called from PeerConnectionBridge)

    func _onStateChange(_ state: RTCPeerConnectionState) async {
        connectionState = state
        await delegate?.peerConnectionDidChangeState(state)
        if state == .failed {
            await delegate?.peerConnectionFailed(.iceFailed)
        }
    }

    func _onLocalCandidate(_ candidate: String, sdpMid: String?, sdpMLineIndex: Int32) async {
        await delegate?.peerConnectionDidGenerateLocalCandidate(
            candidate, sdpMid: sdpMid, sdpMLineIndex: sdpMLineIndex
        )
    }

#if canImport(WebRTC)
    func _onRemoteVideoTrack(_ track: RTCVideoTrack) async {
        remoteVideoTrack = track
        // Drain any renderers that registered before the track arrived.
        for renderer in pendingRenderers {
            track.add(renderer)
        }
        pendingRenderers.removeAll()
        await delegate?.peerConnectionDidReceiveRemoteVideoTrack()
    }
#endif

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
        // Forward to the orchestrator so it can push the candidate to the
        // host over the signaling channel. The candidate.sdp string is the
        // "candidate:..." token line; sdpMid + sdpMLineIndex are needed by
        // the remote peer to attach it to the right m= section.
        let sdp = candidate.sdp
        let mid = candidate.sdpMid
        let mLine = candidate.sdpMLineIndex
        Task { await parent._onLocalCandidate(sdp, sdpMid: mid, sdpMLineIndex: mLine) }
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

    /// Modern Unified-Plan equivalent of didAdd/stream — fires when an
    /// RTCRtpReceiver starts receiving on a transceiver. We use this to
    /// pick up the daemon's video track (the daemon adds it to the
    /// peer connection before sending the answer SDP).
    func peerConnection(_ peerConnection: RTCPeerConnection,
                        didStartReceivingOn transceiver: RTCRtpTransceiver) {
        if let track = transceiver.receiver.track as? RTCVideoTrack {
            Task { await parent._onRemoteVideoTrack(track) }
        }
    }

    /// Plan-B fallback. Some webrtc-rs versions still trigger didAdd/stream
    /// instead of didStartReceivingOn. We listen on both to be safe.
    func peerConnection(_ peerConnection: RTCPeerConnection,
                        didAdd rtpReceiver: RTCRtpReceiver,
                        streams mediaStreams: [RTCMediaStream]) {
        if let track = rtpReceiver.track as? RTCVideoTrack {
            Task { await parent._onRemoteVideoTrack(track) }
        }
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
