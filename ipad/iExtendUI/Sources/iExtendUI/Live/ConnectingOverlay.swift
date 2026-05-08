// ConnectingOverlay.swift
// "iPad · Connecting" state — frosted glass card over the live surface.
// Mirrors the ConnectingOverlay component in scenes-ipad.jsx.
// Shown during ConnectionState.connecting.

import SwiftUI

public struct ConnectingOverlay: View {
    let hostName: String
    @State private var progress: Double = 0.40
    @State private var stepLabel: String = "Negotiating display · 1 of 3"

    public init(hostName: String = "your PC") {
        self.hostName = hostName
    }

    public var body: some View {
        ZStack {
            // Frosted backdrop
            Color.black.opacity(0.55)
                .ignoresSafeArea()
                .overlay(.ultraThinMaterial.opacity(0.15))

            // Card
            VStack(spacing: 0) {
                SpinnerView()
                    .padding(.bottom, 14)

                Text("Connecting to \(hostName)\u{2026}")
                    .font(.system(size: 17, weight: .semibold))
                    .foregroundStyle(.white)

                Text(stepLabel)
                    .font(.system(size: 13))
                    .foregroundStyle(Color.white.opacity(0.60))
                    .padding(.top, 4)
                    .animation(.easeInOut, value: stepLabel)

                // Progress bar
                GeometryReader { geo in
                    ZStack(alignment: .leading) {
                        Capsule()
                            .fill(Color.white.opacity(0.10))
                        Capsule()
                            .fill(
                                LinearGradient(
                                    colors: [Color(hex: "#0a84ff"), Color(hex: "#5e5ce6")],
                                    startPoint: .leading,
                                    endPoint: .trailing
                                )
                            )
                            .frame(width: geo.size.width * progress)
                    }
                    .frame(height: 4)
                }
                .frame(height: 4)
                .padding(.top, 16)
            }
            .padding(28)
            .frame(width: 320)
            .background(
                RoundedRectangle(cornerRadius: 24)
                    .fill(Color(red: 28/255, green: 28/255, blue: 30/255).opacity(0.85))
                    .overlay(
                        RoundedRectangle(cornerRadius: 24)
                            .strokeBorder(Color.white.opacity(0.10), lineWidth: 1)
                    )
                    .shadow(color: .black.opacity(0.5), radius: 30, y: 10)
            )
        }
        .onAppear { animateProgress() }
    }

    // Simulate a 3-step connect sequence so the UI looks alive in Plan 6.
    // Real steps are driven by PeerConnection state transitions in Plan 7+.
    private func animateProgress() {
        let steps: [(delay: Double, progress: Double, label: String)] = [
            (0.8,  0.55, "ICE candidates · 2 of 3"),
            (1.8,  0.80, "DTLS handshake · 3 of 3"),
            (2.8,  1.00, "Almost there\u{2026}"),
        ]
        for step in steps {
            DispatchQueue.main.asyncAfter(deadline: .now() + step.delay) {
                withAnimation(.easeInOut(duration: 0.4)) {
                    self.progress = step.progress
                    self.stepLabel = step.label
                }
            }
        }
    }
}

// MARK: - SpinnerView

struct SpinnerView: View {
    @State private var rotation: Double = 0

    var body: some View {
        ZStack {
            Circle()
                .stroke(Color.white.opacity(0.15), lineWidth: 3)
                .frame(width: 38, height: 38)

            Circle()
                .trim(from: 0, to: 0.25)
                .stroke(Color(hex: "#0a84ff"), style: StrokeStyle(lineWidth: 3, lineCap: .round))
                .frame(width: 38, height: 38)
                .rotationEffect(.degrees(rotation))
                .onAppear {
                    withAnimation(.linear(duration: 1).repeatForever(autoreverses: false)) {
                        rotation = 360
                    }
                }
        }
    }
}

// MARK: - Preview

#Preview {
    ZStack {
        Color(hex: "#0c1430").ignoresSafeArea()
        ConnectingOverlay(hostName: "Aman's PC")
    }
    .preferredColorScheme(.dark)
}
