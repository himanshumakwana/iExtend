// LiveView.swift
// "iPad · Live" — hosts the CAMetalLayer render surface via
// UIViewControllerRepresentable with a SwiftUI overlay for the toolbar,
// connecting / disconnected overlays, and the PencilHud.
//
// Plan 6: wires the view hierarchy; MetalRenderer does a solid-black blit
//         until DecodeSession starts producing frames.
// Plan 8: reprojection and cursor mask are activated by MetalRenderer.

import SwiftUI
import Combine

// Toolbar configuration enums — referenced by FloatingToolbar (which doesn't
// depend on UIKit) so they live outside the iOS-only gate.
public enum ToolbarPosition: String, CaseIterable { case top, bottom, left }
public enum ToolbarDensity:  String, CaseIterable { case compact, regular, comfy }

#if canImport(UIKit)
import iExtendKit

public struct LiveView: View {
    @Environment(\.theme) private var t

    @ObservedObject var sessionViewModel: SessionViewModel
    @State private var toolbarPos: ToolbarPosition = .bottom
    @State private var toolbarDensity: ToolbarDensity = .regular

    public init(sessionViewModel: SessionViewModel) {
        self.sessionViewModel = sessionViewModel
    }

    public var body: some View {
        ZStack {
            // Render path priority:
            //   1. WebRTC session is connected → use RTCMTLVideoView via
            //      RemoteVideoView. This is the Plan A first-pixel path.
            //   2. Fallback to MetalLayerHost (Plan 6 scaffold) so existing
            //      tests + frameQueue-based renders still work.
            if let session = sessionViewModel.webrtcSession,
               session.state == .connected {
                RemoteVideoView(session: session)
                    .ignoresSafeArea()
            } else {
                MetalLayerHost(frameQueue: sessionViewModel.frameQueue)
                    .ignoresSafeArea()
            }

            // Overlays
            overlayLayer
        }
        .ignoresSafeArea()
        .statusBarHidden(true)
        .persistentSystemOverlays(.hidden)
    }

    // MARK: Overlay layer

    @ViewBuilder
    private var overlayLayer: some View {
        // Connecting
        if case .connecting = sessionViewModel.sessionState {
            ConnectingOverlay(hostName: sessionViewModel.connectedHostName)
        }

        // Disconnected / failed
        if case .disconnected = sessionViewModel.sessionState {
            DisconnectedOverlay(
                hostName: sessionViewModel.connectedHostName,
                hostIP: sessionViewModel.connectedHostIP,
                onRetry: { Task { await sessionViewModel.reconnect() } },
                onCancel: { Task { await sessionViewModel.disconnect() } }
            )
        }

        // Live — show toolbar + pencil HUD
        if case .live = sessionViewModel.sessionState {
            FloatingToolbar(
                position: toolbarPos,
                density: toolbarDensity,
                latencyMs: sessionViewModel.latencyMs,
                onModeToggle: { },
                onResolutionToggle: { },
                onPencilToggle: { },
                onSettings: { sessionViewModel.presentSettings() },
                onDisconnect: { Task { await sessionViewModel.disconnect() } }
            )
        }
    }
}

// MARK: - MetalLayerHost (UIViewControllerRepresentable)

public struct MetalLayerHost: UIViewControllerRepresentable {
    let frameQueue: FrameQueue?

    public init(frameQueue: FrameQueue?) {
        self.frameQueue = frameQueue
    }

    public func makeUIViewController(context: Context) -> MetalLayerHostController {
        MetalLayerHostController(frameQueue: frameQueue)
    }

    public func updateUIViewController(_ uiViewController: MetalLayerHostController, context: Context) {
        // Frame queue is owned by the controller; no dynamic updates needed.
    }
}

// MARK: - MetalLayerHostController

public final class MetalLayerHostController: UIViewController {
    private var renderer: MetalRenderer?
    private let frameQueue: FrameQueue?

    public init(frameQueue: FrameQueue?) {
        self.frameQueue = frameQueue
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) { fatalError() }

    public override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .black

        guard let fq = frameQueue,
              let renderer = try? MetalRenderer(frameQueue: fq) else { return }
        self.renderer = renderer

        renderer.layer.frame = view.bounds
        // CALayer.autoresizingMask is macOS-only. On iOS we resize the
        // sublayer manually in viewDidLayoutSubviews(); keep that path
        // for cross-platform consistency.
        #if os(macOS)
        renderer.layer.autoresizingMask = [.layerWidthSizable, .layerHeightSizable]
        #endif
        view.layer.addSublayer(renderer.layer)

        renderer.start()
    }

    public override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        renderer?.stop()
    }

    public override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        renderer?.invalidate()
    }

    public override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        renderer?.layer.frame = view.bounds
    }
}

// MARK: - SessionViewModel (observable bridge for LiveView)

