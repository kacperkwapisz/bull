import SwiftUI

struct CoachSignInScreen: View {
  let loginStatus: String
  let needsConsent: Bool
  let errorMessage: String?
  let acceptConsent: () -> Void
  let setup: () -> Void

  @State private var consentChecked = false

  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: 16) {
        VStack(alignment: .leading, spacing: 10) {
          Image(systemName: "sparkles")
            .font(.title2.weight(.bold))
            .foregroundStyle(.blue)
            .frame(width: 42, height: 42)
            .background(.blue.opacity(0.12), in: RoundedRectangle(cornerRadius: 8, style: .continuous))

          Text("Set up Coach")
            .font(.title2.bold())
          Text("Bull Coach uses your local metrics and sends questions plus bounded tool summaries to the Bull Coach API for guidance.")
            .font(.subheadline)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 8, style: .continuous))

        VStack(alignment: .leading, spacing: 12) {
          CoachStatusLine(title: "Status", value: loginStatus)

          if needsConsent {
            Toggle(isOn: $consentChecked) {
              Text("I understand Coach may send health-related context to Bull’s AI service. This is not medical advice.")
                .font(.footnote)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            }
            .toggleStyle(.switch)
          }

          if let errorMessage, !errorMessage.isEmpty {
            Label(errorMessage, systemImage: "exclamationmark.triangle")
              .font(.footnote)
              .foregroundStyle(.red)
              .fixedSize(horizontal: false, vertical: true)
          }

          Button(action: primaryAction) {
            Label(needsConsent ? "Continue" : "Enable Coach", systemImage: "checkmark.seal")
              .frame(maxWidth: .infinity)
          }
          .buttonStyle(.borderedProminent)
          .disabled(needsConsent && !consentChecked)

          Text("Tokens stay in Keychain on this device. Tools only read local Bull data.")
            .font(.footnote)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
        }
        .padding(16)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
      }
      .padding(.horizontal, 16)
      .padding(.vertical, 18)
    }
  }

  private func primaryAction() {
    if needsConsent {
      acceptConsent()
    }
    setup()
  }
}

private struct CoachStatusLine: View {
  let title: String
  let value: String

  var body: some View {
    HStack {
      Text(title)
        .font(.subheadline)
        .foregroundStyle(.secondary)
      Spacer()
      Text(value)
        .font(.subheadline.weight(.semibold))
        .foregroundStyle(.primary)
        .lineLimit(1)
        .minimumScaleFactor(0.75)
    }
  }
}