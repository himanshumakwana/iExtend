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
            panelsLayer
        }
        .frame(width: size, height: size)
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

#Preview {
    VStack(spacing: 32) {
        LogoMark(size: 96, floats: false)
        LogoMark(size: 192, floats: false)
    }
    .padding(40)
    .background(Color(hex: "#0f0f14"))
    .preferredColorScheme(.dark)
}
#endif
