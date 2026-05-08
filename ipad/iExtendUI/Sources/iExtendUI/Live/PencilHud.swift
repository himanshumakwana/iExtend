// PencilHud.swift — a zero-latency SwiftUI overlay that draws the pencil-tip
// indicator at the locally-reprojected position.
//
// Plan 8 Task 13.
//
// DESIGN
// ======
// The host is told not to render its own cursor tip (via a control message
// `SetHostCursorVisible { tip: false }`) when the live screen is front.
// Instead, this view renders a smooth circle whose position is updated at
// UIKit cadence (~1 ms input-to-overlay update) rather than video-frame
// cadence (~14 ms round-trip).
//
// The circle radius scales with pressure: 8 pt at zero, 24 pt at full.
//
// USAGE
// =====
// In LiveScreenView:
//   ```swift
//   ZStack {
//       MetalLayerView(renderer: renderer)
//       PencilHud(model: hudModel)
//   }
//   ```
// Update the model from EventCapture.onPacket:
//   ```swift
//   cap.onPacket = { pkt in
//       if pkt.kind == .pencilMove || pkt.kind == .pencilBegin {
//           let pl = pkt.decodedPencil()
//           hudModel.position = Reproject.predict(...)
//           hudModel.pressure = pl.pressure
//           hudModel.visible  = true
//       } else if pkt.kind == .pencilEnd {
//           hudModel.visible  = false
//       }
//   }
//   ```

import SwiftUI

// ── HUD Model ─────────────────────────────────────────────────────────────────

/// Observable state that drives PencilHud position and appearance.
@Observable
public final class PencilHudModel {
    /// Current reprojected pencil-tip position in the coordinate space of the
    /// enclosing view (display pixels from top-left).
    public var position:  CGPoint = .zero

    /// Pressure 0..=1; drives circle radius.
    public var pressure:  CGFloat = 0

    /// Whether the HUD is visible (tip is in contact or hovering).
    public var visible:   Bool    = false

    /// Whether this is a hover (not contact) — draws a lighter ring.
    public var isHover:   Bool    = false

    public init() {}
}

// ── PencilHud ─────────────────────────────────────────────────────────────────

/// A SwiftUI view that renders a pressure-sensitive pencil-tip dot.
///
/// Place inside a ZStack above the Metal layer.  `allowsHitTesting(false)`
/// ensures the overlay never intercepts touches.
public struct PencilHud: View {

    public var model: PencilHudModel

    public init(model: PencilHudModel) {
        self.model = model
    }

    // Diameter: linearly interpolated from 8 pt (pressure 0) to 24 pt (pressure 1).
    private var diameter: CGFloat {
        let minD: CGFloat = 8
        let maxD: CGFloat = 24
        return minD + (maxD - minD) * model.pressure
    }

    public var body: some View {
        ZStack {
            if model.isHover {
                // Hover: open ring
                Circle()
                    .stroke(Color.white.opacity(0.70), lineWidth: 1.5)
                    .frame(width: diameter + 4, height: diameter + 4)
                    .blur(radius: 0.5)
                    .position(model.position)
            } else {
                // Contact: filled disc with drop shadow
                Circle()
                    .fill(
                        RadialGradient(
                            colors: [Color.white.opacity(0.95), Color.white.opacity(0.60)],
                            center: .center,
                            startRadius: 0,
                            endRadius: diameter / 2
                        )
                    )
                    .frame(width: diameter, height: diameter)
                    .shadow(color: .black.opacity(0.30), radius: 2, x: 0, y: 1)
                    .blur(radius: 0.8)
                    .position(model.position)
            }
        }
        .opacity(model.visible ? 1 : 0)
        .animation(.linear(duration: 0.015), value: model.position)
        .animation(.easeOut(duration: 0.08), value: model.visible)
        .allowsHitTesting(false)
    }
}

// ── Integrated LiveScreenView plumbing (excerpt) ──────────────────────────────

/// Wraps PencilHud with the cursor overlay driven by MetalRenderer.
///
/// This struct is a self-contained fragment showing how PencilHud slots into
/// the live screen view.  The full LiveScreenView is in
/// iExtendUI/Sources/iExtendUI/Live/LiveScreenView.swift.
public struct LiveScreenOverlay: View {
    public var hudModel: PencilHudModel
    public var cursorPos: CGPoint

    public init(hudModel: PencilHudModel, cursorPos: CGPoint) {
        self.hudModel  = hudModel
        self.cursorPos = cursorPos
    }

    public var body: some View {
        ZStack {
            // Pencil tip HUD (see PencilHud above)
            PencilHud(model: hudModel)

            // Host cursor sprite (arrow / I-beam / etc.)
            // Rendered here as a simple circle; Plan 9 replaces with the real
            // cursor image downloaded from the host via the control channel.
            Circle()
                .fill(Color.white.opacity(0.85))
                .frame(width: 12, height: 12)
                .shadow(color: .black.opacity(0.4), radius: 2)
                .position(cursorPos)
                .allowsHitTesting(false)
        }
    }
}

// ── Preview ───────────────────────────────────────────────────────────────────

#if DEBUG
#Preview("PencilHud — contact") {
    let m = PencilHudModel()
    m.position = CGPoint(x: 200, y: 200)
    m.pressure = 0.7
    m.visible  = true
    return ZStack {
        Color.black.ignoresSafeArea()
        PencilHud(model: m)
    }
}

#Preview("PencilHud — hover") {
    let m = PencilHudModel()
    m.position = CGPoint(x: 200, y: 200)
    m.pressure = 0
    m.visible  = true
    m.isHover  = true
    return ZStack {
        Color.black.ignoresSafeArea()
        PencilHud(model: m)
    }
}
#endif
