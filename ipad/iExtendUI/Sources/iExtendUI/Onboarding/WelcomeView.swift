// WelcomeView.swift
// "iPad · Welcome" artboard — 1:1 port of SceneWelcome in scenes-ipad.jsx.
// Layout: two-column grid. Left: copy + CTAs + feature strip.
// Right: mode-picker card (Extend / Mirror / Drawing tablet).

#if canImport(UIKit)
import SwiftUI
import iExtendKit

public struct WelcomeView: View {
    @Environment(\.colorScheme) private var cs
    @Environment(\.theme) private var t
    var onGetStarted: () -> Void

    public init(onGetStarted: @escaping () -> Void) {
        self.onGetStarted = onGetStarted
    }

    public var body: some View {
        ZStack(alignment: .bottom) {
            // Aurora gradient background
            AuroraBackground()
                .ignoresSafeArea()

            GeometryReader { geo in
                HStack(spacing: 0) {
                    // ── Left column ─────────────────────────────────────
                    leftColumn
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.leading, 56)
                        .padding(.trailing, 8)
                        .padding(.vertical, 40)

                    // ── Right column ────────────────────────────────────
                    modePickerCard
                        .padding(.trailing, 30)
                        .frame(maxWidth: geo.size.width * 0.46)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }

            IPadHomeIndicator()
        }
        .background(t.bg.ignoresSafeArea())
    }

    // MARK: Left column

