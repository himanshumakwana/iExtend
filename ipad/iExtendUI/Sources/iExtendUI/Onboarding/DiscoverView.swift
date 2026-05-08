// DiscoverView.swift
// "iPad · Discover" artboard — port of SceneDiscovery in scenes-ipad.jsx.
// Shows discovered LAN devices with signal bars and ms readout.
// Connects to IExtendSession.startBrowsing() and observes the peer list
// from Signaling.swift via the session actor.

import SwiftUI
import iExtendKit

public struct DiscoverView: View {
    @Environment(\.colorScheme) private var cs
    @Environment(\.theme) private var t

    @Binding var peers: [DiscoveredPeer]
    @Binding var isScanning: Bool
    var networkName: String
    var onConnect: (DiscoveredPeer) -> Void
    var onManualIP: () -> Void
    var onRescan: () -> Void

    @State private var selectedPeer: DiscoveredPeer?

    public init(
        peers: Binding<[DiscoveredPeer]>,
        isScanning: Binding<Bool>,
        networkName: String = "Unknown",
        onConnect: @escaping (DiscoveredPeer) -> Void,
        onManualIP: @escaping () -> Void,
        onRescan: @escaping () -> Void
    ) {
        self._peers = peers
        self._isScanning = isScanning
        self.networkName = networkName
        self.onConnect = onConnect
        self.onManualIP = onManualIP
        self.onRescan = onRescan
    }

    public var body: some View {
        ZStack(alignment: .bottom) {
            AuroraBackground().ignoresSafeArea()

            VStack(spacing: 0) {
                // Status bar spacing
                Color.clear.frame(height: 38)

                VStack(alignment: .leading, spacing: 14) {
                    // Header row
                    HStack(alignment: .lastTextBaseline) {
                        VStack(alignment: .leading, spacing: 4) {
                            Text("Step 2 of 3 · Discover")
                                .font(.body(11, weight: .medium))
                                .foregroundStyle(t.ink2)
                                .textCase(.uppercase)
                                .kerning(0.1)

                            Text("Looking for your computer\u{2026}")
                                .font(.display(28, weight: .bold))
                                .foregroundStyle(t.ink)
                                .kerning(-0.03 * 28)

                            HStack(spacing: 6) {
                                Image(systemName: "wifi")
                                    .font(.system(size: 13))
                                    .foregroundStyle(t.ink2)
                                (Text("On ") + Text(networkName).fontWeight(.bold).foregroundStyle(t.ink) + Text(" · \(peers.count) device\(peers.count == 1 ? "" : "s")"))
                                    .font(.body(13))
                                    .foregroundStyle(t.ink2)
                            }
                        }

                        Spacer()

                        HStack(spacing: 6) {
                            DimButton(label: "Rescan", sysIcon: "arrow.clockwise", action: onRescan)
                            DimButton(label: "Manual IP", sysIcon: "plus", action: onManualIP)
                        }
                    }

                    // Device list card
                    VStack(spacing: 0) {
                        if peers.isEmpty {
                            emptyState
                        } else {
                            ForEach(Array(peers.enumerated()), id: \.element.id) { idx, peer in
                                DeviceRow(
                                    peer: peer,
                                    isSelected: selectedPeer?.id == peer.id,
                                    isFirst: idx == 0,
                                    isLast: idx == peers.count - 1,
                                    onTap: { selectedPeer = peer },
                                    onConnect: { onConnect(peer) }
                                )
                                if idx < peers.count - 1 {
                                    Divider()
                                        .frame(height: 0.5)
                                        .background(t.sep)
                                        .padding(.leading, 64)
                                }
                            }
                        }
                    }
                    .background(
                        RoundedRectangle(cornerRadius: 18)
                            .fill(t.card)
                            .overlay(
                                RoundedRectangle(cornerRadius: 18)
                                    .strokeBorder(t.sep, lineWidth: 1)
                            )
                    )
                    .clipShape(RoundedRectangle(cornerRadius: 18))

                    Spacer()

                    // Help footer
                    HStack(spacing: 8) {
                        ZStack {
                            RoundedRectangle(cornerRadius: 6)
                                .fill(cs == .dark ? Color.white.opacity(0.06) : Color.black.opacity(0.04))
                            Image(systemName: "bolt.fill")
                                .font(.system(size: 12))
                                .foregroundStyle(t.accent)
                        }
                        .frame(width: 22, height: 22)

                        Text("Don't see your PC? Make sure the iExtend desktop app is running on the same network.")
                            .font(.body(11))
                            .foregroundStyle(t.ink2)
                    }
                    .padding(.bottom, 12)
                }
                .padding(.horizontal, 36)
                .padding(.top, 38)
                .padding(.bottom, 22)
            }

            IPadHomeIndicator()
        }
        .background(t.bg.ignoresSafeArea())
        .onAppear {
            if let first = peers.first { selectedPeer = first }
        }
    }

