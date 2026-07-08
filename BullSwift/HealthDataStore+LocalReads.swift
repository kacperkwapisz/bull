import Foundation

// MARK: - Local-first read wiring
//
// These helpers mirror the Phase 1 home payload pattern: when local compute is
// enabled, read/compute display payloads from the on-device SQLite store through
// the Rust bridge, and keep the existing server fetchers as honest fallbacks.

extension HealthDataStore {
  nonisolated static func calendarPayloadLocalOrServer(month: String) async -> [CalendarDayScores] {
    let (mode, databasePath, profile): (ComputeMode, String, LocalHomeProfile) = await MainActor.run {
      (HealthDataStore.computeMode, HealthDataStore.defaultDatabasePath(), HealthDataStore.localComputeProfile())
    }
    if mode == .local {
      let local = await localCalendarPayload(month: month, databasePath: databasePath, profile: profile)
      if local.contains(where: \.hasData) {
        return local
      }
      let server = await fetchCalendar(month: month)
      return server.isEmpty ? local : server
    }
    return await fetchCalendar(month: month)
  }

  nonisolated static func queryLocalOrServer(method: String, args: [String: Any] = [:]) async -> [String: Any]? {
    guard isAllowedLocalReadQuery(method) else { return nil }
    let (mode, databasePath, _): (ComputeMode, String, LocalHomeProfile) = await MainActor.run {
      (HealthDataStore.computeMode, HealthDataStore.defaultDatabasePath(), HealthDataStore.localComputeProfile())
    }
    if mode == .local {
      let local = await localReadQuery(method: method, args: args, databasePath: databasePath)
      if let local, localReadResultHasData(local) {
        return local
      }
      let server = await fetchServerQuery(method: method, args: args)
      return server ?? local
    }
    return await fetchServerQuery(method: method, args: args)
  }

  nonisolated static func inputReportsLocalOrServer() async -> [String: [String: Any]] {
    let (mode, databasePath, profile): (ComputeMode, String, LocalHomeProfile) = await MainActor.run {
      (HealthDataStore.computeMode, HealthDataStore.defaultDatabasePath(), HealthDataStore.localComputeProfile())
    }
    if mode == .local {
      let home = await LocalHomeService.computeHome(databasePath: databasePath, profile: profile)
      let inputs = home["inputs"] as? [String: [String: Any]] ?? [:]
      if !inputs.isEmpty {
        return inputs
      }
      return await fetchServerInputReports()
    }
    return await fetchServerInputReports()
  }

  nonisolated static func scoreReportLocalOrServer(family: String) async -> [String: Any]? {
    let (mode, databasePath, profile): (ComputeMode, String, LocalHomeProfile) = await MainActor.run {
      (HealthDataStore.computeMode, HealthDataStore.defaultDatabasePath(), HealthDataStore.localComputeProfile())
    }
    if mode == .local {
      let home = await LocalHomeService.computeHome(databasePath: databasePath, profile: profile)
      if let report = home[family] as? [String: Any], !report.isEmpty {
        return report
      }
      if let report = await localLatestScoreReport(family: family, databasePath: databasePath, profile: profile) {
        return report
      }
    }
    guard let token = CoachAuthKeychain.load() else { return nil }
    return await fetchServerScoreReport(family: family, token: token)
  }

  nonisolated private static func localCalendarPayload(
    month: String,
    databasePath: String,
    profile: LocalHomeProfile
  ) async -> [CalendarDayScores] {
    guard let bounds = LocalReadCalendarBounds(month: month) else { return [] }
    let bridgeBox = LocalReadBridgeBox(BullRustBridge())
    return await Task.detached(priority: .utility) {
      localCalendarPayloadBlocking(
        bounds: bounds,
        databasePath: databasePath,
        profile: profile,
        bridge: bridgeBox.bridge
      )
    }.value
  }

