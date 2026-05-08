// FloatingToolbar.swift
// Glass pill toolbar matching the FloatingToolbar component in scenes-ipad.jsx.
// Positions: top / bottom (horizontally centered) or left (vertically centered).
// Densities: compact / regular / comfy.
// The toolbar can be dragged to any position along its axis.

import SwiftUI

public struct FloatingToolbar: View {
    let position: ToolbarPosition
    let density: ToolbarDensity
    let latencyMs: Double
    var onModeToggle: () -> Void
    var onResolutionToggle: () -> Void
    var onPencilToggle: () -> Void
    var onSettings: () -> Void
    var onDisconnect: () -> Void

    // Drag state
    @State private var dragOffset: CGSize = .zero
    @State private var baseOffset: CGSize = .zero

    public init(
        position: ToolbarPosition,
        density: ToolbarDensity,
        latencyMs: Double,
        onModeToggle: @escaping () -> Void,
        onResolutionToggle: @escaping () -> Void,
        onPencilToggle: @escaping () -> Void,
        onSettings: @escaping () -> Void,
        onDisconnect: @escaping () -> Void
    ) {
        self.position = position
        self.density  = density
        self.latencyMs = latencyMs
        self.onModeToggle = onModeToggle
        self.onResolutionToggle = onResolutionToggle
        self.onPencilToggle = onPencilToggle
        self.onSettings = onSettings
        self.onDisconnect = onDisconnect
    }

    // MARK: Layout constants

    private var isVertical: Bool { position == .left }
    private var btnSize: CGFloat {
        switch density {
        case .compact: return 32
        case .regular: return 38
        case .comfy:   return 44
        }
    }
    private var itemGap: CGFloat {
        switch density {
        case .compact: return 4
        case .regular: return 8
        case .comfy:   return 12
        }
    }
    private var paddingH: CGFloat {
        switch density {
        case .compact: return 8
        case .regular: return 12
        case .comfy:   return 16
        }
    }
    private var paddingV: CGFloat {
        switch density {
        case .compact: return 6
        case .regular: return 9
        case .comfy:   return 12
        }
    }

    // MARK: Body

    public var body: some View {
        GeometryReader { geo in
            toolbar
                .position(anchorPoint(in: geo.size))
                .offset(clampedDragOffset(in: geo.size))
                .gesture(dragGesture(in: geo.size))
        }
    }

    // MARK: Toolbar pill

    @ViewBuilder
    private var toolbar: some View {
        HStack_or_VStack(isVertical: isVertical, spacing: itemGap) {
            // Drag handle
            Capsule()
                .fill(Color.white.opacity(0.25))
                .frame(
                    width:  isVertical ? 22 : 4,
                    height: isVertical ? 4  : 22
                )
                .padding(isVertical ? .vertical : .horizontal, 4)

            // Buttons
            ForEach(toolbarItems, id: \.key) { item in
                ToolbarButton(item: item, size: btnSize) {
                    item.action()
                }
            }
        }
        .padding(.horizontal, isVertical ? paddingV : paddingH)
        .padding(.vertical,   isVertical ? paddingH : paddingV)
        .background(
            Capsule()
                .fill(Color(red: 20/255, green: 20/255, blue: 22/255).opacity(0.60))
                .overlay(
                    Capsule()
                        .strokeBorder(Color.white.opacity(0.12), lineWidth: 1)
                )
                .shadow(color: .black.opacity(0.45), radius: 20, y: 8)
        )
        .overlay(
            Capsule()
                .fill(.ultraThinMaterial)
                .opacity(0.5)
        )
    }

    // MARK: Items

    private var toolbarItems: [ToolbarItem] {[
        ToolbarItem(key: "mode", sysIcon: "rectangle.on.rectangle",   label: "Mode",     isDanger: false, action: onModeToggle),
        ToolbarItem(key: "res",  sysIcon: "display",                   label: "Res",      isDanger: false, action: onResolutionToggle),
        ToolbarItem(key: "lat",  sysIcon: nil, label: latencyLabel,    isDanger: false, action: {}),
        ToolbarItem(key: "pen",  sysIcon: "pencil.tip",                label: "Pencil",   isDanger: false, action: onPencilToggle),
        ToolbarItem(key: "hand", sysIcon: "hand.raised.fill",          label: "Pan",      isDanger: false, action: {}),
        ToolbarItem(key: "gear", sysIcon: "gearshape.fill",            label: "Settings", isDanger: false, action: onSettings),
        ToolbarItem(key: "end",  sysIcon: "power",                     label: "End",      isDanger: true,  action: onDisconnect),
    ]}

