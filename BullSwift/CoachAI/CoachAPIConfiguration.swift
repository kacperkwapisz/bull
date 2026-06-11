import Foundation

enum CoachAPIConfiguration {
  private static let productionDefault = "https://coach.bull.local"

  static var baseURL: URL {
    if let override = ProcessInfo.processInfo.environment["COACH_API_BASE_URL"],
       let url = URL(string: override) {
      return url
    }
    if let plist = Bundle.main.object(forInfoDictionaryKey: "COACH_API_BASE_URL") as? String,
       let url = URL(string: plist) {
      return url
    }
    #if DEBUG
    return URL(string: "http://127.0.0.1:3000")!
    #else
    return URL(string: productionDefault)!
    #endif
  }

  static var responsesURL: URL {
    baseURL.appendingPathComponent("v1/coach/responses")
  }

  static var devTokenURL: URL {
    baseURL.appendingPathComponent("v1/auth/dev-token")
  }
}