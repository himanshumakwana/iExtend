// Theme.swift
// iPadOS color tokens and typography helpers for iExtend.
// Mirrors the ipadTheme() function in scenes-ipad.jsx exactly.
// Usage:
//   @Environment(\.colorScheme) var cs
//   let t = Theme(dark: cs == .dark)
//   Color(t.accent)

import SwiftUI

// MARK: - Theme

public struct Theme: Sendable {
    // MARK: Surface
    public let bg:       Color
    public let card:     Color
    public let card2:    Color
    public let groupBg:  Color
    public let field:    Color

    // MARK: Text
    public let ink:      Color   // primary label
    public let ink2:     Color   // secondary label
    public let ink3:     Color   // tertiary label

    // MARK: Separator
    public let sep:      Color

    // MARK: Semantic accents
    public let accent:   Color   // system blue  #0a84ff
    public let indigo:   Color   // #5e5ce6
    public let green:    Color   // #30d158
    public let red:      Color   // #ff453a
    public let orange:   Color   // #ff9f0a

    // MARK: - Factory

    public init(dark: Bool) {
        if dark {
            bg       = Color(hex: "#000000")
            card     = Color(hex: "#1c1c1e")
            card2    = Color(hex: "#2c2c2e")
            groupBg  = Color(hex: "#000000")
            field    = Color(hex: "#1c1c1e")
            ink      = Color(hex: "#ffffff")
            ink2     = Color(r: 235, g: 235, b: 245, a: 0.60)
            ink3     = Color(r: 235, g: 235, b: 245, a: 0.30)
            sep      = Color(r: 84,  g: 84,  b: 88,  a: 0.50)
        } else {
            bg       = Color(hex: "#f2f2f7")
            card     = Color(hex: "#ffffff")
            card2    = Color(hex: "#ffffff")
            groupBg  = Color(hex: "#f2f2f7")
            field    = Color(hex: "#ffffff")
            ink      = Color(hex: "#000000")
            ink2     = Color(r: 60, g: 60, b: 67, a: 0.60)
            ink3     = Color(r: 60, g: 60, b: 67, a: 0.30)
            sep      = Color(r: 60, g: 60, b: 67, a: 0.18)
        }
        // Semantic colours are appearance-independent (iOS system style)
        accent = Color(hex: "#0a84ff")
        indigo = Color(hex: "#5e5ce6")
        green  = Color(hex: "#30d158")
        red    = Color(hex: "#ff453a")
        orange = Color(hex: "#ff9f0a")
    }

    // MARK: - Convenience: from environment

    public static func current(colorScheme: ColorScheme) -> Theme {
        Theme(dark: colorScheme == .dark)
    }
}

// MARK: - Environment key

private struct ThemeKey: EnvironmentKey {
    static let defaultValue = Theme(dark: true)
}

public extension EnvironmentValues {
    var theme: Theme {
        get { self[ThemeKey.self] }
        set { self[ThemeKey.self] = newValue }
    }
}

// MARK: - View modifier

public extension View {
    func applyTheme(_ theme: Theme) -> some View {
        environment(\.theme, theme)
    }
}

// MARK: - Color init helpers

extension Color {
    /// Init from a hex string like "#0a84ff" or "0a84ff".
    init(hex: String) {
        let hex = hex.trimmingCharacters(in: CharacterSet(charactersIn: "#"))
        var int: UInt64 = 0
        Scanner(string: hex).scanHexInt64(&int)
        let r = Double((int >> 16) & 0xff) / 255
        let g = Double((int >> 8)  & 0xff) / 255
        let b = Double((int)       & 0xff) / 255
        self.init(red: r, green: g, blue: b)
    }

    /// Init from RGBA components where a is 0–1.
    init(r: Double, g: Double, b: Double, a: Double) {
        self.init(red: r / 255, green: g / 255, blue: b / 255, opacity: a)
    }
}

// MARK: - Typography helpers

public extension Font {
    /// SF Pro Display — titles and headlines.
    static func display(_ size: CGFloat, weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight, design: .default)
    }

    /// SF Pro Text — body copy.
    static func body(_ size: CGFloat, weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight)
    }

    /// Monospaced — latency readouts, pin digits.
    static func mono(_ size: CGFloat, weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight, design: .monospaced)
    }
}

// MARK: - Gradient helpers

public extension LinearGradient {
    /// Brand gradient used in the Welcome wordmark.
    static let brandGradient = LinearGradient(
        colors: [Color(hex: "#0a84ff"), Color(hex: "#5e5ce6"), Color(hex: "#ff9f0a")],
        startPoint: .leading,
        endPoint: .trailing
    )
}

// MARK: - Shape helpers

public extension ShapeStyle where Self == Material {
    /// Glass-effect background used in the FloatingToolbar.
    static var glass: Material { .ultraThinMaterial }
}

// MARK: - Signal strength

/// Returns 1…4 signal bars appropriate for a given ms RTT.
public func signalBars(rttMs: Double) -> Int {
    switch rttMs {
    case ..<10:  return 4
    case ..<20:  return 3
    case ..<40:  return 2
    default:     return 1
    }
}

/// Color for a given ms RTT.
public func latencyColor(_ rttMs: Double, dark: Bool) -> Color {
    let t = Theme(dark: dark)
    switch rttMs {
    case ..<20:  return t.green
    case ..<50:  return t.orange
    default:     return t.red
    }
}
