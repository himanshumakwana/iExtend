// SettingsView.swift
// "iPad · Settings" artboard — iPadOS-style sidebar + detail navigation.
// Mirrors SceneSettings in scenes-ipad.jsx.
// Groups: Connection / Display / Pencil & Touch / Performance / General / Diagnostics.

#if canImport(UIKit)
import SwiftUI
import iExtendKit

public struct SettingsView: View {
    @Environment(\.colorScheme) private var cs
    @Environment(\.theme) private var t
    @Environment(\.dismiss) private var dismiss

    @ObservedObject var session: SessionViewModel
    @State private var selectedGroup: SettingsGroup = .connection
    @State private var searchText = ""

    public init(session: SessionViewModel) {
        self.session = session
    }

    public var body: some View {
        NavigationSplitView(
            sidebar: { sidebar },
            detail: { detailPane }
        )
        .navigationSplitViewStyle(.balanced)
        .background(t.groupBg.ignoresSafeArea())
        .accentColor(t.accent)
    }

    // MARK: Sidebar

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

            // Search bar
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 15))
                    .foregroundStyle(t.ink2)
                TextField("Search", text: $searchText)
                    .font(.body(14))
                    .foregroundStyle(t.ink)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .background(
                RoundedRectangle(cornerRadius: 10)
                    .fill(cs == .dark ? Color.white.opacity(0.12) : Color(red: 118/255, green: 118/255, blue: 128/255).opacity(0.12))
            )
            .padding(.horizontal, 12)
            .padding(.bottom, 8)

            // Group 1: Connection + Display + Pencil + Performance
            SidebarSection(groups: [
                SidebarItem(group: .connection,   label: "Connection",    sysIcon: "link",              tint: t.accent,  badge: session.connectedHostName.isEmpty ? nil : session.connectedHostName),
                SidebarItem(group: .display,      label: "Display",       sysIcon: "display",           tint: t.indigo,  badge: nil),
                SidebarItem(group: .pencilTouch,  label: "Pencil & Touch",sysIcon: "pencil.tip",        tint: t.orange,  badge: nil),
                SidebarItem(group: .performance,  label: "Performance",   sysIcon: "bolt.fill",         tint: t.green,   badge: nil),
            ], selected: $selectedGroup, theme: t)

            // Group 2: General + Diagnostics
            SidebarSection(groups: [
                SidebarItem(group: .general,      label: "General",       sysIcon: "gearshape.fill",    tint: t.ink2,    badge: nil),
                SidebarItem(group: .diagnostics,  label: "Diagnostics",   sysIcon: "exclamationmark.triangle.fill", tint: t.red, badge: nil),
            ], selected: $selectedGroup, theme: t)

            Spacer()
        }
        .background(t.groupBg)
        .navigationBarHidden(true)
    }

    // MARK: Detail pane

    @ViewBuilder
    private var detailPane: some View {
        switch selectedGroup {
        case .connection:
            ConnectionPane(session: session)
        case .display:
            DisplayPane()
        case .pencilTouch:
            PlaceholderPane(title: "Pencil & Touch", message: "Pencil sensitivity and touch rejection settings — Plan 8.")
        case .performance:
            PlaceholderPane(title: "Performance", message: "Bitrate, codec, and frame pacing settings — Plan 8.")
        case .general:
            PlaceholderPane(title: "General", message: "App version, feedback, and reset options.")
        case .diagnostics:
            PlaceholderPane(title: "Diagnostics", message: "Log viewer, packet inspector, and DTLS trace — Plan 9.")
        }
    }
}

// MARK: - SettingsGroup

public enum SettingsGroup: String, CaseIterable, Hashable {
    case connection, display, pencilTouch, performance, general, diagnostics
}

// MARK: - SidebarItem + SidebarSection

private struct SidebarItem {
    let group: SettingsGroup
    let label: String
    let sysIcon: String
    let tint: Color
    let badge: String?
}

private struct SidebarSection: View {
    @Environment(\.colorScheme) private var cs
    let groups: [SidebarItem]
    @Binding var selected: SettingsGroup
    let theme: Theme