  nonisolated private static func localCalendarPayloadBlocking(
    bounds: LocalReadCalendarBounds,
    databasePath: String,
    profile: LocalHomeProfile,
    bridge: BullRustBridge
  ) -> [CalendarDayScores] {
    let recoveryRows = localMetricRows(
      method: "metrics.daily_recovery_metrics",
      databasePath: databasePath,
      startMs: bounds.startMs,
      endMs: bounds.endMs,
      bridge: bridge
    )
    let activityRows = localMetricRows(
      method: "metrics.daily_activity_metrics",
      databasePath: databasePath,
      startMs: bounds.startMs,
      endMs: bounds.endMs,
      bridge: bridge
    )
    let sleepRows = localSleepRows(databasePath: databasePath, bridge: bridge)
    let exportBody = localCuratedExportBody(databasePath: databasePath, bridge: bridge)

    var recoveryByDay: [String: Double] = [:]
    var sleepByDay: [String: Double] = [:]
    var strainByDay: [String: Double] = [:]
    var stressByDay: [String: Double] = [:]

    for row in sleepRows {
      guard let day = row["date_key"] as? String, bounds.contains(day) else { continue }
      if let score = localNumber(row["score_0_to_100"]) {
        sleepByDay[day] = score
      }
    }
    applyExportScores(localArray(exportBody["sleep"]), dayKey: "day", scoreKey: "sleep_score", fallbackRawScoreKey: "score_0_to_100", bounds: bounds, into: &sleepByDay)
    applyExportScores(localArray(exportBody["recovery"]), dayKey: "day", scoreKey: "recovery_score", fallbackRawScoreKey: "score_0_to_100", bounds: bounds, into: &recoveryByDay)
    applyExportScores(localArray(exportBody["strain"]), dayKey: "day", scoreKey: "strain_score", fallbackRawScoreKey: "score_0_to_21", bounds: bounds, into: &strainByDay)
    applyExportScores(localArray(exportBody["stress"]), dayKey: "day", scoreKey: "stress_score", fallbackRawScoreKey: "score_0_to_100", bounds: bounds, into: &stressByDay)

    var candidateDays = Set<String>()
    for row in recoveryRows {
      if let day = row["date_key"] as? String, bounds.contains(day) { candidateDays.insert(day) }
    }
    for row in activityRows {
      if let day = row["date_key"] as? String, bounds.contains(day) { candidateDays.insert(day) }
    }
    for day in sleepByDay.keys where bounds.contains(day) { candidateDays.insert(day) }
    for day in recoveryByDay.keys where bounds.contains(day) { candidateDays.insert(day) }
    for day in strainByDay.keys where bounds.contains(day) { candidateDays.insert(day) }
    for day in stressByDay.keys where bounds.contains(day) { candidateDays.insert(day) }
    for day in candidateDays.sorted() {
      let dayStart = LocalReadTime.milliseconds(fromUTCDateKey: day)
      let dayEnd = dayStart + LocalReadTime.dayMilliseconds
      if recoveryRows.contains(where: { ($0["date_key"] as? String) == day }) {
        let report = try? bridge.request(
          method: "metrics.recovery_score_from_features",
          args: localRecoveryScoreArgs(databasePath: databasePath, dayStartMs: dayStart, dayEndMs: dayEnd)
        )
        recoveryByDay[day] = localScoreValue(report, key: "score_0_to_100")
      }
      if activityRows.contains(where: { ($0["date_key"] as? String) == day }) {
        let strainReport = try? bridge.request(
          method: "metrics.strain_score_from_features",
          args: localStrainScoreArgs(databasePath: databasePath, dayStartMs: dayStart, dayEndMs: dayEnd, profile: profile)
        )
        strainByDay[day] = localScoreValue(strainReport, key: "score_0_to_21")

        let stressReport = try? bridge.request(
          method: "metrics.stress_score_from_features",
          args: localStressScoreArgs(databasePath: databasePath, dayStartMs: dayStart, dayEndMs: dayEnd)
        )
        stressByDay[day] = localScoreValue(stressReport, key: "score_0_to_100")
      }
    }

    return bounds.days.map { day in
      let recovery = recoveryByDay[day]
      let sleep = sleepByDay[day]
      let strain = strainByDay[day]
      let stress = stressByDay[day]
      return CalendarDayScores(
        date: day,
        hasData: recovery != nil || sleep != nil || strain != nil || stress != nil,
        recoveryScore: recovery,
        sleepScore: sleep,
        strainScore: strain,
        stressScore: stress
      )
    }
  }

  nonisolated private static func applyExportScores(
    _ rows: [[String: Any]],
    dayKey: String,
    scoreKey: String,
    fallbackRawScoreKey: String,
    bounds: LocalReadCalendarBounds,
    into scoresByDay: inout [String: Double]
  ) {
    for row in rows {
      guard let day = row[dayKey] as? String,
            bounds.contains(day),
            scoresByDay[day] == nil else { continue }
      let raw = row["raw"] as? [String: Any]
      if let score = localNumber(row[scoreKey]) ?? localNumber(raw?[fallbackRawScoreKey]) {
        scoresByDay[day] = score
      }
    }
  }

