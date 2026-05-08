// PinEntryView.swift
// 4-digit numeric PIN entry behavior used by PairView.
// Handles tap on digit buttons and backspace, exposes the current 4-char string.
// Plan 7's PairingFlow.runSpake2(pin:) receives this string.

import SwiftUI

public struct PinEntryView: View {
    @Environment(\.colorScheme) private var cs
    @Environment(\.theme) private var t

    @Binding var pin: String            // caller observes this
    let expiresIn: String               // e.g. "0:54"
    var onSubmit: (String) -> Void

    public init(pin: Binding<String>, expiresIn: String = "1:00", onSubmit: @escaping (String) -> Void = { _ in }) {
        self._pin = pin
        self.expiresIn = expiresIn
        self.onSubmit = onSubmit
    }

    private let digits = ["1","2","3","4","5","6","7","8","9","","0","⌫"]

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Section header
            Text("Or enter PIN from PC")
                .font(.body(13, weight: .semibold))
                .foregroundStyle(t.ink2)
                .textCase(.uppercase)
                .kerning(0.1)

            Text("Your computer shows a 4\u{2011}digit code")
                .font(.body(16, weight: .bold))
                .foregroundStyle(t.ink)
                .kerning(-0.02 * 16)
                .padding(.top, 4)
                .padding(.bottom, 14)

            // PIN boxes
            PINBoxes(pin: pin)
                .frame(maxWidth: .infinity)
                .padding(.bottom, 12)

            // Numeric pad
            LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 6), count: 3), spacing: 6) {
                ForEach(digits.indices, id: \.self) { i in
                    let key = digits[i]
                    if key.isEmpty {
                        Color.clear
                            .frame(height: 38)
                    } else {
                        Button {
                            handleKey(key)
                        } label: {
                            Text(key)
                                .font(.display(14, weight: .medium))
                                .foregroundStyle(t.ink)
                                .frame(maxWidth: .infinity)
                                .frame(height: 38)
                                .background(
                                    RoundedRectangle(cornerRadius: 8)
                                        .fill(cs == .dark
                                              ? Color.white.opacity(0.06)
                                              : Color.black.opacity(0.04))
                                )
                        }
                        .buttonStyle(.plain)
                    }
                }
            }

            Spacer(minLength: 0)

            // Expiry note
            Group {
                Text("Pairing expires in ")
                    .foregroundStyle(t.ink2)
                + Text(expiresIn)
                    .fontWeight(.semibold)
                    .foregroundStyle(t.ink)
            }
            .font(.body(11))
            .frame(maxWidth: .infinity, alignment: .center)
            .padding(.top, 4)
        }
        .padding(18)
        .background(
            RoundedRectangle(cornerRadius: 22)
                .fill(t.card)
                .overlay(
                    RoundedRectangle(cornerRadius: 22)
                        .strokeBorder(t.sep, lineWidth: 1)
                )
        )
    }

    // MARK: Key handling

    private func handleKey(_ key: String) {
        if key == "⌫" {
            if !pin.isEmpty { pin.removeLast() }
        } else if pin.count < 4 {
            pin.append(key)
            if pin.count == 4 {
                onSubmit(pin)
            }
        }
    }
}

// MARK: - PINBoxes

public struct PINBoxes: View {
    @Environment(\.colorScheme) private var cs
    @Environment(\.theme) private var t
    let pin: String   // 0–4 characters

    public var body: some View {
        HStack(spacing: 8) {
            ForEach(0..<4) { i in
                let ch: String = i < pin.count ? String(pin[pin.index(pin.startIndex, offsetBy: i)]) : ""
                let isActive = i == pin.count && pin.count < 4

                Text(ch.isEmpty ? " " : ch)
                    .font(.display(22, weight: .semibold))
                    .foregroundStyle(t.ink)
                    .kerning(-0.02)
                    .frame(width: 42, height: 52)
                    .background(
                        RoundedRectangle(cornerRadius: 10)
                            .fill(cs == .dark ? Color.white.opacity(0.04) : Color.black.opacity(0.04))
                            .overlay(
                                RoundedRectangle(cornerRadius: 10)
                                    .strokeBorder(
                                        isActive ? t.accent : t.sep,
                                        lineWidth: isActive ? 1.5 : 1
                                    )
                            )
                    )
                    .animation(.easeInOut(duration: 0.15), value: isActive)
            }
        }
    }
}

// MARK: - Preview

#Preview {
    struct Wrapper: View {
        @State var pin = "47"
        var body: some View {
            PinEntryView(pin: $pin, expiresIn: "0:54") { p in
                print("Submitted: \(p)")
            }
            .frame(width: 300, height: 420)
            .preferredColorScheme(.dark)
            .applyTheme(Theme(dark: true))
            .padding()
            .background(Color(hex: "#1c1c1e"))
        }
    }
    return Wrapper()
}
