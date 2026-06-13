import Foundation
import Security

enum CoachAuthKeychain {
  private static let service = "com.bull.swift.coach"
  private static let account = "access-token"

  /// Distinguishes "there is no token" from "the keychain cannot be read
  /// right now" (e.g. the app was relaunched in the background for overnight
  /// BLE capture while the device is locked). Callers must only treat
  /// `.notFound` / an invalid decoded token as a reason to purge — purging on
  /// `.unavailable` would destroy a valid session.
  enum LoadResult {
    case found(String)
    case notFound
    case unavailable(OSStatus)
  }

  static func save(token: String) throws {
    let data = Data(token.utf8)
    let query: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: account,
    ]
    SecItemDelete(query as CFDictionary)
    var insert = query
    insert[kSecValueData as String] = data
    // AfterFirstUnlock: the session token must stay readable when the app is
    // relaunched in the background (overnight capture, uploads) while the
    // device is locked. WhenUnlocked (the default) made those launches see
    // "no token" and sign the user out.
    insert[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlock
    let status = SecItemAdd(insert as CFDictionary, nil)
    guard status == errSecSuccess else {
      throw CoachAuthError.keychain(status)
    }
  }

  static func load() -> String? {
    if case .found(let token) = loadResult() {
      return token
    }
    return nil
  }

  static func loadResult() -> LoadResult {
    let query: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: account,
      kSecReturnData as String: true,
      kSecMatchLimit as String: kSecMatchLimitOne,
    ]
    var item: CFTypeRef?
    let status = SecItemCopyMatching(query as CFDictionary, &item)
    switch status {
    case errSecSuccess:
      guard let data = item as? Data, let token = String(data: data, encoding: .utf8) else {
        return .notFound
      }
      return .found(token)
    case errSecItemNotFound:
      return .notFound
    default:
      return .unavailable(status)
    }
  }

  static func delete() {
    let query: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: account,
    ]
    SecItemDelete(query as CFDictionary)
  }
}

enum CoachAuthError: Error, LocalizedError {
  case keychain(OSStatus)
  case http(Int, String)
  case invalidResponse

  var errorDescription: String? {
    switch self {
    case .keychain(let status):
      return "Coach token keychain error (\(status))."
    case .http(let code, let body):
      return body.isEmpty ? "Coach auth failed (HTTP \(code))." : "Coach auth failed (HTTP \(code)): \(body)"
    case .invalidResponse:
      return "Coach auth returned an invalid response."
    }
  }
}