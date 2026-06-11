import Foundation

enum CoachConsentStore {
  private static let key = "bull.coach.ai.consent.v1"

  static var hasAccepted: Bool {
    UserDefaults.standard.bool(forKey: key)
  }

  static func accept() {
    UserDefaults.standard.set(true, forKey: key)
  }

  static func reset() {
    UserDefaults.standard.removeObject(forKey: key)
  }
}