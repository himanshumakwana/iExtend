// LiveView.swift
// "iPad · Live" — hosts the CAMetalLayer render surface via
// UIViewControllerRepresentable with a SwiftUI overlay for the toolbar,
// connecting / disconnected overlays, and the PencilHud.
//
// Plan 6: wires the view hierarchy; MetalRenderer does a solid-black blit
//         until DecodeSession starts producing frames.
// Plan 8: reprojection and cursor mask are activated by MetalRenderer.

#if canImport(UIKit)
import SwiftUI
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
            // Metal render surface (fills the entire screen)
            MetalLayerHost(frameQueue: sessionViewModel.frameQueue)
                .ignoresSafeArea()

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
        renderer.layer.autoresizingMask = [.layerWidthSizable, .layerHeightSizable]
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

    public init(session: IExtendSession) {
        self.session = session
        Task { await observeSession() }
    }

    private func observeSession() async {
        for await state in await session.stateStream() {
            await MainActor.run {
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

// MARK: - ToolbarPosition / ToolbarDensity

public enum ToolbarPosition: String, CaseIterable { case top, bottom, left }
public enum ToolbarDensity:  String, CaseIterable { case compact, regular, comfy }

// MARK: - Preview

#Preview {
    LiveView(sessionViewModel: SessionViewModel(session: IExtendSession()))
        .preferredColorScheme(.dark)
        .applyTheme(Theme(dark: true))
}
#endif