    // MARK: Empty state

    private var emptyState: some View {
        VStack(spacing: 12) {
            if isScanning {
                ProgressView()
                    .progressViewStyle(.circular)
                    .tint(t.accent)
            } else {
                Image(systemName: "wifi.slash")
                    .font(.system(size: 32))
                    .foregroundStyle(t.ink3)
            }
            Text(isScanning ? "Searching on \(networkName)…" : "No devices found")
                .font(.body(14))
                .foregroundStyle(t.ink2)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 40)
    }
}

// MARK: - DeviceRow

public struct DeviceRow: View {
    @Environment(\.colorScheme) private var cs
    @Environment(\.theme) private var t

    let peer: DiscoveredPeer
    let isSelected: Bool
    let isFirst: Bool
    let isLast: Bool
    var onTap: () -> Void
    var onConnect: () -> Void

    public var body: some View {
        HStack(spacing: 12) {
            // Icon
            ZStack {
                RoundedRectangle(cornerRadius: 10)
                    .fill(cs == .dark ? Color.white.opacity(0.06) : Color.black.opacity(0.04))
                Image(systemName: "display")
                    .font(.system(size: 18))
                    .foregroundStyle(isSelected ? t.accent : t.ink)
            }
            .frame(width: 36, height: 36)

            // Name + subtitle
            VStack(alignment: .leading, spacing: 1) {
                Text(peer.name)
                    .font(.body(14, weight: .semibold))
                    .foregroundStyle(t.ink)
                    .kerning(-0.2)
                Text("\(peer.osHint) · \(peer.host)")
                    .font(.body(11))
                    .foregroundStyle(t.ink2)
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            // Signal bars
            SignalBars(bars: signalBars(rttMs: peer.rttMs))
                .padding(.trailing, 8)

            // Latency
            Text("~\(Int(peer.rttMs)) ms")
                .font(.mono(11))
                .foregroundStyle(t.ink2)
                .frame(minWidth: 50, alignment: .trailing)

            // Action
            if isSelected {
                PillButton(label: "Connect", style: .primary) { onConnect() }
                    .frame(height: 30)
            } else {
                Image(systemName: "chevron.right")
                    .font(.system(size: 11))
                    .foregroundStyle(t.ink3)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 11)
        .background(
            isSelected
            ? (cs == .dark ? t.accent.opacity(0.14) : t.accent.opacity(0.08))
            : Color.clear
        )
        .contentShape(Rectangle())
        .onTapGesture { onTap() }
    }
}

// MARK: - SignalBars

public struct SignalBars: View {
    @Environment(\.theme) private var t
    let bars: Int   // 1…4

    public var body: some View {
        HStack(alignment: .bottom, spacing: 2) {
            ForEach(1...4, id: \.self) { b in
                Capsule()
                    .fill(b <= bars ? t.ink : t.ink3)
                    .frame(width: 3, height: CGFloat(3 + b * 2))
            }
        }
    }
}

// MARK: - DimButton helper

private struct DimButton: View {
    @Environment(\.colorScheme) private var cs
    @Environment(\.theme) private var t
    let label: String
    let sysIcon: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                Image(systemName: sysIcon)
                    .font(.system(size: 13))
                Text(label)
                    .font(.body(13))
            }
            .foregroundStyle(t.ink)
            .padding(.horizontal, 11)
            .padding(.vertical, 7)
            .background(
                Capsule()
                    .fill(cs == .dark ? Color.white.opacity(0.08) : Color.black.opacity(0.06))
            )
        }
        .buttonStyle(.plain)
    }
}

// MARK: - Preview

#Preview {
    DiscoverView(
        peers: .constant([
            DiscoveredPeer(id: "1", name: "Aman's PC",       host: "192.168.1.42", port: 7779, rttMs: 6,  osHint: "Windows 11"),
            DiscoveredPeer(id: "2", name: "Studio Tower",     host: "192.168.1.15", port: 7779, rttMs: 11, osHint: "Windows 11"),
            DiscoveredPeer(id: "3", name: "MacBook Pro",      host: "192.168.1.27", port: 7779, rttMs: 18, osHint: "macOS"),
            DiscoveredPeer(id: "4", name: "Linux Workstation",host: "192.168.1.51", port: 7779, rttMs: 24, osHint: "Ubuntu"),
        ]),
        isScanning: .constant(false),
        networkName: "HomeNet 5G",
        onConnect: { _ in },
        onManualIP: {},
        onRescan: {}
    )
    .preferredColorScheme(.dark)
    .applyTheme(Theme(dark: true))
}
