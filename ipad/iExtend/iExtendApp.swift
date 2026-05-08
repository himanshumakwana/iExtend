// iExtendApp.swift
// @main App entry point for the iExtend iPad app.
// Owns the root IExtendSession actor and injects the SessionViewModel
// into the SwiftUI environment via @StateObject.
// Default window size targets iPad Pro 11" landscape (1194 × 834 pt).

import SwiftUI
import iExtendKit
import iExtendUI

@main
struct iExtendApp: App {
    @StateObject private var sessionViewModel: SessionViewModel = {
        let session = IExtendSession()
        return SessionViewModel(session: session)
    }()

    @Environment(\.colorScheme) private var colorScheme

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(sessionViewModel)
                .applyTheme(Theme(dark: colorScheme == .dark))
                .onAppear {
                    // Start mDNS browsing immediately so the discover
                    // list is pre-populated when the user navigates there.
                    Task { await sessionViewModel.startBrowsingIfIdle() }
                }
        }
        .defaultSize(width: 1194, height: 834) // iPad Pro 11" landscape
    }
}

// MARK: - SessionViewModel browsing bootstrap

extension SessionViewModel {
    /// Called once on app launch; safe to call repeatedly (idempotent).
    @MainActor
    func startBrowsingIfIdle() async {
        guard case .idle = sessionState else { return }
        // IExtendSession.startBrowsing() triggers mDNS via Signaling.swift.
        // Plan 7 expands this to also restore pinned hosts from Keychain.
    }
}