  nonisolated private static func localMetricRows(
    method: String,
    databasePath: String,
    startMs: Int64,
    endMs: Int64,
    bridge: BullRustBridge
  ) -> [[String: Any]] {
    let result = try? bridge.request(method: method, args: [
      "database_path": databasePath,
      "start_time_unix_ms": startMs,
      "end_time_unix_ms": endMs,
    ])
    return localArray(result?["metrics"])
  }

  nonisolated private static func localSleepRows(databasePath: String, bridge: BullRustBridge) -> [[String: Any]] {
    let result = try? bridge.request(method: "sleep.list_nightly", args: [
      "database_path": databasePath,
      "limit": 400,
    ])
    return localArray(result?["nights"])
  }

  nonisolated private static func localCuratedExportBody(databasePath: String, bridge: BullRustBridge) -> [String: Any] {
    let result = try? bridge.request(method: "metrics.export_curated", args: [
      "database_path": databasePath,
      "source": "local_read",
      "sleep_limit": 400,
    ])
    return result?["body"] as? [String: Any] ?? [:]
  }

  nonisolated private static func localLatestScoreReport(
    family: String,
    databasePath: String,
    profile: LocalHomeProfile
  ) async -> [String: Any]? {
    let bridgeBox = LocalReadBridgeBox(BullRustBridge())
    return await Task.detached(priority: .utility) {
      switch family {
      case "sleep":
        return localSleepRows(databasePath: databasePath, bridge: bridgeBox.bridge)
          .sorted { ($0["date_key"] as? String ?? "") > ($1["date_key"] as? String ?? "") }
          .first
      case "recovery":
        guard let day = latestMetricDay(
          method: "metrics.daily_recovery_metrics",
          databasePath: databasePath,
          bridge: bridgeBox.bridge
        ) else { return nil }
        let dayStart = LocalReadTime.milliseconds(fromUTCDateKey: day)
        let dayEnd = dayStart + LocalReadTime.dayMilliseconds
        return try? bridgeBox.bridge.request(
          method: "metrics.recovery_score_from_features",
          args: localRecoveryScoreArgs(databasePath: databasePath, dayStartMs: dayStart, dayEndMs: dayEnd)
        )
      case "strain":
        guard let day = latestMetricDay(
          method: "metrics.daily_activity_metrics",
          databasePath: databasePath,
          bridge: bridgeBox.bridge
        ) else { return nil }
        let dayStart = LocalReadTime.milliseconds(fromUTCDateKey: day)
        let dayEnd = dayStart + LocalReadTime.dayMilliseconds
        return try? bridgeBox.bridge.request(
          method: "metrics.strain_score_from_features",
          args: localStrainScoreArgs(databasePath: databasePath, dayStartMs: dayStart, dayEndMs: dayEnd, profile: profile)
        )
      case "stress":
        guard let day = latestMetricDay(
          method: "metrics.daily_activity_metrics",
          databasePath: databasePath,
          bridge: bridgeBox.bridge
        ) else { return nil }
        let dayStart = LocalReadTime.milliseconds(fromUTCDateKey: day)
        let dayEnd = dayStart + LocalReadTime.dayMilliseconds
        return try? bridgeBox.bridge.request(
          method: "metrics.stress_score_from_features",
          args: localStressScoreArgs(databasePath: databasePath, dayStartMs: dayStart, dayEndMs: dayEnd)
        )
      default:
        return nil
      }
    }.value
  }

  nonisolated private static func latestMetricDay(method: String, databasePath: String, bridge: BullRustBridge) -> String? {
    localMetricRows(
      method: method,
      databasePath: databasePath,
      startMs: 0,
      endMs: Int64.max,
      bridge: bridge
    )
    .compactMap { $0["date_key"] as? String }
    .sorted()
    .last
  }

  nonisolated private static func localReadQuery(method: String, args: [String: Any], databasePath: String) async -> [String: Any]? {
    let bridgeBox = LocalReadBridgeBox(BullRustBridge())
    return await Task.detached(priority: .utility) {
      var localArgs = args
      localArgs["database_path"] = databasePath
      guard var result = try? bridgeBox.bridge.request(method: method, args: localArgs) else {
        return nil
      }
      if method == "biometrics.stream_summary" {
        enrichLocalBiometricSummary(&result, bridge: bridgeBox.bridge)
      }
      return result
    }.value
  }

  nonisolated private static func enrichLocalBiometricSummary(_ result: inout [String: Any], bridge: BullRustBridge) {
    guard let red = localUInt16(result["latest_spo2_red"]),
          let ir = localUInt16(result["latest_spo2_ir"]),
          let converted = try? bridge.request(method: "biometrics.spo2_from_raw", args: ["red": Int(red), "ir": Int(ir)]) else {
      return
    }
    result["latest_spo2_pct"] = converted["spo2_pct"] ?? NSNull()
  }