    var body: some View {
        VStack(spacing: 2) {
            ForEach(groups, id: \.group) { item in
                Button {
                    selected = item.group
                } label: {
                    HStack(spacing: 12) {
                        ZStack {
                            RoundedRectangle(cornerRadius: 7)
                                .fill(item.tint.opacity(0.15))
                            Image(systemName: item.sysIcon)
                                .font(.system(size: 14, weight: .medium))
                                .foregroundStyle(item.tint)
                        }
                        .frame(width: 28, height: 28)

                        Text(item.label)
                            .font(.body(15, weight: .medium))
                            .foregroundStyle(theme.ink)
                            .frame(maxWidth: .infinity, alignment: .leading)

                        if let badge = item.badge {
                            Text(badge)
                                .font(.body(12))
                                .foregroundStyle(theme.ink2)
                                .lineLimit(1)
                        }

                        Image(systemName: "chevron.right")
                            .font(.system(size: 11, weight: .medium))
                            .foregroundStyle(theme.ink3)
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .background(
                        RoundedRectangle(cornerRadius: 10)
                            .fill(selected == item.group
                                  ? (cs == .dark ? Color.white.opacity(0.12) : Color.black.opacity(0.07))
                                  : Color.clear)
                    )
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 8)
        .padding(.top, 14)
    }
}

// MARK: - ConnectionPane

public struct ConnectionPane: View {
    @Environment(\.colorScheme) private var cs
    @Environment(\.theme) private var t
    @ObservedObject var session: SessionViewModel
    @State private var autoConnect = true
    @State private var connectionMethod = 0

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                Text("Connection")
                    .font(.display(28, weight: .bold))
                    .foregroundStyle(t.ink)
                    .kerning(-0.02 * 28)
                    .padding(.horizontal, 14)
                    .padding(.top, 8)
                    .padding(.bottom, 12)

                SettingsListGroup(header: "Active session") {
                    SettingsRow(title: "Connected to",
                                detail: session.connectedHostName.isEmpty ? "—" : session.connectedHostName)
                    SettingsRow(title: "Network", detail: "Wi\u{2011}Fi")
                    SettingsRow(title: "Mode", detail: "Extended desktop", hasChevron: true)
                    SettingsRow(title: "Latency") {
                        LatencySparkline(latencyMs: session.latencyMs)
                    }
                }

                SettingsListGroup(header: "Connection method") {
                    SettingsRow(title: "Transport") {
                        Picker("Transport", selection: $connectionMethod) {
                            Text("Wi\u{2011}Fi").tag(0)
                            Text("USB-C").tag(1)
                            Text("Auto").tag(2)
                        }
                        .pickerStyle(.segmented)
                        .frame(maxWidth: 220)
                    }
                    SettingsRow(title: "Auto\u{2011}connect on launch") {
                        Toggle("", isOn: $autoConnect)
                            .labelsHidden()
                            .tint(t.green)
                    }
                }

                SettingsListGroup(header: "Display") {
                    SettingsRow(title: "Resolution", detail: "1920 × 1200 @ 120 Hz", hasChevron: true)
                    SettingsRow(title: "Scaling") {
                        Slider(value: .constant(0.6), in: 0...1)
                            .frame(maxWidth: 200)
                            .tint(t.accent)
                    }
                    SettingsRow(title: "Color", detail: "P3 Wide", hasChevron: true)
                    SettingsRow(title: "HDR") {
                        Toggle("", isOn: .constant(false))
                            .labelsHidden()
                            .tint(t.green)
                    }
                }
            }
            .padding(.bottom, 40)
        }
        .background(t.groupBg.ignoresSafeArea())
        .navigationBarHidden(true)
    }
}

// MARK: - DisplayPane

public struct DisplayPane: View {
    @Environment(\.theme) private var t
    @State private var scaling: Double = 0.6
    @State private var hdr = false

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                Text("Display")
                    .font(.display(28, weight: .bold))
                    .foregroundStyle(t.ink)
                    .kerning(-0.02 * 28)
                    .padding(.horizontal, 14)
                    .padding(.top, 8)
                    .padding(.bottom, 12)

                SettingsListGroup(header: "Resolution") {
                    SettingsRow(title: "Resolution", detail: "1920 × 1200 @ 120 Hz", hasChevron: true)
                    SettingsRow(title: "Refresh rate", detail: "120 Hz (ProMotion)", hasChevron: true)
                }

                SettingsListGroup(header: "Appearance") {
                    SettingsRow(title: "Scaling") {
                        HStack {
                            Slider(value: $scaling, in: 0.5...2.0)
                                .frame(maxWidth: 180)
                                .tint(t.accent)
                            Text(String(format: "%.0f%%", scaling * 100))
                                .font(.mono(13))
                                .foregroundStyle(t.ink2)
                                .frame(width: 44, alignment: .trailing)
                        }
                    }
                    SettingsRow(title: "Color space", detail: "P3 Wide", hasChevron: true)
                    SettingsRow(title: "HDR") {
                        Toggle("", isOn: $hdr).labelsHidden().tint(t.green)
                    }
                }
            }
            .padding(.bottom, 40)
        }
        .background(t.bg.ignoresSafeArea())
        .navigationBarHidden(true)
    }
}

// MARK: - PlaceholderPane

