// ContentView.swift
// Root navigator that switches between onboarding, live, and settings views
// based on IExtendSession.state.
//
// State machine mapping (mirrors Spec §9):
//   .idle              → WelcomeView
//   .pairing(.browsingMDNS / .discoveredHost) → DiscoverView
//   .pairing(.enteringPin / .spake2Handshake) → PairView
//   .connecting        → LiveView (shows ConnectingOverlay)
//   .live / .degraded  → LiveView
//   .disconnected      → LiveView (shows DisconnectedOverlay)
//   .failed            → WelcomeView (error banner)

import SwiftUI
import iExtendKit
import iExtendUI

public struct ContentView: View {
    @EnvironmentObject private var sessionViewModel: SessionViewModel
    @Environment(\.colorScheme) private var cs

    // Onboarding state
    @State private var discoveredPeers: [DiscoveredPeer] = []
    @State private var selectedPeer: DiscoveredPeer?
    @State private var enteredPin: String = ""
    @State private var pairingToken: String = "iextend://pair/xxxxxxxx"
    @State private var isScanning: Bool = false
    @State private var networkName: String = "Wi\u{2011}Fi"

    // Settings sheet
    @State private var showSettings = false

    public init() {}

    public var body: some View {
        ZStack {
            switch sessionViewModel.sessionState {
            case .idle, .failed:
                WelcomeView(onGetStarted: handleGetStarted)
                    .transition(.opacity)

            case .pairing(let progress):
                pairingView(for: progress)
                    .transition(.opacity)

            case .connecting, .live, .degraded, .disconnected:
                LiveView(sessionViewModel: sessionViewModel)
                    .ignoresSafeArea()
                    .transition(.opacity)
            }
        }
        .animation(.easeInOut(duration: 0.25), value: viewPhase)
        .sheet(isPresented: $showSettings) {
            SettingsView(session: sessionViewModel)
                .applyTheme(Theme(dark: cs == .dark))
        }
        .onChange(of: sessionViewModel.showSettings) { _, show in
            if show { showSettings = true; sessionViewModel.showSettings = false }
        }
    }

    // MARK: Pairing view dispatch

    @ViewBuilder
    private func pairingView(for progress: PairingProgress) -> some View {
        switch progress.step {
        case .browsingMDNS, .discoveredHost:
            // mDNS auto-discovery isn't wired yet, so the peer list will
            // stay empty. The .withManualPairSection() extension renders
            // a "Pair manually" form below the (currently empty) list —
            // that's the actual working pair path.
            DiscoverView(
                peers: $discoveredPeers,
                isScanning: $isScanning,
                networkName: networkName,
                onConnect: handleConnect,
                onManualIP: handleManualIP,
                onRescan: handleRescan
            )
            .withManualPairSection()

        case .enteringPin, .spake2Handshake, .exchangingCert:
            if let peer = selectedPeer {
                PairView(
                    host: peer,
                    pin: $enteredPin,
                    pairingToken: pairingToken,
                    expiresIn: "0:54",
                    onPinSubmit: handlePinSubmit,
                    onBack: handleBack
                )
            } else {
                // Fallback: shouldn't happen in normal flow
                WelcomeView(onGetStarted: handleGetStarted)
            }
        }
    }

    // MARK: Actions

    private func handleGetStarted() {
        isScanning = true
        Task {
            await sessionViewModel.session.startBrowsing()
            // mDNS browsing isn't actually wired yet (Plan 6 stub left a
            // hardcoded peer list as a UI demo). Real auto-discovery comes
            // when a future iteration wires NWBrowser. Until then, the
            // user pairs via the "Pair manually" card we render below the
            // list — show no peers + an explicit empty state so it's
            // obvious the user should scroll to that form.
            await MainActor.run {
                discoveredPeers = []
                isScanning = false
            }
        }
    }

    private func handleConnect(_ peer: DiscoveredPeer) {
        selectedPeer = peer
        enteredPin = ""
        Task {
            await sessionViewModel.session.awaitPin()
        }
    }

    private func handlePinSubmit(_ pin: String) {
        Task {
            let host = HostInfo(
                displayName: selectedPeer?.name ?? "",
                ipAddress: selectedPeer?.host ?? "",
                pubkeyThumbprint: "",
                osHint: selectedPeer?.osHint ?? ""
            )
            await sessionViewModel.session.submitPin(pin)
            await sessionViewModel.session.connect(to: host)
        }
    }

    private func handleManualIP() {
        // TODO: present a sheet for manual IP entry
    }

    private func handleRescan() {
        isScanning = true
        Task {
            await sessionViewModel.session.startBrowsing()
            try? await Task.sleep(for: .seconds(1))
            await MainActor.run {
                discoveredPeers = DiscoveredPeer.stubList
                isScanning = false
            }
        }
    }

    private func handleBack() {
        Task { await sessionViewModel.session.startBrowsing() }
    }

    // MARK: ViewPhase (for animation trigger)

    private var viewPhase: Int {
        switch sessionViewModel.sessionState {
        case .idle, .failed:               return 0
        case .pairing:                     return 1
        case .connecting, .live, .degraded, .disconnected: return 2
        }
    }
}

// MARK: - Note: SessionViewModel.session
// SessionViewModel.session is exposed as `public let` in LiveView.swift.
// ContentView calls session commands via sessionViewModel.session directly.

// MARK: - Stub peer list for Plan 6

extension DiscoveredPeer {
    static let stubList: [DiscoveredPeer] = [
        DiscoveredPeer(id: "1", name: "Aman's PC",        host: "192.168.1.42", port: 7779, rttMs: 6,  osHint: "Windows 11"),
        DiscoveredPeer(id: "2", name: "Studio Tower",      host: "192.168.1.15", port: 7779, rttMs: 11, osHint: "Windows 11"),
        DiscoveredPeer(id: "3", name: "MacBook Pro",       host: "192.168.1.27", port: 7779, rttMs: 18, osHint: "macOS"),
        DiscoveredPeer(id: "4", name: "Linux Workstation", host: "192.168.1.51", port: 7779, rttMs: 24, osHint: "Ubuntu 24"),
    ]
}

// MARK: - Preview

#Preview {
    ContentView()
        .environmentObject(SessionViewModel(session: IExtendSession()))
        .preferredColorScheme(.dark)
        .applyTheme(Theme(dark: true))
}