  nonisolated private static func isAllowedLocalReadQuery(_ method: String) -> Bool {
    switch method {
    case "sleep.list_nightly",
         "biometrics.stream_summary",
         "activity.list_sessions",
         "activity.list_metrics":
      return true
    default:
      return false
    }
  }

  nonisolated private static func localReadResultHasData(_ result: [String: Any]) -> Bool {
    guard !result.isEmpty else { return false }
    var sawExplicitEmptyCollectionOrCount = false
    for (key, value) in result {
      if let array = value as? [Any] {
        sawExplicitEmptyCollectionOrCount = true
        if !array.isEmpty { return true }
      }
      if key == "count" || key.hasSuffix("_count") {
        sawExplicitEmptyCollectionOrCount = true
        if let count = localNumber(value), count > 0 { return true }
      }
    }
    return !sawExplicitEmptyCollectionOrCount
  }

  nonisolated private static func localRecoveryScoreArgs(databasePath: String, dayStartMs: Int64, dayEndMs: Int64) -> [String: Any] {
    let historyStart = dayStartMs - 14 * LocalReadTime.dayMilliseconds
    return [
      "database_path": databasePath,
      "start": LocalReadTime.isoString(fromMilliseconds: historyStart),
      "end": LocalReadTime.isoString(fromMilliseconds: dayEndMs),
      "hrv_start": LocalReadTime.isoString(fromMilliseconds: dayStartMs),
      "hrv_end": LocalReadTime.isoString(fromMilliseconds: dayEndMs),
      "hrv_baseline_start": LocalReadTime.isoString(fromMilliseconds: historyStart),
      "hrv_baseline_end": LocalReadTime.isoString(fromMilliseconds: dayEndMs),
      "resting_start": LocalReadTime.isoString(fromMilliseconds: historyStart),
      "resting_end": LocalReadTime.isoString(fromMilliseconds: dayEndMs),
      "sleep_start": LocalReadTime.isoString(fromMilliseconds: dayStartMs - LocalReadTime.dayMilliseconds),
      "sleep_end": LocalReadTime.isoString(fromMilliseconds: dayEndMs),
      "prior_strain_start": LocalReadTime.isoString(fromMilliseconds: dayStartMs - LocalReadTime.dayMilliseconds),
      "prior_strain_end": LocalReadTime.isoString(fromMilliseconds: dayStartMs),
      "min_owned_captures": 2,
      "require_trusted_evidence": false,
      "resting_baseline_min_days": 3,
      "hrv_min_rr_intervals_to_compute": 2,
      "hrv_baseline_min_days": 3,
      "sleep_need_minutes": 480.0,
      "low_motion_threshold_0_to_1": 0.05,
      "disturbance_motion_threshold_0_to_1": 0.2,
      "target_midpoint_minutes_since_midnight": 180.0,
      "prior_strain_resting_baseline_min_days": 3,
    ]
  }

  nonisolated private static func localStrainScoreArgs(
    databasePath: String,
    dayStartMs: Int64,
    dayEndMs: Int64,
    profile: LocalHomeProfile
  ) -> [String: Any] {
    var args: [String: Any] = [
      "database_path": databasePath,
      "start": LocalReadTime.isoString(fromMilliseconds: dayStartMs),
      "end": LocalReadTime.isoString(fromMilliseconds: dayEndMs),
      "resting_start": LocalReadTime.isoString(fromMilliseconds: dayStartMs - 14 * LocalReadTime.dayMilliseconds),
      "resting_end": LocalReadTime.isoString(fromMilliseconds: dayEndMs),
      "min_owned_captures": 2,
      "require_trusted_evidence": false,
      "resting_baseline_min_days": 3,
    ]
    if let age = profile.ageYears, age >= 13, age <= 120 {
      args["max_hr_bpm"] = max(120.0, min(210.0, 208.0 - 0.7 * Double(age)))
    }
    return args
  }

  nonisolated private static func localStressScoreArgs(databasePath: String, dayStartMs: Int64, dayEndMs: Int64) -> [String: Any] {
    let historyStart = dayStartMs - 14 * LocalReadTime.dayMilliseconds
    return [
      "database_path": databasePath,
      "start": LocalReadTime.isoString(fromMilliseconds: dayStartMs),
      "end": LocalReadTime.isoString(fromMilliseconds: dayEndMs),
      "resting_start": LocalReadTime.isoString(fromMilliseconds: historyStart),
      "resting_end": LocalReadTime.isoString(fromMilliseconds: dayEndMs),
      "hrv_start": LocalReadTime.isoString(fromMilliseconds: dayStartMs),
      "hrv_end": LocalReadTime.isoString(fromMilliseconds: dayEndMs),
      "hrv_baseline_start": LocalReadTime.isoString(fromMilliseconds: historyStart),
      "hrv_baseline_end": LocalReadTime.isoString(fromMilliseconds: dayEndMs),
      "min_owned_captures": 2,
      "require_trusted_evidence": false,
      "resting_baseline_min_days": 3,
      "hrv_min_rr_intervals_to_compute": 2,
      "hrv_baseline_min_days": 3,
    ]
  }

