import AuthenticationServices
import Foundation

/// Real-accounts-only session state. The app is gated on launch by Sign in
/// with Apple; the resulting BullAPI session JWT lives in the Keychain
/// (`CoachAuthKeychain`) and is the Bearer token for coach + data requests.
@MainActor
final class BullAccountSession: ObservableObject {
  @Published private(set) var isSignedIn: Bool
  @Published private(set) var isAuthorizing = false
  @Published private(set) var errorMessage: String?

  private static let userIDDefaultsKey = "bull.account.user_id"
  private let client = CoachAPIClient()

  init() {
    // Only a user-scoped session (issued by /v1/auth/apple, carries the
    // `user_id` claim) counts as signed in. Legacy tokens are purged so the
    // device migrates onto a real account through the gate.
    if let token = CoachAuthKeychain.load(), Self.isUserScopedToken(token) {
      isSignedIn = true
    } else {
      CoachAuthKeychain.delete()
      isSignedIn = false
    }
  }

  var userID: String? {
    UserDefaults.standard.string(forKey: Self.userIDDefaultsKey)
  }

  func handleAuthorization(_ result: Result<ASAuthorization, Error>) {
    switch result {
    case .failure(let error):
      if let authError = error as? ASAuthorizationError, authError.code == .canceled {
        return
      }
      errorMessage = describe(error)
    case .success(let authorization):
      guard
        let credential = authorization.credential as? ASAuthorizationAppleIDCredential,
        let tokenData = credential.identityToken,
        let identityToken = String(data: tokenData, encoding: .utf8)
      else {
        errorMessage = "Apple did not return an identity token."
        return
      }
      exchange(identityToken: identityToken)
    }
  }

  func signOut() {
    CoachAuthKeychain.delete()
    UserDefaults.standard.removeObject(forKey: Self.userIDDefaultsKey)
    isSignedIn = false
  }

  private func exchange(identityToken: String) {
    isAuthorizing = true
    errorMessage = nil
    Task { [weak self] in
      guard let self else {
        return
      }
      do {
        let session = try await client.exchangeAppleIdentityToken(
          identityToken,
          deviceID: UIDeviceIdentifier.coachDeviceID
        )
        try CoachAuthKeychain.save(token: session.accessToken)
        UserDefaults.standard.set(session.userID, forKey: Self.userIDDefaultsKey)
        isAuthorizing = false
        isSignedIn = true
      } catch {
        isAuthorizing = false
        errorMessage = describe(error)
      }
    }
  }

  private static func isUserScopedToken(_ token: String) -> Bool {
    let segments = token.split(separator: ".")
    guard segments.count == 3 else {
      return false
    }
    var base64 = String(segments[1])
      .replacingOccurrences(of: "-", with: "+")
      .replacingOccurrences(of: "_", with: "/")
    while base64.count % 4 != 0 {
      base64 += "="
    }
    guard
      let data = Data(base64Encoded: base64),
      let payload = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      let userID = payload["user_id"] as? String, !userID.isEmpty
    else {
      return false
    }
    if let expiry = payload["exp"] as? TimeInterval {
      return Date(timeIntervalSince1970: expiry) > Date()
    }
    return true
  }

  private func describe(_ error: Error) -> String {
    if let localized = error as? LocalizedError, let description = localized.errorDescription {
      return description
    }
    return String(describing: error)
  }
}
