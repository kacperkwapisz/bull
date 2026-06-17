import Foundation

enum CoachAPIConfiguration {
  private static let productionDefault = "https://bull-api.kwapisz.co"

  static var baseURL: URL {
    if let override = ProcessInfo.processInfo.environment["COACH_API_BASE_URL"],
       let url = URL(string: override) {
      return url
    }
    if let plist = Bundle.main.object(forInfoDictionaryKey: "COACH_API_BASE_URL") as? String,
       let url = URL(string: plist) {
      return url
    }
    // Default to the production API in every configuration (Debug included).
    // To target a local dev server instead, set COACH_API_BASE_URL via the
    // Xcode scheme's environment variables (the shared "BullSwift" scheme keeps
    // a disabled entry with the local/Tailscale address ready to re-enable) or
    // an Info.plist key.
    return URL(string: productionDefault)!
  }

  static var responsesURL: URL {
    baseURL.appendingPathComponent("v1/coach/responses")
  }

  static var appleAuthURL: URL {
    baseURL.appendingPathComponent("v1/auth/apple")
  }

  static var dataUploadsURL: URL {
    baseURL.appendingPathComponent("v1/data/uploads")
  }

  /// Curated daily-metrics sync (Track A): POST pushes locally-computed rows to
  /// the long-term store; GET restores history into the local store on a fresh
  /// install. Same URL for both verbs.
  static var dataMetricsURL: URL {
    baseURL.appendingPathComponent("v1/data/metrics")
  }

  /// Profile + timezone upload so server-side compute can derive energy and
  /// bucket daily rollups on the user's local calendar day.
  static var dataProfileURL: URL {
    baseURL.appendingPathComponent("v1/data/profile")
  }
}