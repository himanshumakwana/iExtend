// LogoGeometry.swift
// Pure-data constants for the iExtend Origami Fold mark. Single source of
// truth for panel polygon coordinates and the canonical 160×160 viewBox.
// Both LogoMark.swift (SwiftUI) and scripts/app-icon-source.svg consume
// these numbers — keep them in sync if the geometry ever changes.
//
// Coordinate space: 160×160 square viewBox. Origin top-left, y-down,
// matching SVG and SwiftUI conventions.

import CoreGraphics

public enum LogoGeometry {
    /// The square viewBox the mark is drawn in.
    public static let viewBoxSize: CGFloat = 160

    /// Vertices for one fold panel, in draw order (clockwise from top-left).
    public struct Panel: Equatable {
        public let name: String
        public let vertices: [CGPoint]   // exactly 4
    }

    /// The three panels, ordered left → right.
    public static let panels: [Panel] = [
        Panel(name: "blue",   vertices: [CGPoint(x: 34, y: 42), CGPoint(x: 88,  y: 30), CGPoint(x: 96,  y: 118), CGPoint(x: 38,  y: 108)]),
        Panel(name: "indigo", vertices: [CGPoint(x: 88, y: 30), CGPoint(x: 122, y: 52), CGPoint(x: 122, y: 128), CGPoint(x: 96,  y: 118)]),
        Panel(name: "purple", vertices: [CGPoint(x: 122, y: 52), CGPoint(x: 138, y: 76), CGPoint(x: 134, y: 134), CGPoint(x: 122, y: 128)]),
    ]

    /// Crease lines between adjacent panels (start, end), drawn at 0.6pt
    /// in white at 50–70% opacity.
    public static let creases: [(start: CGPoint, end: CGPoint)] = [
        (CGPoint(x: 88,  y: 30), CGPoint(x: 96,  y: 118)),  // blue ↔ indigo
        (CGPoint(x: 122, y: 52), CGPoint(x: 122, y: 128)),  // indigo ↔ purple
    ]
}