@MainActor
public final class SessionViewModel: ObservableObject {
    @Published public var sessionState: SessionState = .idle
    @Published public var latencyMs: Double = 0
    @Published public var connectedHostName: String = ""
    @Published public var connectedHostIP: String = ""
    @Published public var showSettings = false

    public var frameQueue: FrameQueue? { _frameQueue }
    private var _frameQueue: FrameQueue?

    public let session: IExtendSession

    /// Loopback listener for USB pair flow. Bound at `init`; views observe
    /// `usbListener.pendingConnection` to present the USB PIN sheet. Fails
    /// silently in Simulator / on a non-iOS host where 7780 might be busy —
    /// USB pair just won't work in those environments.
    public let usbListener = USBPairingListener()

    /// Active WebRTC streaming session. Nil until `startStreaming(host:)` is
    /// called after a successful pair; created fresh per "connect" press.
    /// Views observe its published state and attach the remote-video
    /// renderer when it transitions to `.connected`.
    @Published public var webrtcSession: WebRTCSession?

    /// Combine sink subscribed to the current WebRTCSession's `$state`.
    /// Replaced (with Cancellable auto-disposal) on every `startStreaming`.
    private var stateCancellable: AnyCancellable?

    public init(session: IExtendSession) {
        self.session = session
        do {
            try usbListener.start()
        } catch {
            print("USBPairingListener.start failed: \(error)")
        }
        Task { await observeSession() }
    }

    /// Start a screen-share session to `host`. Tears down any existing
    /// WebRTC session first. The caller (ContentView post-pair) is
    /// responsible for keeping the host IP — we don't persist it.
    ///
    /// As a side effect we drive the underlying `IExtendSession` through
    /// .connecting → .live so the LiveView UI swaps in correctly. State
    /// is mirrored from the WebRTC session's published `state`.
    public func startStreaming(host: String) {
        Task { @MainActor in
            if let existing = webrtcSession {
                await existing.stop()
            }
            let s = WebRTCSession(host: host)
            self.webrtcSession = s

            // Move the IExtendSession to .connecting immediately so any
            // existing UI driven by `sessionState` (ConnectingOverlay)
            // shows up while WebRTC negotiates.
            let hostInfo = HostInfo(
                displayName: "iExtend host",
                ipAddress: host,
                pubkeyThumbprint: "",
                osHint: ""
            )
            await session.connect(to: hostInfo)

            // Subscribe to `s.$state` BEFORE awaiting start() so we don't
            // miss the very first transition (which can land synchronously
            // on signaling.start()).
            //
            // Combine's @Published runs on whatever queue mutates the
            // property; WebRTCSession is @MainActor so the sink fires on
            // main — safe to call back into self.session from here.
            //
            // Replacing self.stateCancellable cancels any prior
            // subscription via AnyCancellable's drop semantics.
            self.stateCancellable = s.$state
                .removeDuplicates()
                .sink { [weak self] newState in
                    print("[SessionViewModel] webrtc state → \(newState)")
                    guard let self else { return }
                    Task { @MainActor in
                        await self.handleWebRTCState(newState, ifSessionIs: s)
                    }
                }

            await s.start()
        }
    }

    /// Translate WebRTCSession state into IExtendSession lifecycle calls.
    /// Guarded by `ifSessionIs` so a late-firing sink for a torn-down
    /// session can't move the lifecycle for a *new* session.
    private func handleWebRTCState(
        _ newState: WebRTCSessionState,
        ifSessionIs expected: WebRTCSession
    ) async {
        guard self.webrtcSession === expected else { return }
        switch newState {
        case .connected:
            print("[SessionViewModel] dispatching wentLive")
            await self.session.wentLive(stats: .zero)
        case .failed(let reason):
            await self.session.fail(.unknown(message: reason))
        case .closed:
            await self.session.disconnect(reason: .userRequested)
        default:
            break
        }
    }

    /// Tear down the streaming session.
    public func stopStreaming() {
        Task { @MainActor in
            if let s = webrtcSession {
                await s.stop()
                self.webrtcSession = nil
            }
        }
    }

    private func observeSession() async {
        for await state in await session.stateStream() {
            await MainActor.run {
                print("[SessionViewModel] sessionState → \(state)")
                self.sessionState = state
                switch state {
                case .live(let stats):
                    self.latencyMs = stats.rttMs
                case .connecting(let host):
                    self.connectedHostName = host.displayName
                    self.connectedHostIP   = host.ipAddress
                    if _frameQueue == nil { _frameQueue = FrameQueue(capacity: 8) }
                default:
                    break
                }
            }
        }
    }

    public func reconnect() async {
        // TODO: Plan 7 — re-run pairing / reconnect flow.
    }

    public func disconnect() async {
        await session.disconnect(reason: .userRequested)
    }

    public func presentSettings() {
        showSettings = true
    }
}

// MARK: - Preview

#Preview {
    LiveView(sessionViewModel: SessionViewModel(session: IExtendSession()))
        .preferredColorScheme(.dark)
        .applyTheme(Theme(dark: true))
}
#endif
