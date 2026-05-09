// PairView.swift
// "iPad · Pair" artboard — port of ScenePairing in scenes-ipad.jsx.
// Two-column layout: Left = QR code card; Right = PinEntryView.
// Plan 7 replaces the stub QR + PIN stub with SPAKE2 flow via PairingFlow.

#if canImport(UIKit)
import SwiftUI
import iExtendKit

public struct PairView: View {
    @Environment(\.colorScheme) private var cs
    @Environment(\.theme) private var t

    let host: DiscoveredPeer
    @Binding var pin: String
    let pairingToken: String        // "iextend://pair/xxxx" URI shown in QR
    let expiresIn: String           // countdown string, e.g. "0:54"
    var onPinSubmit: (String) -> Void
    var onBack: () -> Void

    public init(
        host: DiscoveredPeer,
        pin: Binding<String>,
        pairingToken: String = "iextend://pair/xxxxxxxx",
        expiresIn: String = "1:00",
        onPinSubmit: @escaping (String) -> Void,
        onBack: @escaping () -> Void
    ) {
        self.host = host
        self._pin = pin
        self.pairingToken = pairingToken
        self.expiresIn = expiresIn
        self.onPinSubmit = onPinSubmit
        self.onBack = onBack
    }

    public var body: some View {
        ZStack(alignment: .bottom) {
            AuroraBackground().ignoresSafeArea()

            VStack(spacing: 0) {
                Color.clear.frame(height: 40) // status bar

                HStack(spacing: 22) {
                    // ── Left: QR card ──────────────────────────────────
                    qrCard
                        .frame(maxWidth: .infinity)

                    // ── Right: PIN entry ───────────────────────────────
                    PinEntryView(
                        pin: $pin,
                        expiresIn: expiresIn,
                        onSubmit: onPinSubmit
                    )
                    .frame(maxWidth: .infinity)
                }
                .padding(.horizontal, 36)
                .padding(.top, 40)
                .padding(.bottom, 28)

                Spacer()
            }

            IPadHomeIndicator()
        }
        .background(t.bg.ignoresSafeArea())
    }

    // MARK: QR card

    private var qrCard: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Scan from your PC")
                .font(.body(11, weight: .semibold))
                .foregroundStyle(t.ink2)
                .textCase(.uppercase)
                .kerning(0.1)

            Text("Point the iExtend app\u{2019}s camera at this code")
                .font(.body(17, weight: .bold))
                .foregroundStyle(t.ink)
                .kerning(-0.02 * 17)
                .lineSpacing(2)
                .padding(.top, 4)
                .padding(.bottom, 14)

            // Faux QR grid (21×21 dots — same deterministic RNG as JSX)
            FauxQRCode()
                .frame(width: 180, height: 180)
                .padding(12)
                .background(Color.white)
                .clipShape(RoundedRectangle(cornerRadius: 18))
                .shadow(color: .black.opacity(0.18), radius: 14, y: 8)
                .frame(maxWidth: .infinity)

            Text(pairingToken)
                .font(.mono(13))
                .foregroundStyle(t.ink2)
                .padding(.top, 18)
                .frame(maxWidth: .infinity, alignment: .center)

            Spacer()
        }
        .padding(20)
        .background(
            RoundedRectangle(cornerRadius: 24)
                .fill(t.card)
                .overlay(
                    RoundedRectangle(cornerRadius: 24)
                        .strokeBorder(t.sep, lineWidth: 1)
                )
        )
    }
}

// MARK: - FauxQRCode

/// 21×21 deterministic dot grid mirroring the JSX faux-QR implementation.
private struct FauxQRCode: View {
    let size = 21

    var body: some View {
        GeometryReader { geo in
            let cellW = geo.size.width  / CGFloat(size)
            let cellH = geo.size.height / CGFloat(size)
            Canvas { ctx, _ in
                for row in 0..<size {
                    for col in 0..<size {
                        let idx = row * size + col
                        let r = UInt64(bitPattern: Int64(idx) * 9301 &+ 49297) % 233280
                        let filled = inFinder(x: col, y: row) || Double(r) / 233280.0 > 0.5
                        if filled {
                            let rect = CGRect(
                                x: CGFloat(col) * cellW + 0.5,
                                y: CGFloat(row) * cellH + 0.5,
                                width: cellW - 1,
                                height: cellH - 1
                            )
                            ctx.fill(Path(rect), with: .color(.black))
                        }
                    }
                }
            }
        }
    }

    private func inFinder(x: Int, y: Int) -> Bool {
        func block(ox: Int, oy: Int) -> Bool {
            guard x >= ox && x <= ox + 6 && y >= oy && y <= oy + 6 else { return false }
            let inner = x > ox + 1 && x < ox + 5 && y > oy + 1 && y < oy + 5
            let core  = x >= ox + 2 && x <= ox + 4 && y >= oy + 2 && y <= oy + 4
            return !inner || core
        }
        return block(ox: 0, oy: 0) || block(ox: 14, oy: 0) || block(ox: 0, oy: 14)
    }
}

// MARK: - Preview

#Preview {
    struct Wrapper: View {
        @State var pin = ""
        let peer = DiscoveredPeer(id: "1", name: "Aman's PC", host: "192.168.1.42", port: 7779, rttMs: 6, osHint: "Windows 11")
        var body: some View {
            PairView(
                host: peer,
                pin: $pin,
                pairingToken: "iextend://pair/9k2-4npx",
                expiresIn: "0:54",
                onPinSubmit: { p in print("PIN: \(p)") },
                onBack: {}
            )
            .preferredColorScheme(.dark)
            .applyTheme(Theme(dark: true))
        }
    }
    return Wrapper()
}
#endif
