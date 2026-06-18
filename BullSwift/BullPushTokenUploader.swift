import Foundation

/// Registers this device's APNs token with the user's Bull account so the server
/// can deliver recovery-ready pushes. The token is cached locally and uploaded
/// whenever it changes and the user is signed in; an unchanged token is not
/// re-sent.
enum BullPushTokenUploader {
  private static let deviceTokenKey = "bull.push.device-token"
  private static let lastUploadedKey = "bull.push.last-uploaded-token"

  /// Called from the app delegate when APNs returns a device token.
  static func register(deviceToken: Data) {
    let hex = deviceToken.map { String(format: "%02x", $0) }.joined()
    UserDefaults.standard.set(hex, forKey: deviceTokenKey)
    upload(token: hex)
  }

  /// Re-attempt upload of the cached token (e.g. after sign-in or on activate).
  static func uploadCachedTokenIfNeeded() {
    guard let token = UserDefaults.standard.string(forKey: deviceTokenKey) else { return }
    upload(token: token)
  }

  private static func upload(token: String) {
    let defaults = UserDefaults.standard
    guard defaults.string(forKey: lastUploadedKey) != token else { return }
    guard let auth = CoachAuthKeychain.load() else { return } // not signed in yet
    var request = URLRequest(url: CoachAPIConfiguration.dataPushTokenURL)
    request.httpMethod = "POST"
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    request.setValue("Bearer \(auth)", forHTTPHeaderField: "Authorization")
    #if DEBUG
    let environment = "sandbox"
    #else
    let environment = "production"
    #endif
    let body: [String: Any] = [
      "token": token,
      "platform": "ios",
      "environment": environment,
      "bundle_id": Bundle.main.bundleIdentifier ?? "com.bull.swift",
    ]
    request.httpBody = try? JSONSerialization.data(withJSONObject: body)
    URLSession.shared.dataTask(with: request) { _, response, _ in
      if let http = response as? HTTPURLResponse, (200...299).contains(http.statusCode) {
        defaults.set(token, forKey: lastUploadedKey)
      }
    }.resume()
  }
}