  nonisolated private static func localScoreValue(_ report: [String: Any]?, key: String) -> Double? {
    guard let output = (report?["score_result"] as? [String: Any])?["output"] as? [String: Any] else {
      return nil
    }
    return localNumber(output[key])
  }

  nonisolated private static func localArray(_ value: Any?) -> [[String: Any]] {
    value as? [[String: Any]] ?? []
  }

  nonisolated private static func localNumber(_ value: Any?) -> Double? {
    switch value {
    case let value as Double where value.isFinite:
      return value
    case let value as Float where value.isFinite:
      return Double(value)
    case let value as Int:
      return Double(value)
    case let value as Int64:
      return Double(value)
    case let value as NSNumber:
      let double = value.doubleValue
      return double.isFinite ? double : nil
    default:
      return nil
    }
  }

  nonisolated private static func localUInt16(_ value: Any?) -> UInt16? {
    guard let number = localNumber(value), number >= 0, number <= Double(UInt16.max) else {
      return nil
    }
    return UInt16(number)
  }
}

private struct LocalReadCalendarBounds {
  let month: String
  let days: [String]
  let startMs: Int64
  let endMs: Int64

  init?(month: String) {
    let parts = month.split(separator: "-").compactMap { Int($0) }
    guard parts.count == 2, let year = parts.first, let monthNumber = parts.last,
          (1...12).contains(monthNumber) else {
      return nil
    }
    var calendar = Calendar(identifier: .gregorian)
    calendar.locale = Locale(identifier: "en_US_POSIX")
    calendar.timeZone = TimeZone(secondsFromGMT: 0)!
    var components = DateComponents()
    components.calendar = calendar
    components.timeZone = calendar.timeZone
    components.year = year
    components.month = monthNumber
    components.day = 1
    guard let start = calendar.date(from: components),
          let end = calendar.date(byAdding: .month, value: 1, to: start),
          let dayRange = calendar.range(of: .day, in: .month, for: start) else {
      return nil
    }
    self.month = String(format: "%04d-%02d", year, monthNumber)
    self.days = dayRange.map { String(format: "%04d-%02d-%02d", year, monthNumber, $0) }
    self.startMs = LocalReadTime.milliseconds(from: start)
    self.endMs = LocalReadTime.milliseconds(from: end)
  }

  func contains(_ day: String) -> Bool {
    day.hasPrefix("\(month)-")
  }
}

private enum LocalReadTime {
  static let dayMilliseconds: Int64 = 86_400_000

  static func milliseconds(from date: Date) -> Int64 {
    Int64((date.timeIntervalSince1970 * 1_000).rounded())
  }

  static func milliseconds(fromUTCDateKey day: String) -> Int64 {
    let parts = day.split(separator: "-").compactMap { Int($0) }
    guard parts.count == 3 else { return 0 }
    var calendar = Calendar(identifier: .gregorian)
    calendar.locale = Locale(identifier: "en_US_POSIX")
    calendar.timeZone = TimeZone(secondsFromGMT: 0)!
    var components = DateComponents()
    components.calendar = calendar
    components.timeZone = calendar.timeZone
    components.year = parts[0]
    components.month = parts[1]
    components.day = parts[2]
    return milliseconds(from: calendar.date(from: components) ?? Date(timeIntervalSince1970: 0))
  }

  static func isoString(fromMilliseconds milliseconds: Int64) -> String {
    let formatter = DateFormatter()
    formatter.calendar = Calendar(identifier: .gregorian)
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.timeZone = TimeZone(secondsFromGMT: 0)
    formatter.dateFormat = "yyyy-MM-dd'T'HH:mm:ss.SSS'Z'"
    let date = Date(timeIntervalSince1970: TimeInterval(milliseconds) / 1_000)
    return formatter.string(from: date)
  }
}

private final class LocalReadBridgeBox: @unchecked Sendable {
  let bridge: BullRustBridge

  init(_ bridge: BullRustBridge) {
    self.bridge = bridge
  }
}
