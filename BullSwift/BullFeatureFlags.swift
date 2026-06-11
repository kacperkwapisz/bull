import Foundation

/// Product vs developer surfaces. Dev gaps and tools stay off consumer paths unless explicitly enabled.
enum BullFeatureFlags {
  /// Coach overview shows packet input / algorithm readiness gaps (developer).
  static let showCoachDevGaps: Bool = {
    #if DEBUG
    let processInfo = ProcessInfo.processInfo
    return processInfo.arguments.contains("--bull-coach-dev-gaps")
      || processInfo.environment["BULL_COACH_DEV_GAPS"] == "1"
    #else
    return false
    #endif
  }()
}