    private var latencyLabel: String { "\(Int(latencyMs)) ms" }

    // MARK: Positioning

    private func anchorPoint(in size: CGSize) -> CGPoint {
        switch position {
        case .top:    return CGPoint(x: size.width / 2, y: 38 + 30)
        case .bottom: return CGPoint(x: size.width / 2, y: size.height - 28 - 30)
        case .left:   return CGPoint(x: 16 + 30,        y: size.height / 2)
        }
    }

    private func clampedDragOffset(in size: CGSize) -> CGSize {
        let total = CGSize(
            width:  baseOffset.width  + dragOffset.width,
            height: baseOffset.height + dragOffset.height
        )
        switch position {
        case .top, .bottom:
            // Only horizontal freedom.
            let clamped = max(-size.width / 2 + 60, min(size.width / 2 - 60, total.width))
            return CGSize(width: clamped, height: 0)
        case .left:
            // Only vertical freedom.
            let clamped = max(-size.height / 2 + 60, min(size.height / 2 - 60, total.height))
            return CGSize(width: 0, height: clamped)
        }
    }

    private func dragGesture(in size: CGSize) -> some Gesture {
        DragGesture(minimumDistance: 4)
            .onChanged { value in
                dragOffset = value.translation
            }
            .onEnded { value in
                baseOffset = CGSize(
                    width:  baseOffset.width  + value.translation.width,
                    height: baseOffset.height + value.translation.height
                )
                dragOffset = .zero
            }
    }
}

// MARK: - Sub-components

private struct ToolbarItem: Identifiable {
    let key: String
    let sysIcon: String?
    let label: String
    let isDanger: Bool
    let action: () -> Void
    var id: String { key }
}

private struct ToolbarButton: View {
    let item: ToolbarItem
    let size: CGFloat
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            ZStack {
                Circle()
                    .fill(item.isDanger
                          ? Color(hex: "#ff453a").opacity(0.16)
                          : Color.white.opacity(0.08))
                    .overlay(Circle().strokeBorder(Color.white.opacity(0.08), lineWidth: 1))

                if item.key == "lat" {
                    // Latency dot + label
                    Circle()
                        .fill(Color(hex: "#30d158"))
                        .frame(width: 8, height: 8)
                } else if let icon = item.sysIcon {
                    Image(systemName: icon)
                        .font(.system(size: size * 0.45, weight: .medium))
                        .foregroundStyle(item.isDanger ? Color(hex: "#ff453a") : .white)
                }
            }
            .frame(width: size, height: size)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(item.label)
        // Show latency label below button when not compact
        .overlay(alignment: .bottom) {
            if item.key == "lat" {
                Text(item.label)
                    .font(.system(size: 9))
                    .foregroundStyle(Color(hex: "#30d158"))
                    .offset(y: size / 2 + 6)
                    .fixedSize()
            }
        }
    }
}

// MARK: - HStack / VStack switcher

private struct HStack_or_VStack<Content: View>: View {
    let isVertical: Bool
    let spacing: CGFloat
    @ViewBuilder let content: () -> Content

    var body: some View {
        if isVertical {
            VStack(spacing: spacing) { content() }
        } else {
            HStack(spacing: spacing) { content() }
        }
    }
}

// MARK: - Preview

#Preview {
    ZStack {
        Color.black.ignoresSafeArea()
        FloatingToolbar(
            position: .bottom,
            density: .regular,
            latencyMs: 8,
            onModeToggle: {},
            onResolutionToggle: {},
            onPencilToggle: {},
            onSettings: {},
            onDisconnect: {}
        )
    }
    .preferredColorScheme(.dark)
}
