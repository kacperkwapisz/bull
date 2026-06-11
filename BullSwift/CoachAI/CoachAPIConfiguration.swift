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
    // Local dev default (your current CoachAPI host + port on the Tailscale/local network).
    // The shared "BullSwift" scheme (xcshareddata/xcschemes) pre-sets COACH_API_BASE_URL for you.
    // You can also override via Xcode scheme env vars manually, or Info.plist key.
    return URL(string: "http://100.95.172.121:3333")!
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