private struct PlaceholderPane: View {
    @Environment(\.theme) private var t
    let title: String
    let message: String

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(title)
                .font(.display(28, weight: .bold))
                .foregroundStyle(t.ink)
                .kerning(-0.02 * 28)
                .padding(.horizontal, 14)
                .padding(.top, 8)
                .padding(.bottom, 12)

            VStack(spacing: 8) {
                Image(systemName: "wrench.and.screwdriver")
                    .font(.system(size: 36))
                    .foregroundStyle(t.ink3)
                Text(message)
                    .font(.body(14))
                    .foregroundStyle(t.ink2)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 320)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .background(t.groupBg.ignoresSafeArea())
        .navigationBarHidden(true)
    }
}

// MARK: - SettingsListGroup + SettingsRow

public struct SettingsListGroup<Content: View>: View {
    @Environment(\.colorScheme) private var cs
    @Environment(\.theme) private var t
    let header: String?
    @ViewBuilder let content: () -> Content

    public init(header: String? = nil, @ViewBuilder content: @escaping () -> Content) {
        self.header = header; self.content = content
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let header {
                Text(header)
                    .font(.body(12))
                    .foregroundStyle(t.ink2)
                    .textCase(.uppercase)
                    .kerning(0.06)
                    .padding(.horizontal, 30)
                    .padding(.bottom, 6)
            }
            VStack(spacing: 0) {
                content()
            }
            .background(
                RoundedRectangle(cornerRadius: 18)
                    .fill(t.card)
                    .overlay(
                        RoundedRectangle(cornerRadius: 18)
                            .strokeBorder(t.sep, lineWidth: 0.5)
                    )
            )
            .clipShape(RoundedRectangle(cornerRadius: 18))
            .padding(.horizontal, 14)
        }
        .padding(.top, 18)
    }
}

public struct SettingsRow<Right: View>: View {
    @Environment(\.theme) private var t
    let title: String
    var detail: String?
    var hasChevron: Bool
    @ViewBuilder var rightContent: () -> Right

    public init(title: String, detail: String? = nil, hasChevron: Bool = false, @ViewBuilder right: @escaping () -> Right) {
        self.title = title; self.detail = detail; self.hasChevron = hasChevron; self.rightContent = right
    }

    public var body: some View {
        HStack(spacing: 0) {
            Text(title)
                .font(.body(15))
                .foregroundStyle(t.ink)
                .frame(maxWidth: .infinity, alignment: .leading)

            if let detail {
                Text(detail)
                    .font(.body(15))
                    .foregroundStyle(t.ink2)
            }

            rightContent()
                .padding(.leading, 8)

            if hasChevron {
                Image(systemName: "chevron.right")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(t.ink3)
                    .padding(.leading, 8)
            }
        }
        .padding(.horizontal, 16)
        .frame(minHeight: 44)
        .overlay(alignment: .bottom) {
            Divider()
                .background(t.sep)
                .padding(.leading, 16)
        }
    }
}

// Convenience overload with no right content
extension SettingsRow where Right == EmptyView {
    public init(title: String, detail: String? = nil, hasChevron: Bool = false) {
        self.init(title: title, detail: detail, hasChevron: hasChevron) { EmptyView() }
    }
}

// MARK: - LatencySparkline

struct LatencySparkline: View {
    let latencyMs: Double
    @State private var samples: [Double] = Array(repeating: 8, count: 20)

    var body: some View {
        HStack(spacing: 8) {
            SparklineShape(samples: samples)
                .stroke(Color(hex: "#30d158"), lineWidth: 1.6)
                .frame(width: 140, height: 24)
            Text("\(Int(latencyMs)) ms")
                .font(.mono(13))
                .foregroundStyle(.primary)
        }
        .onAppear { startUpdating() }
    }

    private func startUpdating() {
        Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { _ in
            samples.append(latencyMs + Double.random(in: -2...2))
            if samples.count > 20 { samples.removeFirst() }
        }
    }
}

// MARK: - SparklineShape

private struct SparklineShape: Shape {
    let samples: [Double]

    func path(in rect: CGRect) -> Path {
        guard samples.count > 1 else { return Path() }
        let max = samples.max() ?? 1
        var path = Path()
        for (i, v) in samples.enumerated() {
            let x = rect.width * CGFloat(i) / CGFloat(samples.count - 1)
            let y = rect.height - (rect.height * CGFloat(v / max))
            if i == 0 { path.move(to: CGPoint(x: x, y: y)) }
            else       { path.addLine(to: CGPoint(x: x, y: y)) }
        }
        return path
    }
}

// MARK: - Preview

#Preview {
    SettingsView(session: SessionViewModel(session: IExtendSession()))
        .preferredColorScheme(.dark)
        .applyTheme(Theme(dark: true))
}
#endif
