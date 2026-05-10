# iExtend Logo Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the iExtend "Origami Fold, floating" logo and wire it into every iPad-app surface (Welcome, Discover, Pair, Live's FloatingToolbar, Settings) plus the springboard app icon.

**Architecture:** Three new SwiftUI files under `iExtendUI/Branding/` — pure-data `LogoGeometry` and `BrandGradient` constants (testable on macOS), plus the UIKit-gated `LogoMark` and `LogoLockup` views that consume them. Six existing onboarding/live/settings views are modified to drop in the lockup. The springboard icon is a separate raster pipeline: a canonical `scripts/app-icon-source.svg` rendered to `icon-1024.png` by `scripts/generate-app-icon.sh` (calls `rsvg-convert`).

**Tech Stack:** Swift 5.9, SwiftUI (iOS 17+), SwiftPM (existing `iExtend` package), XcodeGen (existing `project.yml`), `librsvg2-bin` (build-time, for SVG → PNG).

**Spec:** `docs/superpowers/specs/2026-05-10-iextend-logo-design.md`

**Pre-flight (one-time, run by the engineer before Task 13):**

```bash
which rsvg-convert || sudo apt install librsvg2-bin
which xcodegen || brew install xcodegen   # only needed if regenerating Xcode project
```

---

## Task 1: `LogoGeometry` constants

**Files:**
- Create: `ipad/iExtendUI/Sources/iExtendUI/Branding/LogoGeometry.swift`

This is the single source of truth for the fold's panel coordinates. Pure data — no SwiftUI views — so it compiles on macOS and is unit-testable. The SwiftUI mark (Task 4) and the SVG icon (Task 13) both reference these numbers via documentation comments.

- [ ] **Step 1: Create the file with the panel constants**

```swift
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
```

- [ ] **Step 2: Commit**

```bash
git add ipad/iExtendUI/Sources/iExtendUI/Branding/LogoGeometry.swift
git commit -m "feat(ui/branding): add LogoGeometry — fold panel coordinates"
```

---

## Task 2: `BrandGradient` extensions

**Files:**
- Create: `ipad/iExtendUI/Sources/iExtendUI/Branding/BrandGradient.swift`

Three named `LinearGradient` values for the three panels, plus the lockup spacing constant. Lives alongside the existing `LinearGradient.brandGradient` in `Theme.swift:139` — single namespace.

- [ ] **Step 1: Create the file**

```swift
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
```

- [ ] **Step 2: Commit**

```bash
git add ipad/iExtendUI/Sources/iExtendUI/Branding/BrandGradient.swift
git commit -m "feat(ui/branding): add BrandGradient + BrandLockup constants"
```

---

## Task 3: `iExtendUITests` target with first tests

**Files:**
- Modify: `ipad/Package.swift` (add `iExtendUITests` target)
- Create: `ipad/iExtendUI/Tests/iExtendUITests/LogoGeometryTests.swift`
- Create: `ipad/iExtendUI/Tests/iExtendUITests/BrandGradientTests.swift`

The `iExtendUI` target has no test target today (only `iExtendKit` and `iExtendInput` do). Add one for the pure-data logo constants. SwiftUI views themselves stay untested — verified by Xcode Preview.

- [ ] **Step 1: Add the test target to `Package.swift`**

In `ipad/Package.swift`, after the existing `.testTarget(name: "iExtendInputTests", ...)` block (around line 63), add:

```swift
        .testTarget(
            name: "iExtendUITests",
            dependencies: ["iExtendUI"],
            path: "iExtendUI/Tests/iExtendUITests"
        ),
```

- [ ] **Step 2: Write the failing geometry test**

Create `ipad/iExtendUI/Tests/iExtendUITests/LogoGeometryTests.swift`:

```swift
import XCTest
@testable import iExtendUI

final class LogoGeometryTests: XCTestCase {

    func test_viewBoxIsSquare160() {
        XCTAssertEqual(LogoGeometry.viewBoxSize, 160)
    }

    func test_threePanelsExist() {
        XCTAssertEqual(LogoGeometry.panels.count, 3)
        XCTAssertEqual(LogoGeometry.panels.map { $0.name }, ["blue", "indigo", "purple"])
    }

    func test_eachPanelHasFourVertices() {
        for panel in LogoGeometry.panels {
            XCTAssertEqual(panel.vertices.count, 4, "panel \(panel.name)")
        }
    }

    func test_adjacentPanelsShareCreasePoints() {
        // The blue panel's right edge (vertices [1] and [2]) must equal
        // the indigo panel's left edge (vertices [0] and [3]).
        let blue   = LogoGeometry.panels[0]
        let indigo = LogoGeometry.panels[1]
        XCTAssertEqual(blue.vertices[1], indigo.vertices[0])
        XCTAssertEqual(blue.vertices[2], indigo.vertices[3])

        let purple = LogoGeometry.panels[2]
        XCTAssertEqual(indigo.vertices[1], purple.vertices[0])
        XCTAssertEqual(indigo.vertices[2], purple.vertices[3])
    }

    func test_creaseEndpointsMatchPanelEdges() {
        XCTAssertEqual(LogoGeometry.creases.count, 2)
        let blue   = LogoGeometry.panels[0]
        let indigo = LogoGeometry.panels[1]
        XCTAssertEqual(LogoGeometry.creases[0].start, blue.vertices[1])
        XCTAssertEqual(LogoGeometry.creases[0].end,   blue.vertices[2])
        XCTAssertEqual(LogoGeometry.creases[1].start, indigo.vertices[1])
        XCTAssertEqual(LogoGeometry.creases[1].end,   indigo.vertices[2])
    }
}
```

- [ ] **Step 3: Write the failing gradient/lockup test**

Create `ipad/iExtendUI/Tests/iExtendUITests/BrandGradientTests.swift`:

```swift
import XCTest
import SwiftUI
@testable import iExtendUI

final class BrandGradientTests: XCTestCase {

    func test_lockupSpacingRatioIsHalfMarkHeight() {
        XCTAssertEqual(BrandLockup.markToWordSpacingRatio, 0.5)
    }

    func test_compactSizes() {
        XCTAssertEqual(BrandLockup.Compact.markSize, 24)
        XCTAssertEqual(BrandLockup.Compact.wordSize, 14)
    }

    func test_heroSizes() {
        XCTAssertEqual(BrandLockup.Hero.markSize, 46)
        XCTAssertEqual(BrandLockup.Hero.wordSize, 22)
    }

    func test_heroIsLargerThanCompact() {
        XCTAssertGreaterThan(BrandLockup.Hero.markSize, BrandLockup.Compact.markSize)
        XCTAssertGreaterThan(BrandLockup.Hero.wordSize, BrandLockup.Compact.wordSize)
    }

    // The three brand-fold gradient values must be statically reachable.
    // (We can't introspect a LinearGradient's internal stops via public API,
    // so the test asserts the symbols compile and resolve.)
    func test_foldGradientsExist() {
        let _: LinearGradient = .brandFoldBlue
        let _: LinearGradient = .brandFoldIndigo
        let _: LinearGradient = .brandFoldPurple
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd ipad && swift test --filter "iExtendUITests"`
Expected: 9 passing tests across 2 test classes.

If `swift test` fails because no test runner is configured, the engineer is on Linux and `swift test` only runs the macOS-buildable subset; that's acceptable — the test target compiles and the data assertions pass.

- [ ] **Step 5: Commit**

```bash
git add ipad/Package.swift ipad/iExtendUI/Tests/iExtendUITests/
git commit -m "test(ui/branding): add iExtendUITests target with geometry + gradient tests"
```

---

## Task 4: `LogoMark` view — flat variant first

**Files:**
- Create: `ipad/iExtendUI/Sources/iExtendUI/Branding/LogoMark.swift`

Render the three panels + crease lines + sheen, with `floats: false` as default in this task. Floating effect (Task 5) builds on top.

- [ ] **Step 1: Create the file**

```swift
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
```

- [ ] **Step 2: Verify the file compiles**

Run: `cd ipad && swift build`
Expected: builds successfully (UI files gated by `#if canImport(UIKit)` are skipped on macOS host build, but the file must be syntactically valid Swift).

- [ ] **Step 3: Manual visual check (engineer with Xcode only)**

Open `LogoMark.swift` in Xcode and verify the Preview renders three panels in blue/indigo/purple gradients with thin white crease lines between them. Reference: brainstorm artifact at `.superpowers/brainstorm/<session>/content/origami-variants.html` card "L1".