    private var leftColumn: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Version badge
            HStack(spacing: 8) {
                Circle()
                    .fill(t.accent)
                    .frame(width: 6, height: 6)
                Text("iExtend 1.0")
                    .font(.body(12, weight: .medium))
                    .foregroundStyle(t.ink2)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .background(
                Capsule()
                    .fill(cs == .dark ? Color.white.opacity(0.06) : Color.black.opacity(0.04))
                    .overlay(Capsule().strokeBorder(t.sep, lineWidth: 1))
            )

            // Headline
            VStack(alignment: .leading, spacing: 0) {
                Text("Your iPad,")
                    .font(.display(44, weight: .bold))
                    .foregroundStyle(t.ink)
                Text("a second screen.")
                    .font(.display(44, weight: .bold))
                    .foregroundStyle(
                        LinearGradient.brandGradient
                    )
            }
            .padding(.top, 14)
            .padding(.bottom, 12)
            .lineSpacing(2)

            // Tagline
            Text("Extend your PC to your iPad over Wi\u{2011}Fi. No cables, no drivers — just pick a workspace and keep going.")
                .font(.body(15))
                .foregroundStyle(t.ink2)
                .lineSpacing(4)
                .frame(maxWidth: 320, alignment: .leading)
                .padding(.bottom, 22)

            // CTAs
            HStack(spacing: 10) {
                PillButton(label: "Get started", style: .primary) { onGetStarted() }
                PillButton(label: "Learn more",  style: .ghost)   { }
            }

            // Feature strip
            HStack(spacing: 22) {
                ForEach(featureItems, id: \.label) { item in
                    Label(item.label, systemImage: item.sysImage)
                        .font(.body(13))
                        .foregroundStyle(t.ink2)
                        .labelStyle(.iconFirst)
                }
            }
            .padding(.top, 28)

            Spacer()
        }
    }

    // MARK: Mode picker card

    private var modePickerCard: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("How will you use it?")
                .font(.body(13, weight: .semibold))
                .foregroundStyle(t.ink2)

            ForEach(modeItems.indices, id: \.self) { i in
                modeRow(at: i)
            }
        }
        .padding(22)
        .background(modePickerCardBackground)
        .frame(width: 270)
    }

    /// Single mode-picker row. Extracted out of `modePickerCard` to keep the
    /// SwiftUI type-checker from dying on the ternary-laden background
    /// expression — Swift hits "unable to type-check this expression in
    /// reasonable time" on the original inline form.
    @ViewBuilder
    private func modeRow(at i: Int) -> some View {
        let mode = modeItems[i]
        let isSelected = i == 0
        let rowFillSelected = Color(hex: mode.tint).opacity(0.8)
        let rowFillUnselected: Color = (cs == .dark
            ? Color.white.opacity(0.04)
            : Color.black.opacity(0.03))
        let rowFill: Color = isSelected ? rowFillSelected : rowFillUnselected
        let strokeColor: Color = isSelected ? t.accent : t.sep
        let strokeWidth: CGFloat = isSelected ? 1.5 : 1

        HStack(spacing: 14) {
            ZStack {
                RoundedRectangle(cornerRadius: 12)
                    .fill(Color(hex: mode.tint))
                Image(systemName: mode.sysIcon)
                    .font(.system(size: 16, weight: .medium))
                    .foregroundStyle(mode.iconColor)
            }
            .frame(width: 44, height: 44)

            VStack(alignment: .leading, spacing: 2) {
                Text(mode.title)
                    .font(.body(15, weight: .semibold))
                    .foregroundStyle(t.ink)
                Text(mode.subtitle)
                    .font(.body(12))
                    .foregroundStyle(t.ink2)
            }
            Spacer()

            if isSelected {
                Image(systemName: "checkmark")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(t.accent)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
        .background(
            RoundedRectangle(cornerRadius: 16)
                .fill(rowFill)
                .overlay(
                    RoundedRectangle(cornerRadius: 16)
                        .strokeBorder(strokeColor, lineWidth: strokeWidth)
                )
        )
    }

    /// Card background extracted for the same type-check-time reason.
    @ViewBuilder
    private var modePickerCardBackground: some View {
        let cardFill: Color = cs == .dark
            ? Color(hex: "#1c1c1e").opacity(0.7)
            : Color.white.opacity(0.65)
        let shadowOpacity: Double = cs == .dark ? 0.5 : 0.12
        let strokeColor: Color = cs == .dark
            ? Color.white.opacity(0.1)
            : Color.black.opacity(0.05)

        RoundedRectangle(cornerRadius: 28)
            .fill(cardFill)
            .shadow(color: .black.opacity(shadowOpacity), radius: 30, y: 10)
            .overlay(
                RoundedRectangle(cornerRadius: 28)
                    .strokeBorder(strokeColor, lineWidth: 1)
            )
    }

    // MARK: Data

    private struct FeatureItem { let label: String; let sysImage: String }
    private let featureItems = [
        FeatureItem(label: "Wi\u{2011}Fi or USB", sysImage: "wifi"),
        FeatureItem(label: "Pencil ready",        sysImage: "pencil.tip"),
        FeatureItem(label: "Up to 120 Hz",        sysImage: "bolt.fill"),
    ]

    private struct ModeItem {
        let title, subtitle, sysIcon, tint: String
        let iconColor: Color
    }
    private var modeItems: [ModeItem] {[
        ModeItem(title: "Extend desktop", subtitle: "More room for windows",  sysIcon: "rectangle.on.rectangle",   tint: "rgba(10,132,255,0.12)",  iconColor: t.accent),
        ModeItem(title: "Mirror screen",  subtitle: "Show the same view",     sysIcon: "rectangle.2.swap",         tint: "rgba(94,92,230,0.12)",   iconColor: t.indigo),
        ModeItem(title: "Drawing tablet", subtitle: "Pencil + Wacom mode",    sysIcon: "pencil.and.outline",       tint: "rgba(255,159,10,0.14)",  iconColor: t.orange),
    ]}
}

// MARK: - Shared sub-components used across onboarding scenes

struct AuroraBackground: View {
    @Environment(\.colorScheme) private var cs

    var body: some View {
        ZStack {
            if cs == .dark {
                RadialGradient(
                    colors: [Color(hex: "#5e5ce6").opacity(0.45), .clear],
                    center: UnitPoint(x: 0.25, y: 0.15),
                    startRadius: 0, endRadius: 500
                )
                RadialGradient(
                    colors: [Color(hex: "#0a84ff").opacity(0.40), .clear],
                    center: UnitPoint(x: 0.80, y: 0.90),
                    startRadius: 0, endRadius: 520
                )
                RadialGradient(
                    colors: [Color(hex: "#ff9f0a").opacity(0.18), .clear],
                    center: UnitPoint(x: 0.90, y: 0.10),
                    startRadius: 0, endRadius: 380
                )
            } else {
                RadialGradient(
                    colors: [Color(hex: "#5e5ce6").opacity(0.20), .clear],
                    center: UnitPoint(x: 0.25, y: 0.15),
                    startRadius: 0, endRadius: 500
                )
                RadialGradient(
                    colors: [Color(hex: "#0a84ff").opacity(0.20), .clear],
                    center: UnitPoint(x: 0.80, y: 0.90),
                    startRadius: 0, endRadius: 520
                )
            }
        }
    }
}

struct IPadHomeIndicator: View {
    @Environment(\.colorScheme) private var cs
    var body: some View {
        VStack {
            Spacer()
            Capsule()
                .fill(cs == .dark ? Color.white.opacity(0.3) : Color.black.opacity(0.2))
                .frame(width: 134, height: 5)
                .padding(.bottom, 8)
        }
    }
}

// MARK: - LabelStyle helper (icon first)

struct IconFirstLabelStyle: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 6) {
            configuration.icon
            configuration.title
        }
    }
}

extension LabelStyle where Self == IconFirstLabelStyle {
    static var iconFirst: Self { IconFirstLabelStyle() }
}

// MARK: - Preview

#Preview {
    WelcomeView(onGetStarted: {})
        .preferredColorScheme(.dark)
        .applyTheme(Theme(dark: true))
}
#endif
