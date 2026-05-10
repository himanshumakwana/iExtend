// LogoLockup.swift
// Horizontal lockup of LogoMark + "iExtend" wordmark. Two presets:
// .compact (24pt mark, 14pt word) for headers and toolbars,
// .hero    (46pt mark, 22pt word) for the Welcome screen.
// All sizing constants live in BrandGradient.swift's BrandLockup enum.

#if canImport(UIKit)
import SwiftUI

public struct LogoLockup: View {
    @Environment(\.theme) private var t

    public enum Size {
        case compact, hero

        var markSize: CGFloat {
            switch self {
            case .compact: return BrandLockup.Compact.markSize
            case .hero:    return BrandLockup.Hero.markSize
            }
        }

        var wordSize: CGFloat {
            switch self {
            case .compact: return BrandLockup.Compact.wordSize
            case .hero:    return BrandLockup.Hero.wordSize
            }
        }
    }

    public let size: Size
    public let floats: Bool

    /// `floats` defaults to true for hero (welcome screen) and false for
    /// compact (header strips, where we don't want a halo bleeding into
    /// neighboring chrome).
    public init(_ size: Size = .compact, floats: Bool? = nil) {
        self.size = size
        self.floats = floats ?? (size == .hero)
    }

    public var body: some View {
        HStack(spacing: size.markSize * BrandLockup.markToWordSpacingRatio) {
            LogoMark(size: size.markSize, floats: floats)
            Text("iExtend")
                .font(.display(size.wordSize, weight: .semibold))
                .kerning(-0.01 * size.wordSize)
                .foregroundStyle(t.ink)
        }
    }
}

#Preview("Compact + Hero, dark") {
    VStack(alignment: .leading, spacing: 32) {
        LogoLockup(.compact)
        LogoLockup(.hero)
    }
    .padding(40)
    .background(Color(hex: "#0f0f14"))
    .preferredColorScheme(.dark)
    .applyTheme(Theme(dark: true))
}

#Preview("Compact + Hero, light") {
    VStack(alignment: .leading, spacing: 32) {
        LogoLockup(.compact)
        LogoLockup(.hero)
    }
    .padding(40)
    .background(Color(hex: "#f2f2f7"))
    .preferredColorScheme(.light)
    .applyTheme(Theme(dark: false))
}
#endif
