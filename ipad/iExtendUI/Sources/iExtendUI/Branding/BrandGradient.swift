// BrandGradient.swift
// Brand-gradient extensions on LinearGradient. The three fold-panel
// gradients (blue, indigo, purple) used by LogoMark, plus the wordmark
// spacing constant used by LogoLockup. Sits alongside the existing
// LinearGradient.brandGradient (Theme.swift) — same namespace, single
// source of truth for the brand palette.

import SwiftUI

public extension LinearGradient {
    /// Fold panel 1 (left). Blue → deep blue.
    static let brandFoldBlue = LinearGradient(
        colors: [Color(hex: "#0a84ff"), Color(hex: "#0050b8")],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
    )

    /// Fold panel 2 (middle). Indigo → deep indigo.
    static let brandFoldIndigo = LinearGradient(
        colors: [Color(hex: "#5e5ce6"), Color(hex: "#3f3d80")],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
    )

    /// Fold panel 3 (right). Purple → deep purple.
    static let brandFoldPurple = LinearGradient(
        colors: [Color(hex: "#bf5af2"), Color(hex: "#7a3acd")],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
    )
}

/// Layout constants shared by LogoLockup and any caller that needs to
/// reproduce the brand spacing manually (e.g., a header strip with extra
/// trailing widgets).
public enum BrandLockup {
    /// Horizontal gap between mark and wordmark, expressed as a fraction
    /// of the mark's height. 0.5 means a 24pt mark gets 12pt of gap.
    public static let markToWordSpacingRatio: CGFloat = 0.5

    public enum Compact {
        public static let markSize: CGFloat = 24
        public static let wordSize: CGFloat = 14
    }

    public enum Hero {
        public static let markSize: CGFloat = 46
        public static let wordSize: CGFloat = 22
    }
}
