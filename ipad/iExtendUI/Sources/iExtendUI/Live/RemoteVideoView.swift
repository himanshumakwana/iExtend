// RemoteVideoView.swift
//
// SwiftUI bridge that hosts an `RTCMTLVideoView` (Metal-backed video view
// from WebRTC.framework) and attaches it as a renderer on the
// `PeerConnection`'s remote track.
//
// First-pixel experience (Plan A milestone M5):
//   ContentView.swift inserts this view into the live workspace once
//   `WebRTCSession.state == .connected`. The view automatically picks up
//   the daemon's video track via `peer.attachVideoRenderer(self)`, which
//   queues the renderer if the track hasn't arrived yet and replays it
//   when it does.

#if canImport(UIKit)
import SwiftUI
import iExtendKit
#if canImport(WebRTC)
import WebRTC
#endif

public struct RemoteVideoView: UIViewRepresentable {
    let session: WebRTCSession

    public init(session: WebRTCSession) {
        self.session = session
    }

#if canImport(WebRTC)
    public func makeUIView(context: Context) -> RTCMTLVideoView {
        let view = RTCMTLVideoView(frame: .zero)
        view.videoContentMode = .scaleAspectFit
        // RTCMTLVideoView conforms to RTCVideoRenderer.
        Task { @MainActor in
            await session.peer.attachVideoRenderer(view)
        }
        // Coordinator holds the view so we can detach on tear-down.
        context.coordinator.view = view
        return view
    }

    public func updateUIView(_ uiView: RTCMTLVideoView, context: Context) {
        // Nothing to update — the renderer is attached once at make time
        // and frames flow via the WebRTC track.
    }

    public static func dismantleUIView(_ uiView: RTCMTLVideoView, coordinator: Coordinator) {
        if let session = coordinator.session {
            Task { @MainActor in
                await session.peer.detachVideoRenderer(uiView)
            }
        }
    }

    public func makeCoordinator() -> Coordinator {
        Coordinator(session: session)
    }

    public final class Coordinator {
        weak var session: WebRTCSession?
        weak var view: RTCMTLVideoView?
        init(session: WebRTCSession) { self.session = session }
    }
#else
    // No-WebRTC fallback so iExtendUI compiles on macOS host (swift test).
    public func makeUIView(context: Context) -> UIView { UIView() }
    public func updateUIView(_ uiView: UIView, context: Context) {}
#endif
}
#endif
