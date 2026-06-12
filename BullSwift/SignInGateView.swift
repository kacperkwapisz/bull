import AuthenticationServices
import SwiftUI

/// Launch gate: the app requires a real Apple account before anything else.
/// Visual language follows the bold, dark, ring-centric activity aesthetic.
struct SignInGateView: View {
  @ObservedObject var session: BullAccountSession
  @State private var ringsVisible = false

  var body: some View {
    ZStack {
      Color.black.ignoresSafeArea()

      VStack(spacing: 0) {
        Spacer(minLength: 0)

        ActivityRingsBadge(animated: ringsVisible)
          .frame(width: 168, height: 168)
          .padding(.bottom, 40)

        Text("Welcome to Bull")
          .font(.system(size: 34, weight: .bold, design: .rounded))
          .foregroundStyle(.white)
          .multilineTextAlignment(.center)
          .padding(.horizontal, 32)

        Text("Recovery, sleep, and strain from your band — kept on your device, coached by AI.")
          .font(.system(size: 16, weight: .medium))
          .foregroundStyle(.white.opacity(0.6))
          .multilineTextAlignment(.center)
          .fixedSize(horizontal: false, vertical: true)
          .padding(.horizontal, 40)
          .padding(.top, 10)

        Spacer(minLength: 0)

        VStack(spacing: 14) {
          if let message = session.errorMessage, !message.isEmpty {
            Label(message, systemImage: "exclamationmark.triangle.fill")
              .font(.footnote.weight(.semibold))
              .foregroundStyle(Color(red: 1.0, green: 0.27, blue: 0.32))
              .multilineTextAlignment(.leading)
              .fixedSize(horizontal: false, vertical: true)
              .padding(.horizontal, 8)
              .transition(.opacity)
          }

          ZStack {
            SignInWithAppleButton(.signIn) { request in
              request.requestedScopes = []
            } onCompletion: { result in
              session.handleAuthorization(result)
            }
            .signInWithAppleButtonStyle(.white)
            .frame(height: 56)
            .clipShape(Capsule())
            .opacity(session.isAuthorizing ? 0.35 : 1)
            .allowsHitTesting(!session.isAuthorizing)

            if session.isAuthorizing {
              ProgressView()
                .tint(.white)
            }
          }

          Text("Your account holds your uploads and Coach sessions. Health data stays on this iPhone unless you upload it.")
            .font(.caption)
            .foregroundStyle(.white.opacity(0.4))
            .multilineTextAlignment(.center)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.horizontal, 16)
        }
        .padding(.horizontal, 24)
        .padding(.bottom, 24)
      }
      .animation(.easeInOut(duration: 0.2), value: session.errorMessage)
    }
    .preferredColorScheme(.dark)
    .onAppear {
      withAnimation(.spring(response: 1.4, dampingFraction: 0.82).delay(0.25)) {
        ringsVisible = true
      }
    }
  }
}

/// Concentric tricolor rings, drawn on with a spring when the gate appears.
private struct ActivityRingsBadge: View {
  let animated: Bool
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  private struct RingSpec {
    let color: Color
    let inset: CGFloat
    let fraction: CGFloat
  }

  private let rings: [RingSpec] = [
    RingSpec(color: Color(red: 0.98, green: 0.07, blue: 0.31), inset: 0, fraction: 0.78),
    RingSpec(color: Color(red: 0.57, green: 0.91, blue: 0.16), inset: 26, fraction: 0.64),
    RingSpec(color: Color(red: 0.0, green: 1.0, blue: 0.96), inset: 52, fraction: 0.86),
  ]

  var body: some View {
    ZStack {
      ForEach(rings.indices, id: \.self) { index in
        let ring = rings[index]
        ZStack {
          Circle()
            .stroke(ring.color.opacity(0.18), lineWidth: 18)
          Circle()
            .trim(from: 0, to: progress(for: ring))
            .stroke(
              ring.color,
              style: StrokeStyle(lineWidth: 18, lineCap: .round)
            )
            .rotationEffect(.degrees(-90))
            .shadow(color: ring.color.opacity(0.35), radius: 6)
        }
        .padding(CGFloat(ring.inset))
      }
    }
    .accessibilityHidden(true)
  }

  private func progress(for ring: RingSpec) -> CGFloat {
    if reduceMotion {
      return ring.fraction
    }
    return animated ? ring.fraction : 0.02
  }
}
