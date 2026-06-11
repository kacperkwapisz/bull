import Foundation
import Security

enum CoachAuthKeychain {
  private static let service = "com.bull.swift.coach"
  private static let account = "access-token"

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
    let status = SecItemAdd(insert as CFDictionary, nil)
    guard status == errSecSuccess else {
      throw CoachAuthError.keychain(status)
    }
  }

  static func load() -> String? {
    let query: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: account,
      kSecReturnData as String: true,
      kSecMatchLimit as String: kSecMatchLimitOne,
    ]
    var item: CFTypeRef?
    let status = SecItemCopyMatching(query as CFDictionary, &item)
    guard status == errSecSuccess, let data = item as? Data else {
      return nil
    }
    return String(data: data, encoding: .utf8)
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