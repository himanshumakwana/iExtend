// DisconnectedOverlay.swift
// "iPad · Disconnected" state overlay.
// Mirrors the DisconnectedOverlay component in scenes-ipad.jsx.
// Shown for SessionState.disconnected and .failed.

import SwiftUI

public struct DisconnectedOverlay: View {
    let hostName: String
    let hostIP: String
    var onRetry: () -> Void
    var onCancel: () -> Void

    // Auto-reconnect countdown (3 s)
    @State private var countdown: Int = 3
    @State private var timer: Timer?

    public init(
        hostName: String = "PC",
        hostIP: String = "192.168.x.x",
        onRetry: @escaping () -> Void = {},
        onCancel: @escaping () -> Void = {}
    ) {
        self.hostName = hostName
        self.hostIP   = hostIP
        self.onRetry  = onRetry
        self.onCancel = onCancel
    }

    public var body: some View {
        ZStack {
            // Frosted backdrop
            Color.black.opacity(0.65)
                .ignoresSafeArea()
                .background(.regularMaterial.opacity(0.2))

            // Card
            VStack(spacing: 0) {
                // Warning icon
                ZStack {
                    Circle()
                        .fill(Color(hex: "#ff453a").opacity(0.18))
                        .frame(width: 56, height: 56)
                    Image(systemName: "exclamationmark.triangle.fill")
                        .font(.system(size: 24, weight: .semibold))
                        .foregroundStyle(Color(hex: "#ff453a"))
                }

                Text("Lost connection to PC")
                    .font(.system(size: 19, weight: .bold))
                    .foregroundStyle(.white)
                    .padding(.top, 14)

                // Description
                VStack(spacing: 2) {
                    Text("Wi\u{2011}Fi signal dropped at")
                        .foregroundStyle(Color.white.opacity(0.65))
                    + Text(" ")
                    + Text(hostIP)
                        .font(.system(.body, design: .monospaced))
                        .foregroundStyle(Color.white.opacity(0.65))
                    Text(countdown > 0
                         ? "Reconnecting in \(countdown)s\u{2026}"
                         : "Attempting to reconnect\u{2026}")
                        .fontWeight(.semibold)
                        .foregroundStyle(.white)
                }
                .font(.system(size: 13))
                .multilineTextAlignment(.center)
                .lineSpacing(3)
                .padding(.top, 6)
                .padding(.horizontal, 16)

                // CTA buttons
                HStack(spacing: 10) {
                    Button("Try again now") { onRetry() }
                        .buttonStyle(PillButtonStyle(style: .primary))

                    Button("Cancel") { onCancel() }
                        .buttonStyle(PillButtonStyle(style: .secondary))
                }
                .padding(.top, 18)
            }
            .padding(28)
            .frame(width: 360)
            .background(
                RoundedRectangle(cornerRadius: 24)
                    .fill(Color(red: 28/255, green: 28/255, blue: 30/255).opacity(0.92))
                    .overlay(
                        RoundedRectangle(cornerRadius: 24)
                            .strokeBorder(Color(hex: "#ff453a").opacity(0.4), lineWidth: 1)
                    )
                    .shadow(color: .black.opacity(0.55), radius: 30, y: 10)
            )
        }
        .onAppear { startCountdown() }
        .onDisappear { timer?.invalidate() }
    }

    // MARK: Countdown

    private func startCountdown() {
        countdown = 3
        timer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { t in
            if countdown > 0 {
                countdown -= 1
            } else {
                t.invalidate()
                onRetry()
            }
        }
    }
}

// MARK: - PillButtonStyle

public enum PillButtonKind { case primary, secondary, ghost }

public struct PillButtonStyle: ButtonStyle {
    let style: PillButtonKind

    public func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 15, weight: .semibold))
            .foregroundStyle(foregroundColor)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 12)
            .background(
                Capsule().fill(backgroundColor)
                    .opacity(configuration.isPressed ? 0.75 : 1)
            )
    }

    private var backgroundColor: Color {
        switch style {
        case .primary:   return Color(hex: "#0a84ff")
        case .secondary: return Color(white: 0.3, opacity: 0.5)
        case .ghost:     return Color.clear
        }
    }

    private var foregroundColor: Color {
        switch style {
        case .primary:   return .white
        case .secondary: return .white
        case .ghost:     return Color(hex: "#0a84ff")
        }
    }
}

// MARK: - PillButton (SwiftUI-native CTA)

public struct PillButton: View {
    public enum Style { case primary, secondary, ghost }
    let label: String
    let style: Style
    let action: () -> Void

    public init(label: String, style: Style = .primary, action: @escaping () -> Void) {
        self.label = label; self.style = style; self.action = action
    }

    public var body: some View {
        Button(label, action: action)
            .buttonStyle(PillButtonStyle(style: pillKind))
    }

    private var pillKind: PillButtonKind {
        switch style {
        case .primary:   return .primary
        case .secondary: return .secondary
        case .ghost:     return .ghost
        }
    }
}

// MARK: - Preview

#Preview {
    ZStack {
        Color(hex: "#0c1430").ignoresSafeArea()
        DisconnectedOverlay(
            hostName: "Aman's PC",
            hostIP: "192.168.1.42",
            onRetry: {},
            onCancel: {}
        )
    }
    .preferredColorScheme(.dark)
}
