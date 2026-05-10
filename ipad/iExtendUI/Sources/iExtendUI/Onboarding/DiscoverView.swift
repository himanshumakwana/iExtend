// DiscoverView.swift
// "iPad · Discover" artboard — port of SceneDiscovery in scenes-ipad.jsx.
// Shows discovered LAN devices with signal bars and ms readout.
// Connects to IExtendSession.startBrowsing() and observes the peer list
// from Signaling.swift via the session actor.
//
// Also contains `ManualPairViewModel` and the "Pair manually" section that
// drives the simple-pair-v0 handshake via `PairingFlow.pair(...)`.

#if canImport(UIKit)
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

                // Brand header strip
                HStack {
                    LogoLockup(.compact)
                    Spacer()
                }
                .padding(.horizontal, 36)
                .padding(.bottom, 8)

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
        VStack(spacing: 14) {
            if isScanning {
                ProgressView()
                    .progressViewStyle(.circular)
                    .tint(t.accent)
                Text("Searching on \(networkName)…")
                    .font(.body(14))
                    .foregroundStyle(t.ink2)
            } else {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 28))
                    .foregroundStyle(t.ink3)
                Text("Auto-discovery (mDNS) isn't wired up yet.")
                    .font(.body(14, weight: .semibold))
                    .foregroundStyle(t.ink)
                Text("Scroll down and use **Pair manually** to enter your laptop's IP, port, and PIN.")
                    .font(.body(12))
                    .foregroundStyle(t.ink2)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 24)
                Image(systemName: "arrow.down")
                    .font(.system(size: 18, weight: .semibold))
                    .foregroundStyle(t.accent)
                    .padding(.top, 4)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 32)
        .padding(.horizontal, 16)
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

// MARK: - ManualPairViewModel

/// Drives the "Pair manually" form and invokes `PairingFlow.pair(...)`.
@MainActor
public final class ManualPairViewModel: ObservableObject {

    // MARK: Input fields

    @Published public var hostIP: String    = "192.168.1.10"
    @Published public var portText: String  = "12345"
    @Published public var pin: String       = ""
    @Published public var deviceName: String = "My iPad"

    // MARK: State

    @Published public private(set) var isPairing: Bool = false
    @Published public private(set) var result: PairResult? = nil
    @Published public private(set) var errorMessage: String? = nil

    /// True while the toast (success or error) should be visible.
    @Published public var showToast: Bool = false

    // MARK: Derived

    var portNumber: UInt16? {
        guard let n = UInt16(portText) else { return nil }
        return n
    }

    var isInputValid: Bool {
        !hostIP.isEmpty &&
        portNumber != nil &&
        pin.count == 4 &&
        pin.allSatisfy(\.isNumber) &&
        !deviceName.isEmpty
    }

    // Lightweight IPv4-ish validation: 1–3 digits, dot, repeat, no leading zeros.
    var hostIPValid: Bool {
        let parts = hostIP.split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count == 4 else { return false }
        return parts.allSatisfy { part in
            guard let n = Int(part), n >= 0, n <= 255 else { return false }
            return part.count == 1 || part.first != "0"
        }
    }

    // MARK: Actions

    public func startPairing() {
        guard isInputValid, let port = portNumber else { return }
        isPairing = true
        errorMessage = nil
        result = nil
        showToast = false

        Task {
            do {
                let r = try await PairingFlow.pair(
                    host: hostIP,
                    port: port,
                    pin: pin,
                    displayName: deviceName
                )
                self.result = r
                self.isPairing = false
                self.showToast = true
                // Auto-dismiss success toast after 4 seconds.
                try? await Task.sleep(nanoseconds: 4_000_000_000)
                if self.result != nil { self.showToast = false }
            } catch {
                self.errorMessage = error.localizedDescription
                self.isPairing = false
                self.showToast = true
                // Auto-dismiss error toast after 6 seconds.
                try? await Task.sleep(nanoseconds: 6_000_000_000)
                if self.result == nil { self.showToast = false }
            }
        }
    }
}

// MARK: - ManualPairSection

/// The "Pair manually" card embedded at the bottom of DiscoverView.
public struct ManualPairSection: View {
    @Environment(\.colorScheme) private var cs
    @Environment(\.theme) private var t

    @StateObject private var vm = ManualPairViewModel()

    public init() {}