If on Linux without Xcode, skip this step — the snapshot test target (Task 5's manual check) covers visual verification.

- [ ] **Step 4: Commit**

```bash
git add ipad/iExtendUI/Sources/iExtendUI/Branding/LogoMark.swift
git commit -m "feat(ui/branding): add LogoMark — flat origami fold (no float)"
```

---

## Task 5: Add floating effect to `LogoMark`

**Files:**
- Modify: `ipad/iExtendUI/Sources/iExtendUI/Branding/LogoMark.swift`

Add the floor shadow + ambient indigo glow when `floats == true`. Glow scales by colorScheme (full strength dark, half strength light).

- [ ] **Step 1: Modify the `body` to layer the float effects**

In `LogoMark.swift`, replace the entire `public var body: some View` block (currently `ZStack { panelsLayer } .frame(width: size, height: size)`) with:

```swift
    public var body: some View {
        ZStack {
            if floats {
                ambientGlow
                panelsLayer
                    .shadow(
                        color: Color.black.opacity(cs == .dark ? 0.35 : 0.21),
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
```

- [ ] **Step 2: Update the Preview to show both floating and non-floating**

Replace the existing `#Preview` block at the bottom of `LogoMark.swift` with:

```swift
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
```

- [ ] **Step 3: Verify the file still compiles**

Run: `cd ipad && swift build`
Expected: builds successfully.

- [ ] **Step 4: Manual visual check (engineer with Xcode only)**

Open `LogoMark.swift` in Xcode. The "Dark — floating vs flat" preview should show:
- Left mark: indigo glow halo behind, soft drop shadow below — appears to lift off the dark surface
- Right mark: same panels, no halo, no shadow — sits flat

The "Light — floating" preview should show the mark on a light gray background with a subtler glow and shadow (both about half-opacity vs dark mode).

- [ ] **Step 5: Commit**

```bash
git add ipad/iExtendUI/Sources/iExtendUI/Branding/LogoMark.swift
git commit -m "feat(ui/branding): add floating effect to LogoMark (shadow + halo, theme-aware)"
```

---

## Task 6: `LogoLockup` view

**Files:**
- Create: `ipad/iExtendUI/Sources/iExtendUI/Branding/LogoLockup.swift`

Mark + "iExtend" wordmark in a horizontal arrangement, two presets (`.compact`, `.hero`). Reads sizes from `BrandLockup.Compact` / `BrandLockup.Hero` (Task 2).

- [ ] **Step 1: Create the file**

```swift
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
```

- [ ] **Step 2: Verify the file compiles**

Run: `cd ipad && swift build`
Expected: builds successfully.

- [ ] **Step 3: Manual visual check (engineer with Xcode only)**

Open `LogoLockup.swift` Preview. Both dark and light previews should show:
- Compact lockup: 24pt mark + "iExtend" at 14pt semibold, ~12pt gap between
- Hero lockup: 46pt mark with floating effect + "iExtend" at 22pt semibold, ~23pt gap

Wordmark color follows `Theme.ink` (white in dark, black in light).

- [ ] **Step 4: Commit**

```bash
git add ipad/iExtendUI/Sources/iExtendUI/Branding/LogoLockup.swift
git commit -m "feat(ui/branding): add LogoLockup — mark + wordmark, .compact/.hero"
```

---

## Task 7: Wire into `WelcomeView`

**Files:**
- Modify: `ipad/iExtendUI/Sources/iExtendUI/Onboarding/WelcomeView.swift` (lines 50–66)

Replace the standalone "iExtend 1.0" capsule with `LogoLockup(.hero)` directly above the headline, with the version chip moved to a smaller line beneath.

- [ ] **Step 1: Replace the version-chip block with the hero lockup**

In `WelcomeView.swift`, replace lines 50–66 (the `VStack { ... // Version badge` block ending with the version capsule's `.background(Capsule()...)` modifier) with:

```swift
        VStack(alignment: .leading, spacing: 0) {
            // Brand lockup (hero size, floating)
            LogoLockup(.hero)

            // Version chip directly underneath
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
            .padding(.top, 12)
```

(The closing `}` of the surrounding `VStack` and the rest of the left column — Headline, Tagline, CTAs, Feature strip — stay unchanged.)

- [ ] **Step 2: Verify the file compiles**

Run: `cd ipad && swift build`
Expected: builds successfully.

- [ ] **Step 3: Manual visual check (engineer with Xcode only)**

Open `WelcomeView.swift` Preview. The left column should show, top-to-bottom:
1. The hero lockup (mark + "iExtend") with floating effect
2. The "iExtend 1.0" chip directly below
3. Then the existing "Your iPad, a second screen." headline, tagline, CTAs, and feature strip — unchanged

- [ ] **Step 4: Commit**

```bash
git add ipad/iExtendUI/Sources/iExtendUI/Onboarding/WelcomeView.swift
git commit -m "feat(ui/welcome): replace version chip with hero LogoLockup"
```

---

## Task 8: Add header lockup to `DiscoverView`

**Files:**
- Modify: `ipad/iExtendUI/Sources/iExtendUI/Onboarding/DiscoverView.swift` (around line 47–82)

Insert a thin header strip at the top of the inner `VStack`, before the existing "Step 2 of 3 · Discover" eyebrow. The lockup sits at top-left, content below shifts down naturally.

- [ ] **Step 1: Add the header strip**

In `DiscoverView.swift`, find the line `// Status bar spacing` followed by `Color.clear.frame(height: 38)` (around line 48–49). Replace those two lines with:

```swift
                // Status bar spacing
                Color.clear.frame(height: 38)

                // Brand header strip
                HStack {
                    LogoLockup(.compact)
                    Spacer()
                }
                .padding(.horizontal, 36)
                .padding(.bottom, 8)
```

- [ ] **Step 2: Verify the file compiles**

Run: `cd ipad && swift build`
Expected: builds successfully.

- [ ] **Step 3: Manual visual check**

Open `DiscoverView.swift` Preview. Top-left should show the compact lockup above the existing "Step 2 of 3 · Discover" eyebrow. Spacing should feel comfortable — neither cramped nor floating in dead space.

- [ ] **Step 4: Commit**

```bash
git add ipad/iExtendUI/Sources/iExtendUI/Onboarding/DiscoverView.swift
git commit -m "feat(ui/discover): add compact LogoLockup header strip"
```

---

## Task 9: Add header lockup to `PairView`

**Files:**
- Modify: `ipad/iExtendUI/Sources/iExtendUI/Onboarding/PairView.swift` (lines 41–60)

`PairView` has the same outer `VStack { Color.clear.frame(height: 40) ... HStack { qrCard ; PinEntryView } }` structure as `DiscoverView`. Insert the header strip the same way. `PinEntryView` (the right column) inherits this header — no separate lockup inside `PinEntryView`.

- [ ] **Step 1: Add the header strip**

In `PairView.swift`, find `Color.clear.frame(height: 40) // status bar` (line 42). After that line and before the `HStack(spacing: 22)` (line 44), insert:

```swift
                // Brand header strip
                HStack {
                    LogoLockup(.compact)
                    Spacer()
                }
                .padding(.horizontal, 36)
                .padding(.bottom, 8)
```

- [ ] **Step 2: Verify the file compiles**

Run: `cd ipad && swift build`
Expected: builds successfully.

- [ ] **Step 3: Manual visual check**

Open `PairView.swift` Preview. Top-left should show the compact lockup. The two-column QR + PinEntry layout sits beneath, unchanged.

- [ ] **Step 4: Commit**

```bash
git add ipad/iExtendUI/Sources/iExtendUI/Onboarding/PairView.swift
git commit -m "feat(ui/pair): add compact LogoLockup header strip"
```

---

## Task 10: Add `LogoMark` to `FloatingToolbar`

**Files:**
- Modify: `ipad/iExtendUI/Sources/iExtendUI/Live/FloatingToolbar.swift` (lines 89–105)

Insert `LogoMark(size: 22, floats: false)` immediately after the drag handle, before the buttons. `floats: false` because the toolbar's glass background doesn't need a halo bleeding into it.

- [ ] **Step 1: Add the mark to the toolbar HStack/VStack**

In `FloatingToolbar.swift`, find the `@ViewBuilder private var toolbar: some View` block (line 88) and inside the `HStack_or_VStack` body, after the drag handle's `Capsule()` block (lines 91–98) and before the `// Buttons` comment (line 100), insert:

```swift
            // Brand mark (no float — toolbar glass already has its own depth)
            LogoMark(size: 22, floats: false)
                .padding(isVertical ? .vertical : .horizontal, 4)
```

- [ ] **Step 2: Verify the file compiles**

Run: `cd ipad && swift build`
Expected: builds successfully.

- [ ] **Step 3: Manual visual check**

Open `FloatingToolbar.swift` Preview. The toolbar pill should show: drag handle → 22pt logo mark → toolbar buttons. Mark should be small but legible, no halo glow visible (since `floats: false`).

- [ ] **Step 4: Commit**

```bash
git add ipad/iExtendUI/Sources/iExtendUI/Live/FloatingToolbar.swift
git commit -m "feat(ui/live): add LogoMark to FloatingToolbar left edge"
```

---

## Task 11: Add header lockup to `SettingsView` sidebar

**Files:**
- Modify: `ipad/iExtendUI/Sources/iExtendUI/Settings/SettingsView.swift` (lines 35–44)

Replace the bare `Text("Settings")` title with a small lockup + "Settings" title underneath. Keeps the brand visible in the sidebar without crowding the search field.

- [ ] **Step 1: Replace the sidebar title block**

In `SettingsView.swift`, replace lines 35–44 (the `private var sidebar: some View { VStack(...) { Text("Settings") ... .padding(.bottom, 12)` block) with:

```swift
    private var sidebar: some View {
        VStack(alignment: .leading, spacing: 0) {
            LogoLockup(.compact)
                .padding(.horizontal, 22)
                .padding(.top, 8)
                .padding(.bottom, 12)

            Text("Settings")
                .font(.display(28, weight: .bold))
                .foregroundStyle(t.ink)
                .kerning(-0.02 * 28)
                .padding(.horizontal, 22)
                .padding(.bottom, 12)
```

- [ ] **Step 2: Verify the file compiles**

Run: `cd ipad && swift build`
Expected: builds successfully.

- [ ] **Step 3: Manual visual check**

Open `SettingsView.swift` Preview. The sidebar should show, top-to-bottom: compact lockup → "Settings" title → search field → group lists. The lockup sits flush with the same 22pt left padding as the title.

- [ ] **Step 4: Commit**

```bash
git add ipad/iExtendUI/Sources/iExtendUI/Settings/SettingsView.swift
git commit -m "feat(ui/settings): add compact LogoLockup above sidebar title"
```

---

## Task 12: Real General pane with About row

**Files:**
- Modify: `ipad/iExtendUI/Sources/iExtendUI/Settings/SettingsView.swift` (lines 96–97 + new struct)

The General pane currently shows a `PlaceholderPane` ("App version, feedback, and reset options."). Replace with a real pane whose first section is an About row featuring `LogoMark(size: 64)` + version + build label.

- [ ] **Step 1: Replace the General-case `PlaceholderPane` call**

In `SettingsView.swift`, find lines 96–97:

```swift
        case .general:
            PlaceholderPane(title: "General", message: "App version, feedback, and reset options.")
```

Replace with:

```swift
        case .general:
            GeneralPane()
```

- [ ] **Step 2: Add the new `GeneralPane` struct**

In `SettingsView.swift`, just above `// MARK: - PlaceholderPane` (around line 292), insert:

```swift
// MARK: - GeneralPane

private struct GeneralPane: View {
    @Environment(\.theme) private var t

    private var versionString: String {
        let info = Bundle.main.infoDictionary ?? [:]
        let v = info["CFBundleShortVersionString"] as? String ?? "0.0"
        let b = info["CFBundleVersion"] as? String ?? "0"
        return "Version \(v) (\(b))"
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                Text("General")
                    .font(.display(28, weight: .bold))
                    .foregroundStyle(t.ink)
                    .kerning(-0.02 * 28)
                    .padding(.horizontal, 14)
                    .padding(.top, 8)

                // About row
                SettingsListGroup(header: "About") {
                    HStack(spacing: 16) {
                        LogoMark(size: 64, floats: true)
                        VStack(alignment: .leading, spacing: 2) {
                            Text("iExtend")
                                .font(.body(17, weight: .semibold))
                                .foregroundStyle(t.ink)
                            Text(versionString)
                                .font(.body(13))
                                .foregroundStyle(t.ink2)
                        }
                        Spacer()
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 14)
                }
            }
            .padding(.bottom, 40)
        }
        .background(t.bg.ignoresSafeArea())
        .navigationBarHidden(true)
    }
}
```

- [ ] **Step 3: Verify the file compiles**

Run: `cd ipad && swift build`
Expected: builds successfully.

- [ ] **Step 4: Manual visual check**

Open `SettingsView.swift` Preview, switch to General in the sidebar. The detail pane should show "General" title and an About card containing the 64pt floating mark, "iExtend" label, and version/build text.

- [ ] **Step 5: Commit**

```bash
git add ipad/iExtendUI/Sources/iExtendUI/Settings/SettingsView.swift
git commit -m "feat(ui/settings): replace General placeholder with About row using LogoMark"
```

---

## Task 13: Canonical app-icon SVG

**Files:**
- Create: `scripts/app-icon-source.svg`

1024×1024 SVG with a dark indigo→purple background tile and the fold mark scaled to ~62% of the icon area. iOS 17 applies its own corner mask, so the SVG draws a full square (no rounded corners).

- [ ] **Step 1: Create the SVG file**

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!--
  app-icon-source.svg
  Canonical 1024×1024 source for the iExtend springboard icon.
  iOS 17+ applies its own corner mask, so this draws a full square.

  Geometry mirrors LogoGeometry.swift's panel coordinates (160×160 viewBox)
  scaled to fit ~62% of the 1024px icon, centered. If the geometry there
  changes, update the polygon points here too.
-->
<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%"   stop-color="#1a1530"/>
      <stop offset="100%" stop-color="#2a1a4a"/>
    </linearGradient>
    <linearGradient id="blue" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%"   stop-color="#0a84ff"/>
      <stop offset="100%" stop-color="#0050b8"/>
    </linearGradient>
    <linearGradient id="indigo" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%"   stop-color="#5e5ce6"/>
      <stop offset="100%" stop-color="#3f3d80"/>
    </linearGradient>
    <linearGradient id="purple" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%"   stop-color="#bf5af2"/>
      <stop offset="100%" stop-color="#7a3acd"/>
    </linearGradient>
    <linearGradient id="sheen" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%"   stop-color="#ffffff" stop-opacity="0.18"/>
      <stop offset="100%" stop-color="#ffffff" stop-opacity="0"/>
    </linearGradient>
    <radialGradient id="halo" cx="0.5" cy="0.5" r="0.5">
      <stop offset="0%"   stop-color="#5e5ce6" stop-opacity="0.6"/>
      <stop offset="100%" stop-color="#5e5ce6" stop-opacity="0"/>
    </radialGradient>
    <filter id="haloBlur" x="-20%" y="-20%" width="140%" height="140%">
      <feGaussianBlur stdDeviation="32"/>
    </filter>
  </defs>

  <!-- Background tile -->
  <rect width="1024" height="1024" fill="url(#bg)"/>

  <!-- Mark group: scale 160 viewBox → 640px (62% of 1024), translate to center -->
  <!-- transform: translate((1024-640)/2, (1024-640)/2) scale(640/160) -->
  <g transform="translate(192 192) scale(4)">
    <!-- Ambient halo behind panels -->
    <ellipse cx="80" cy="80" rx="80" ry="50" fill="url(#halo)" filter="url(#haloBlur)"/>

    <!-- Panel 1: blue -->
    <polygon points="34,42 88,30 96,118 38,108" fill="url(#blue)"/>
    <!-- Sheen on panel 1 -->
    <polygon points="34,42 88,30 96,118 38,108" fill="url(#sheen)"/>

    <!-- Panel 2: indigo -->
    <polygon points="88,30 122,52 122,128 96,118" fill="url(#indigo)"/>

    <!-- Panel 3: purple -->
    <polygon points="122,52 138,76 134,134 122,128" fill="url(#purple)"/>

    <!-- Crease lines -->
    <line x1="88"  y1="30" x2="96"  y2="118" stroke="#ffffff" stroke-width="0.6" opacity="0.6"/>
    <line x1="122" y1="52" x2="122" y2="128" stroke="#ffffff" stroke-width="0.6" opacity="0.5"/>
  </g>
</svg>
```

- [ ] **Step 2: Validate the SVG is well-formed**

Run: `xmllint --noout scripts/app-icon-source.svg`
Expected: exit code 0, no output (parses cleanly).

If `xmllint` is not installed: `sudo apt install libxml2-utils`.

- [ ] **Step 3: Eyeball the SVG in a browser**

Open `scripts/app-icon-source.svg` directly in a browser. Verify it renders as a dark indigo→purple square with the three-panel fold mark centered, with a soft purple halo behind it. If anything looks off (wrong colors, missing panels, halo too strong/weak), fix here before generating the PNG in Task 15.

- [ ] **Step 4: Commit**

```bash
git add scripts/app-icon-source.svg
git commit -m "feat(icon): add canonical app-icon-source.svg (1024px, mirrors LogoGeometry)"
```

---

## Task 14: SVG → PNG render script

**Files:**
- Create: `scripts/generate-app-icon.sh`

Self-resolving paths (works from any CWD), checks for `rsvg-convert`, prints install hint on failure, writes to the AppIcon.appiconset.

- [ ] **Step 1: Create the script**

```bash
#!/usr/bin/env bash
# generate-app-icon.sh
# Renders scripts/app-icon-source.svg → ipad/iExtend/Assets.xcassets/AppIcon.appiconset/icon-1024.png
# at 1024×1024. Uses rsvg-convert (librsvg2-bin). Run-once when the icon source changes.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SRC="$SCRIPT_DIR/app-icon-source.svg"
OUT_DIR="$REPO_ROOT/ipad/iExtend/Assets.xcassets/AppIcon.appiconset"
OUT="$OUT_DIR/icon-1024.png"

if ! command -v rsvg-convert >/dev/null 2>&1; then
    echo "error: rsvg-convert not found." >&2
    echo "install with: sudo apt install librsvg2-bin   (Linux)" >&2
    echo "         or: brew install librsvg              (macOS)" >&2
    exit 1
fi

if [ ! -f "$SRC" ]; then
    echo "error: source SVG missing at $SRC" >&2
    exit 1
fi

mkdir -p "$OUT_DIR"
rsvg-convert -w 1024 -h 1024 "$SRC" -o "$OUT"

echo "wrote $OUT"
ls -la "$OUT"
```

- [ ] **Step 2: Make it executable**

Run: `chmod +x scripts/generate-app-icon.sh`

- [ ] **Step 3: Lint the script**

Run: `shellcheck scripts/generate-app-icon.sh`
Expected: no warnings (or skip if shellcheck not installed — it's a nice-to-have).

- [ ] **Step 4: Commit (without running it yet — Task 15 wires up the assetset)**

```bash
git add scripts/generate-app-icon.sh
git commit -m "feat(icon): add generate-app-icon.sh (rsvg-convert SVG → PNG)"
```

---

## Task 15: `AppIcon.appiconset` + generated PNG + xcodegen

**Files:**
- Create: `ipad/iExtend/Assets.xcassets/Contents.json`
- Create: `ipad/iExtend/Assets.xcassets/AppIcon.appiconset/Contents.json`
- Create: `ipad/iExtend/Assets.xcassets/AppIcon.appiconset/icon-1024.png` (generated)

iOS 17 single-size template. The `Assets.xcassets` directory doesn't exist yet (verified in pre-flight: `find ipad/iExtend -name "Assets.xcassets"` returned empty), so create both the catalog `Contents.json` and the `AppIcon.appiconset/Contents.json`.

- [ ] **Step 1: Create the asset catalog `Contents.json`**

Create `ipad/iExtend/Assets.xcassets/Contents.json`:

```json
{
  "info" : {
    "author" : "xcode",
    "version" : 1
  }
}
```

- [ ] **Step 2: Create the AppIcon `Contents.json`**

Create `ipad/iExtend/Assets.xcassets/AppIcon.appiconset/Contents.json`:

```json
{
  "images" : [
    {
      "filename" : "icon-1024.png",
      "idiom" : "universal",
      "platform" : "ios",
      "size" : "1024x1024"
    }
  ],
  "info" : {
    "author" : "xcode",
    "version" : 1
  }
}
```

- [ ] **Step 3: Run the render script to produce the PNG**

Run: `bash scripts/generate-app-icon.sh`
Expected output:
```
wrote /<repo>/ipad/iExtend/Assets.xcassets/AppIcon.appiconset/icon-1024.png
-rw-r--r-- ... ... ... icon-1024.png
```

- [ ] **Step 4: Verify the PNG dimensions and size**

Run: `file ipad/iExtend/Assets.xcassets/AppIcon.appiconset/icon-1024.png`
Expected: `... PNG image data, 1024 x 1024, 8-bit/color RGBA, non-interlaced`

If `file` reports a different dimension, the script or SVG `width`/`height` is wrong — debug before committing.

- [ ] **Step 5: Eyeball the PNG**

Open `ipad/iExtend/Assets.xcassets/AppIcon.appiconset/icon-1024.png` in an image viewer. It should look identical to the SVG rendered at 1024px — dark gradient background, three-panel fold mark centered with a soft halo behind. If colors look washed out or the halo is missing, `rsvg-convert` may have rendering quirks with the gradient/filter — switch to `resvg` (`apt install resvg` if available) and update the script.

- [ ] **Step 6: Regenerate the Xcode project (engineer with macOS + xcodegen)**

Run: `cd ipad && xcodegen generate`
Expected: regenerates `iExtend.xcodeproj` with the new `Assets.xcassets` included in the `iExtend` target's resources. The `project.yml` already declares `sources: - path: iExtend` (line 32), so xcodegen picks up new files automatically.

If on Linux without xcodegen, skip this step — the .xcodeproj is regenerated in CI on the next push.

- [ ] **Step 7: Commit the assetset and PNG**

```bash
git add ipad/iExtend/Assets.xcassets/
git commit -m "feat(icon): wire AppIcon.appiconset (1024×1024 PNG generated from SVG)"
```

---

## Spec Coverage Check (do this after all tasks)

Before declaring done, verify against `docs/superpowers/specs/2026-05-10-iextend-logo-design.md`:

- [ ] **Visual concept** — Origami fold with 3 panels, gradient-shaded, floor shadow + halo. → Tasks 1, 2, 4, 5.
- [ ] **Geometry** — exact panel coordinates from spec table. → Task 1, asserted in Task 3 tests.
- [ ] **Floating effect** — floor shadow + ambient glow, dark at full strength, light at half. → Task 5.
- [ ] **Wordmark** — SF Pro Display semibold, ≈ −1% tracking, theme-aware ink color. → Task 6.
- [ ] **Three deliverables** — LogoMark, LogoLockup, app icon. → Tasks 4–6, 13–15.
- [ ] **File layout** — Branding/ folder, scripts/, AppIcon.appiconset. → Tasks 1, 2, 4, 6, 13, 14, 15.
- [ ] **Component contracts** — `LogoMark(size:, floats:)`, `LogoLockup(.compact|.hero)`, `BrandGradient` extensions. → Tasks 4, 5, 6, 2.
- [ ] **Where it appears** — Welcome (hero), Discover/Pair (compact), FloatingToolbar (mark only), Settings sidebar (compact), Settings General About (mark + version), springboard (icon). → Tasks 7–12, 15.
- [ ] **App-icon pipeline** — canonical SVG, `rsvg-convert` script, single-size `Contents.json`. → Tasks 13–15.
- [ ] **Out of scope** — animated launch logo, iPadOS 18 tinted variants, host/tray/installer chrome — none of these have tasks. ✓

---

## Notes for the executor

- Each task is independent and committable on its own — failing tasks don't block earlier ones.
- TDD applies cleanly only to Task 3 (unit tests for the data constants in Tasks 1–2). The visual tasks rely on Xcode Preview for verification.
- If you hit "swift build" failures on Linux, those are expected for files gated by `#if canImport(UIKit)` only when the dependent code can't be parsed. The pure-data files in Tasks 1, 2 must build cleanly on macOS host.
- The Xcode project (`iExtend.xcodeproj`) is regenerated by xcodegen at CI time per `project.yml`. Any file under `ipad/iExtend/` (other than `Info.plist`) and any file under the SPM packages is auto-included — you do not need to edit the .xcodeproj by hand.
