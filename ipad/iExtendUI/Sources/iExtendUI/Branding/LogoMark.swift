// LogoMark.swift
// SwiftUI implementation of the iExtend Origami Fold mark. Consumes
// LogoGeometry for panel coordinates and LinearGradient.brandFold* for
// fills. The `floats` flag toggles the floor shadow + ambient glow
// (added in Task 5 of the implementation plan).

#if canImport(UIKit)
import SwiftUI

public struct LogoMark: View {
    @Environment(\.colorScheme) private var cs

    public let size: CGFloat
    public let floats: Bool

    public init(size: CGFloat = 64, floats: Bool = true) {
        self.size = size
        self.floats = floats
    }

    public var body: some View {
        ZStack {
            if floats {
                ambientGlow
                panelsLayer
                    .shadow(
                        color: Color.black.opacity(cs == .dark ? 0.55 : 0.33),
                        radius: 12,
                        x: 0,
                        y: 8
                    )
            } else {
                panelsLayer
            }
        }
        .frame(width: size, height: size)
    }

    // Indigo halo behind the panels. Sized roughly 80×50pt at the canonical
    // 160pt mark size; scales with `size`.
    private var ambientGlow: some View {
        let glowOpacity = cs == .dark ? 0.6 : 0.3
        return RadialGradient(
            colors: [
                Color(hex: "#5e5ce6").opacity(glowOpacity),
                Color(hex: "#5e5ce6").opacity(0)
            ],
            center: .center,
            startRadius: 0,
            endRadius: size * 0.5
        )
        .frame(width: size * 1.0, height: size * 0.625)
        .blur(radius: 8)
    }

    // MARK: Panels + sheen + creases (drawn in this z-order)

    private var panelsLayer: some View {
        GeometryReader { geo in
            let scale = geo.size.width / LogoGeometry.viewBoxSize
            ZStack {
                // Three gradient-filled panels
                panelPath(LogoGeometry.panels[0], scale: scale)
                    .fill(LinearGradient.brandFoldBlue)
                panelPath(LogoGeometry.panels[1], scale: scale)
                    .fill(LinearGradient.brandFoldIndigo)
                panelPath(LogoGeometry.panels[2], scale: scale)
                    .fill(LinearGradient.brandFoldPurple)

                // White top-down sheen, panel 1 only (18% → 0%)
                panelPath(LogoGeometry.panels[0], scale: scale)
                    .fill(LinearGradient(
                        colors: [Color.white.opacity(0.18), Color.white.opacity(0)],
                        startPoint: .top,
                        endPoint: .bottom
                    ))

                // Crease lines on top
                ForEach(Array(LogoGeometry.creases.enumerated()), id: \.offset) { index, crease in
                    Path { path in
                        path.move(to: CGPoint(x: crease.start.x * scale, y: crease.start.y * scale))
                        path.addLine(to: CGPoint(x: crease.end.x * scale, y: crease.end.y * scale))
                    }
                    .stroke(Color.white.opacity(index == 0 ? 0.6 : 0.5), lineWidth: 0.6)
                }
            }
        }
    }

    private func panelPath(_ panel: LogoGeometry.Panel, scale: CGFloat) -> Path {
        var p = Path()
        let v = panel.vertices
        p.move(to: CGPoint(x: v[0].x * scale, y: v[0].y * scale))
        p.addLine(to: CGPoint(x: v[1].x * scale, y: v[1].y * scale))
        p.addLine(to: CGPoint(x: v[2].x * scale, y: v[2].y * scale))
        p.addLine(to: CGPoint(x: v[3].x * scale, y: v[3].y * scale))
        p.closeSubpath()
        return p
    }
}

#Preview("Dark — floating vs flat") {
    HStack(spacing: 32) {
        VStack(spacing: 8) {
            LogoMark(size: 128, floats: true)
            Text("floats: true").font(.caption).foregroundStyle(.white)
        }
        VStack(spacing: 8) {
            LogoMark(size: 128, floats: false)
            Text("floats: false").font(.caption).foregroundStyle(.white)
        }
    }
    .padding(40)
    .background(Color(hex: "#0f0f14"))
    .preferredColorScheme(.dark)
}

#Preview("Light — floating") {
    LogoMark(size: 128, floats: true)
        .padding(40)
        .background(Color(hex: "#f2f2f7"))
        .preferredColorScheme(.light)
}
#endif