    public var body: some View {
        VStack(alignment: .leading, spacing: 14) {

            // Section header
            Text("Pair manually")
                .font(.body(13, weight: .semibold))
                .foregroundStyle(t.ink2)
                .textCase(.uppercase)
                .kerning(0.1)

            // Form card
            VStack(spacing: 0) {
                fieldRow(label: "Host IP", placeholder: "192.168.1.10",
                         text: $vm.hostIP, keyboard: .decimalPad)
                    .overlay(
                        vm.hostIP.isEmpty || vm.hostIPValid
                        ? nil
                        : AnyView(
                            HStack {
                                Spacer()
                                Image(systemName: "exclamationmark.circle")
                                    .foregroundStyle(t.red)
                                    .padding(.trailing, 16)
                            }
                        )
                    )

                divider

                fieldRow(label: "Port", placeholder: "12345",
                         text: $vm.portText, keyboard: .numberPad)

                divider

                fieldRow(label: "PIN", placeholder: "4 digits",
                         text: $vm.pin, keyboard: .numberPad)
                    .onChange(of: vm.pin) { _, new in
                        // Clamp to 4 digits.
                        let filtered = String(new.filter(\.isNumber).prefix(4))
                        if filtered != new { vm.pin = filtered }
                    }

                divider

                fieldRow(label: "Device name", placeholder: "My iPad",
                         text: $vm.deviceName, keyboard: .default)
            }
            .background(
                RoundedRectangle(cornerRadius: 14)
                    .fill(t.card)
                    .overlay(
                        RoundedRectangle(cornerRadius: 14)
                            .strokeBorder(t.sep, lineWidth: 1)
                    )
            )
            .clipShape(RoundedRectangle(cornerRadius: 14))

            // Pair button
            Button(action: vm.startPairing) {
                HStack(spacing: 8) {
                    if vm.isPairing {
                        ProgressView()
                            .progressViewStyle(.circular)
                            .tint(.white)
                            .scaleEffect(0.8)
                    }
                    Text(vm.isPairing ? "Pairing…" : "Pair")
                        .font(.body(15, weight: .semibold))
                }
                .frame(maxWidth: .infinity)
                .padding(.vertical, 13)
                .background(
                    RoundedRectangle(cornerRadius: 12)
                        .fill(vm.isInputValid && !vm.isPairing
                              ? t.accent
                              : t.accent.opacity(0.4))
                )
                .foregroundStyle(.white)
            }
            .disabled(!vm.isInputValid || vm.isPairing)
            .buttonStyle(.plain)

            // Toast overlay (success / error)
            if vm.showToast {
                toastView
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .animation(.spring(duration: 0.3), value: vm.showToast)
    }

    // MARK: Sub-views

    private func fieldRow(
        label: String,
        placeholder: String,
        text: Binding<String>,
        keyboard: UIKeyboardType
    ) -> some View {
        HStack(spacing: 12) {
            Text(label)
                .font(.body(14))
                .foregroundStyle(t.ink)
                .frame(minWidth: 100, alignment: .leading)

            TextField(placeholder, text: text)
                .keyboardType(keyboard)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
                .font(.body(14))
                .foregroundStyle(t.ink)
                .multilineTextAlignment(.trailing)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
    }

    private var divider: some View {
        Divider()
            .frame(height: 0.5)
            .background(t.sep)
            .padding(.leading, 16)
    }

    private var toastView: some View {
        HStack(spacing: 10) {
            if let result = vm.result {
                Image(systemName: "checkmark.circle.fill")
                    .foregroundStyle(t.green)
                VStack(alignment: .leading, spacing: 2) {
                    Text("Paired!")
                        .font(.body(14, weight: .semibold))
                        .foregroundStyle(t.ink)
                    Text("ID: \(result.pairId.prefix(8))…")
                        .font(.mono(11))
                        .foregroundStyle(t.ink2)
                }
            } else {
                Image(systemName: "xmark.circle.fill")
                    .foregroundStyle(t.red)
                Text(vm.errorMessage ?? "Unknown error")
                    .font(.body(13))
                    .foregroundStyle(t.ink)
                    .lineLimit(3)
            }
            Spacer()
        }
        .padding(14)
        .background(
            RoundedRectangle(cornerRadius: 12)
                .fill(t.card2)
                .overlay(
                    RoundedRectangle(cornerRadius: 12)
                        .strokeBorder(
                            vm.result != nil ? t.green.opacity(0.4) : t.red.opacity(0.4),
                            lineWidth: 1
                        )
                )
        )
        .shadow(color: .black.opacity(0.12), radius: 8, y: 4)
    }
}

// MARK: - DiscoverView extension: inject ManualPairSection

extension DiscoverView {
    /// A version of the view body that appends the manual-pair section.
    /// Call `discoverWithManualPair()` from the navigation host instead of
    /// `DiscoverView(...)` when you want the full onboarding flow.
    public func withManualPairSection() -> some View {
        VStack(spacing: 0) {
            self
            ManualPairSection()
                .padding(.horizontal, 36)
                .padding(.bottom, 28)
        }
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
#endif
