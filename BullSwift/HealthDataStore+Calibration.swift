import Foundation

extension HealthDataStore {
  /// Distinct days with a stored recovery score (packet daily rows or today's live score).
  func calibrationRecoveryScoreDayCount() -> Int {
    if let report = packetScoreReports["recovery"] {
      let dailyCount = Self.array(report["daily"])
        .filter { Self.doubleValue($0["score_0_to_100"]) != nil }
        .count
      if dailyCount > 0 {
        return dailyCount
      }
    }
    return recoveryScoreValue() != nil ? 1 : 0
  }

  /// Observed personal-baseline nights when the recovery score report exposes them.
  func recoveryBaselineObservedNightCount() -> Int? {
    let recoveryV2 = Self.map(packetScoreReports["recovery"], "score_result", "provenance", "recovery_v2")
    if let hrvBaselineNights = Self.intValue(recoveryV2?["hrv_baseline_n"]) {
      return hrvBaselineNights
    }
    if let rhrBaselineNights = Self.intValue(recoveryV2?["rhr_baseline_n"]) {
      return rhrBaselineNights
    }
    return nil
  }

  /// Nights with sleep score or duration from the local sleep bridge.
  func calibrationSleepNightCount() -> Int {
    if let report = packetScoreReports["sleep"] {
      let dailyCount = Self.array(report["daily"])
        .filter {
          Self.doubleValue($0["score_0_to_100"]) != nil
            || Self.doubleValue($0["sleep_duration_minutes"]) != nil
            || Self.doubleValue($0["time_in_bed_minutes"]) != nil
        }
        .count
      if dailyCount > 0 {
        return dailyCount
      }
    }
    return primarySleepDetail != nil ? 1 : 0
  }

  /// Days with meaningful strain signal (daily activity rows or non-zero strain today).
  func calibrationStrainDayCount() -> Int {
    let activityRows = displaySafeMetrics(family: "daily_activity")
    if !activityRows.isEmpty {
      return activityRows.count
    }
    return strainScore0To100() > 0 ? 1 : 0
  }

  /// Days with heart-rate samples usable for stress (distinct calendar days in stored series).
  /// Intentionally avoids `stressAlgorithmSummary()` — that buckets a full day of HR on the main thread.
  func calibrationStressDayCount() -> Int {
    heartRateSeriesStore.distinctSampleDayCount(withinLastDays: 14)
  }

  func calibrationMetricReady(_ route: HealthRoute, requiredCount: Int = 3) -> Bool {
    switch route {
    case .recovery:
      return calibrationRecoveryScoreDayCount() >= requiredCount
    case .sleep:
      return calibrationSleepNightCount() >= requiredCount
    case .strain:
      return calibrationStrainDayCount() >= requiredCount
    case .stress:
      return calibrationStressDayCount() >= requiredCount
    default:
      return true
    }
  }
}

extension HeartRateSeriesStore {
  /// Distinct local-calendar days represented in stored HR samples (bounded scan).
  func distinctSampleDayCount(withinLastDays: Int) -> Int {
    let calendar = Calendar.current
    let end = Date()
    guard let start = calendar.date(byAdding: .day, value: -withinLastDays, to: end) else {
      return 0
    }
    var days = Set<Date>()
    for sample in samples(from: start, to: end) {
      days.insert(calendar.startOfDay(for: sample.capturedAt))
    }
    return days.count
  }
}