import Foundation

struct LocalHomeProfile {
  var weightKg: Double?
  var heightCm: Double?
  var ageYears: Int?
  var sex: String?          // "male" | "female" | nil
  var timezone: String      // IANA, e.g. TimeZone.current.identifier
}

enum LocalHomeService {
  private static let storeRetentionDays = 5
  private static let dayMilliseconds: Int64 = 86_400_000

  /// Per-metric analysis windows. Each metric is judged against the window that is
  /// physiologically meaningful for it, not a single blanket window. Baselines are
  /// folded from persisted nightly summaries, so a longer baseline window stays cheap.
  enum ScoreWindows {
    /// Personal HRV / resting-HR baseline is set against roughly the last month of
    /// nights, long enough to be stable and to follow genuine physiological drift.
    static let recoveryBaselineDays = 30
    /// Minimum nights before a personal baseline is trusted rather than calibrating.
    static let recoveryBaselineMinDays = 3
    /// Days of data needed before a strain target is meaningful.
    static let strainCalibrationDays = 4
    /// Consecutive nights needed before sleep guidance is personalized.
    static let sleepCalibrationNights = 3
    /// Trend comparison windows (short vs long rolling average).
    static let trendShortDays = 30
    static let trendLongDays = 90
  }
  private static let hourMilliseconds: Int64 = 3_600_000
  private static let localBiometricDeviceID = "bull-local"
  private static let inputBiometricDeviceID = "bull.device.local.v1"

  /// Runs Bull's local-first, on-device replacement for the server `/v1/data/home`
  /// compute path. Synchronous Rust bridge work is moved off the caller's actor;
  /// physiological data is processed locally from the device-backed SQLite store.
  static func computeHome(
    databasePath: String,
    profile: LocalHomeProfile,
    bridge: BullRustBridge = BullRustBridge(),
    now: Date = Date()
  ) async -> [String: Any] {
    let box = BridgeBox(bridge)
    return await Task.detached(priority: .utility) {
      computeHomeBlocking(
        databasePath: databasePath,
        profile: profile,
        bridge: box.bridge,
        now: now
      )
    }.value
  }

  private static func computeHomeBlocking(
    databasePath: String,
    profile: LocalHomeProfile,
    bridge: BullRustBridge,
    now: Date
  ) -> [String: Any] {
    let dayKeys = recentUTCDayKeys(now: now)

    for day in dayKeys {
      let windows = pipelineWindows(day: day)
      let featureStart = isoString(fromMilliseconds: windows.daily.startMs - Int64(storeRetentionDays) * dayMilliseconds)
      _ = try? bridge.request(method: "metrics.run_pipeline", args: [
        "database_path": databasePath,
        "device_id": localBiometricDeviceID,
        "daily_window": windows.daily.bridgeObject,
        "hourly_window": windows.hourly.bridgeObject,
        "feature_window_start_iso": featureStart,
        "skip_feature_passes": false,
        "skip_step_discovery": true,
      ])
    }

    let sleepReports = computeSleepReports(
      databasePath: databasePath,
      profile: profile,
      bridge: bridge,
      dayKeys: dayKeys,
      now: now
    )

    let exported = (try? bridge.request(method: "metrics.export_curated", args: [
      "database_path": databasePath,
      "source": "local_compute",
    ])) ?? [:]
    let body = exported["body"] as? [String: Any] ?? [:]
    let vitalsByDay = mergedVitalsByDay(from: body["vitals"] as? [[String: Any]] ?? [])
    let sortedVitalsDays = vitalsByDay.keys.sorted()

    var recoveryReportsByDay: [String: [String: Any]] = [:]
    var strainReportsByDay: [String: [String: Any]] = [:]
    var stressReportsByDay: [String: [String: Any]] = [:]
    var recoveryDaily: [[String: Any]] = []
    var strainDaily: [[String: Any]] = []

    for day in sortedVitalsDays {
      let dayStart = "\(day)T00:00:00Z"
      let dayEnd = "\(day)T23:59:59Z"
      var recoveryArgs = scoreArgs(now: now)
      recoveryArgs["database_path"] = databasePath
      recoveryArgs["date_key"] = day
      recoveryArgs["hrv_start"] = dayStart
      recoveryArgs["hrv_end"] = dayEnd
      recoveryArgs["sleep_start"] = isoString(fromMilliseconds: milliseconds(fromUTCStartOfDayKey: day) - dayMilliseconds)
      recoveryArgs["sleep_end"] = dayEnd
      recoveryArgs["prior_strain_start"] = isoString(fromMilliseconds: milliseconds(fromUTCStartOfDayKey: day) - dayMilliseconds)
      recoveryArgs["prior_strain_end"] = dayStart

      if let report = try? bridge.request(method: "metrics.recovery_score_from_features", args: recoveryArgs) {
        recoveryReportsByDay[day] = report
        if let score = scoreValue(report) {
          recoveryDaily.append(["day": day, "score_0_to_100": score])
        }
      }

      var strainArgs = scoreArgs(now: now)
      strainArgs["database_path"] = databasePath
      strainArgs["date_key"] = day
      strainArgs["start"] = dayStart
      strainArgs["end"] = dayEnd
      if let report = try? bridge.request(method: "metrics.strain_score_from_features", args: strainArgs) {
        strainReportsByDay[day] = report
        if let score = scoreValue(report) {
          strainDaily.append(["day": day, "score_0_to_21": score])
        }
      }

      var stressArgs = scoreArgs(now: now)
      stressArgs["database_path"] = databasePath
      stressArgs["date_key"] = day
      stressArgs["start"] = dayStart
      stressArgs["end"] = dayEnd
      if let report = try? bridge.request(method: "metrics.stress_score_from_features", args: stressArgs) {
        stressReportsByDay[day] = report
      }
    }

    var sleep = latestSleepReport(from: sleepReports) ?? latestExportSleep(from: body["sleep"] as? [[String: Any]] ?? []) ?? [:]
    let sleepDaily = sleepReports
      .sorted { $0.day > $1.day }
      .compactMap { entry -> [String: Any]? in
        guard let score = scoreValue(entry.report) else { return nil }
        return ["day": entry.day, "score_0_to_100": score, "sleep_duration_minutes": NSNull()]
      }
    if !sleep.isEmpty {
      sleep["daily"] = sleepDaily
    }

    var recovery = latestReport(from: recoveryReportsByDay) ?? [:]
    if !recovery.isEmpty {
      recovery["daily"] = recoveryDaily.sorted { ($0["day"] as? String ?? "") > ($1["day"] as? String ?? "") }
    }

    var strain = latestReport(from: strainReportsByDay) ?? [:]
    if !strain.isEmpty {
      strain["daily"] = strainDaily.sorted { ($0["day"] as? String ?? "") > ($1["day"] as? String ?? "") }
    }

    let stress = latestReport(from: stressReportsByDay) ?? [:]
    let inputs = computeInputReports(
      databasePath: databasePath,
      profile: profile,
      bridge: bridge,
      now: now
    )
    let energy = inputs["energy_rollup"] ?? [:]
    let vitals = sortedVitalsDays.last.flatMap { vitalsByDay[$0] } ?? [:]

    return [
      "recovery": recovery,
      "sleep": sleep,
      "strain": strain,
      "stress": stress,
      "energy": energy,
      "vitals": vitals,
      "inputs": inputs,
    ]
  }

  private static func computeSleepReports(
    databasePath: String,
    profile: LocalHomeProfile,
    bridge: BullRustBridge,
    dayKeys: [String],
    now: Date
  ) -> [DatedReport] {
    dayKeys.compactMap { day in
      let dayMs = milliseconds(fromUTCStartOfDayKey: day)
      var args = sleepScoreArgs(now: now)
      args["database_path"] = databasePath
      args["start"] = isoString(fromMilliseconds: dayMs - 12 * hourMilliseconds)
      args["end"] = isoString(fromMilliseconds: dayMs + 12 * hourMilliseconds)
      if let offset = utcOffsetMinutes(timezoneIdentifier: profile.timezone, atMilliseconds: dayMs) {
        args["night_gate_utc_offset_minutes"] = offset
      }
      guard let report = try? bridge.request(method: "metrics.sleep_score_from_features", args: args) else {
        return nil
      }
      return DatedReport(day: day, report: report)
    }
  }

  private static func computeInputReports(
    databasePath: String,
    profile: LocalHomeProfile,
    bridge: BullRustBridge,
    now: Date
  ) -> [String: [String: Any]] {
    let daily = dailyWindow(now: now, timezoneIdentifier: profile.timezone)
    let hourly = hourlyWindow(now: now, timezoneIdentifier: profile.timezone)
    let energyProfileArgs = profileEnergyArgs(profile)
    let base: [String: Any] = [
      "database_path": databasePath,
      "start": "0000",
      "end": "9999",
      "min_owned_captures": 2,
      "require_trusted_evidence": false,
    ]
    var reports: [String: [String: Any]] = [:]

    func call(_ key: String, _ method: String, _ args: [String: Any]) {
      do {
        reports[key] = try bridge.request(method: method, args: args)
      } catch {
        reports[key] = ["error": errorMessage(error)]
      }
    }

    call("readiness", "metrics.input_readiness", [
      "database_path": databasePath,
      "start": "0000",
      "end": "9999",
      "min_owned_captures": 2,
      "require_owned_captures": false,
      "require_scores_ready": true,
    ])
    call("motion", "metrics.motion_features", base)
    call("step_discovery", "metrics.step_packet_discovery", merging(base, [
      "max_candidate_fields": 100,
    ]))
    call("step_counter_ingest", "metrics.step_counter_ingest", merging(base, [
      "max_candidate_fields": 1_000,
    ]))
    call("raw_motion_step_estimate", "metrics.raw_motion_step_estimate", [
      "database_path": databasePath,
      "start": daily.startISO,
      "end": daily.endISO,
      "min_owned_captures": 2,
      "require_trusted_evidence": false,
      "date_key": daily.dateKey,
      "timezone": daily.timezone,
      "write_metric": true,
    ])
    call("biometric_ingest", "biometrics.ingest_from_decoded", [
      "database_path": databasePath,
      "device_id": inputBiometricDeviceID,
      "start": "0000",
      "end": "9999",
    ])
    call("heart_rate", "metrics.heart_rate_features", base)
    call("vital_event", "metrics.vital_event_features", base)
    call("hrv", "metrics.hrv_features", merging(base, [
      "min_rr_intervals_to_compute": 2,
      "baseline_min_days": 3,
      "require_baseline": false,
    ]))
    call("window", "metrics.window_features", base)
    call("resting_hr", "metrics.resting_hr_features", merging(base, [
      "baseline_min_days": 3,
      "require_baseline": false,
    ]))

    call("resting_hr_rollup", "metrics.resting_hr_daily_rollup", [
      "database_path": databasePath,
      "date_key": daily.dateKey,
      "timezone": daily.timezone,
      "start": daily.startISO,
      "end": daily.endISO,
      "min_owned_captures": 2,
      "require_trusted_evidence": false,
      "baseline_min_days": 3,
      "require_baseline": false,
      "min_sample_count": 2,
      "write_metric": true,
    ])
    let restingHrBpm = numberValue(reports["resting_hr_rollup"]?["resting_hr_bpm"])

    call("step_counter_rollup", "metrics.step_counter_daily_rollup", [
      "database_path": databasePath,
      "date_key": daily.dateKey,
      "timezone": daily.timezone,
      "start_time_unix_ms": daily.startMs,
      "end_time_unix_ms": daily.endMs,
      "min_sample_count": 2,
      "write_metric": true,
    ])
    call("step_counter_hourly_rollup", "metrics.step_counter_hourly_rollup", [
      "database_path": databasePath,
      "date_key": hourly.dateKey,
      "timezone": hourly.timezone,
      "start_time_unix_ms": hourly.startMs,
      "end_time_unix_ms": hourly.endMs,
      "min_sample_count": 2,
      "write_metric": true,
    ])
    call("activity_unavailable_status", "metrics.activity_unavailable_daily_status", [
      "database_path": databasePath,
      "date_key": daily.dateKey,
      "timezone": daily.timezone,
      "start_time_unix_ms": daily.startMs,
      "end_time_unix_ms": daily.endMs,
      "min_sample_count": 2,
      "write_metric": true,
    ])

    var energyDailyArgs = merging([
      "database_path": databasePath,
      "date_key": daily.dateKey,
      "timezone": daily.timezone,
      "start": daily.startISO,
      "end": daily.endISO,
      "min_owned_captures": 2,
      "require_trusted_evidence": false,
      "min_heart_rate_samples": 2,
      "write_metric": true,
    ], energyProfileArgs)
    if let restingHrBpm {
      energyDailyArgs["resting_hr_bpm"] = restingHrBpm
    }
    call("energy_rollup", "metrics.energy_daily_rollup", energyDailyArgs)

    var energyHourlyArgs = merging([
      "database_path": databasePath,
      "date_key": hourly.dateKey,
      "timezone": hourly.timezone,
      "start": hourly.startISO,
      "end": hourly.endISO,
      "min_owned_captures": 2,
      "require_trusted_evidence": false,
      "min_heart_rate_samples": 2,
      "write_metric": true,
    ], energyProfileArgs)
    if let restingHrBpm {
      energyHourlyArgs["resting_hr_bpm"] = restingHrBpm
    }
    call("energy_hourly_rollup", "metrics.energy_hourly_rollup", energyHourlyArgs)
    call("energy_unavailable_status", "metrics.energy_unavailable_daily_status", energyDailyArgs)

    let recoveryStatusArgs: [String: Any] = [
      "database_path": databasePath,
      "date_key": daily.dateKey,
      "timezone": daily.timezone,
      "start": daily.startISO,
      "end": daily.endISO,
      "min_owned_captures": 2,
      "require_trusted_evidence": false,
      "min_rr_intervals_to_compute": 2,
      "write_metric": true,
    ]
    call("recovery_sensor_rollup", "metrics.recovery_sensor_daily_rollup", recoveryStatusArgs)
    call("recovery_unavailable_status", "metrics.recovery_unavailable_daily_status", recoveryStatusArgs)

    let dailyHistoryStartMs = daily.startMs - 29 * dayMilliseconds
    call("daily_activity", "metrics.daily_activity_metrics", [
      "database_path": databasePath,
      "start_time_unix_ms": dailyHistoryStartMs,
      "end_time_unix_ms": daily.endMs,
    ])
    call("hourly_activity", "metrics.hourly_activity_metrics", [
      "database_path": databasePath,
      "start_time_unix_ms": hourly.startMs - 48 * hourMilliseconds,
      "end_time_unix_ms": hourly.endMs,
    ])
    call("daily_recovery", "metrics.daily_recovery_metrics", [
      "database_path": databasePath,
      "start_time_unix_ms": dailyHistoryStartMs,
      "end_time_unix_ms": daily.endMs,
    ])

    return reports
  }

  private static func sleepScoreArgs(now: Date) -> [String: Any] {
    let start = isoString(fromMilliseconds: milliseconds(from: now) - 14 * dayMilliseconds)
    return [
      "start": start,
      "end": "9999",
      "min_owned_captures": 2,
      "require_trusted_evidence": false,
      "sleep_need_minutes": 480.0,
      "low_motion_threshold_0_to_1": 0.05,
      "disturbance_motion_threshold_0_to_1": 0.2,
      "target_midpoint_minutes_since_midnight": 180.0,
      "history_import_in_progress": false,
      "algorithm_id": "bull.sleep.v1",
      "persist_nightly": true,
    ]
  }

  private static func scoreArgs(now: Date) -> [String: Any] {
    let start = isoString(fromMilliseconds: milliseconds(from: now) - 14 * dayMilliseconds)
    // The personal baseline is set against a longer window than the current-value
    // window; it folds cheap persisted nightly summaries, so reaching back a month
    // costs nothing and yields a more stable, drift-aware baseline.
    let baselineStart = isoString(
      fromMilliseconds: milliseconds(from: now)
        - Int64(ScoreWindows.recoveryBaselineDays) * dayMilliseconds)
    return [
      "start": start,
      "end": "9999",
      "min_owned_captures": 2,
      "require_trusted_evidence": false,
      "hrv_start": start,
      "hrv_end": "9999",
      "hrv_baseline_start": baselineStart,
      "hrv_baseline_end": "9999",
      "resting_start": baselineStart,
      "resting_end": "9999",
      "sleep_start": start,
      "sleep_end": "9999",
      "prior_strain_start": start,
      "prior_strain_end": "9999",
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

  private static func profileEnergyArgs(_ profile: LocalHomeProfile) -> [String: Any] {
    var args: [String: Any] = [:]
    if let weight = profile.weightKg, weight >= 25, weight <= 300 {
      args["profile_weight_kg"] = weight
    }
    if let age = profile.ageYears, age >= 13, age <= 120 {
      args["profile_age_years"] = age
      args["max_hr_bpm"] = max(120.0, min(210.0, 208.0 - 0.7 * Double(age)))
    }
    if profile.sex == "male" || profile.sex == "female" {
      args["profile_sex"] = profile.sex
    }
    return args
  }

  private static func scoreValue(_ report: [String: Any]) -> Double? {
    guard let output = nestedDictionary(report, "score_result", "output") else { return nil }
    for key in ["score_0_to_100", "score_0_to_21"] {
      if let value = numberValue(output[key]) {
        return value
      }
    }
    return nil
  }

  private static func latestSleepReport(from reports: [DatedReport]) -> [String: Any]? {
    let sorted = reports.sorted { $0.day > $1.day }
    return sorted.first { report in
      boolValue(report.report["nightly_sleep_persisted"]) == true
        || report.report["nightly_sleep_window"] is [String: Any]
    }?.report ?? sorted.first?.report
  }

  private static func latestExportSleep(from rows: [[String: Any]]) -> [String: Any]? {
    rows
      .filter { $0["day"] as? String != nil }
      .sorted { ($0["day"] as? String ?? "") > ($1["day"] as? String ?? "") }
      .first
  }

  private static func latestReport(from reportsByDay: [String: [String: Any]]) -> [String: Any]? {
    reportsByDay.keys.sorted().last.flatMap { reportsByDay[$0] }
  }

  private static func mergedVitalsByDay(from rows: [[String: Any]]) -> [String: [String: Any]] {
    var byDay: [String: [String: Any]] = [:]
    for row in rows {
      guard let day = row["day"] as? String, !day.isEmpty else { continue }
      var merged = byDay[day] ?? ["day": day]
      for (key, value) in row where key != "raw" && !(value is NSNull) {
        merged[key] = value
      }
      byDay[day] = merged
    }
    return byDay
  }

  private static func recentUTCDayKeys(now: Date) -> [String] {
    let todayStartMs = utcStartOfDayMilliseconds(for: now)
    var days: [String] = []
    for offset in stride(from: storeRetentionDays, through: 0, by: -1) {
      days.append(dayKey(fromMilliseconds: todayStartMs - Int64(offset) * dayMilliseconds))
    }
    return days
  }

  private static func pipelineWindows(day: String) -> (daily: MetricWindow, hourly: MetricWindow) {
    let dayStartMs = milliseconds(fromUTCStartOfDayKey: day)
    let daily = MetricWindow(
      dateKey: day,
      timezone: "UTC",
      startMs: dayStartMs,
      endMs: dayStartMs + dayMilliseconds
    )
    let hourly = MetricWindow(
      dateKey: day,
      timezone: "UTC",
      startMs: dayStartMs,
      endMs: dayStartMs + hourMilliseconds
    )
    return (daily, hourly)
  }

  private static func dailyWindow(now: Date, timezoneIdentifier: String) -> MetricWindow {
    var calendar = Calendar(identifier: .gregorian)
    calendar.locale = Locale(identifier: "en_US_POSIX")
    let resolvedTimezoneIdentifier = TimeZone(identifier: timezoneIdentifier) == nil ? "UTC" : timezoneIdentifier
    calendar.timeZone = TimeZone(identifier: resolvedTimezoneIdentifier) ?? TimeZone(secondsFromGMT: 0)!
    let start = calendar.startOfDay(for: now)
    let end = calendar.date(byAdding: .day, value: 1, to: start) ?? start.addingTimeInterval(86_400)
    return MetricWindow(
      dateKey: localDayKey(for: start, calendar: calendar),
      timezone: resolvedTimezoneIdentifier,
      startMs: milliseconds(from: start),
      endMs: milliseconds(from: end)
    )
  }

  private static func hourlyWindow(now: Date, timezoneIdentifier: String) -> MetricWindow {
    var calendar = Calendar(identifier: .gregorian)
    calendar.locale = Locale(identifier: "en_US_POSIX")
    let resolvedTimezoneIdentifier = TimeZone(identifier: timezoneIdentifier) == nil ? "UTC" : timezoneIdentifier
    calendar.timeZone = TimeZone(identifier: resolvedTimezoneIdentifier) ?? TimeZone(secondsFromGMT: 0)!
    let components = calendar.dateComponents([.year, .month, .day, .hour], from: now)
    let start = calendar.date(from: components) ?? now
    let end = calendar.date(byAdding: .hour, value: 1, to: start) ?? start.addingTimeInterval(3_600)
    return MetricWindow(
      dateKey: localDayKey(for: start, calendar: calendar),
      timezone: resolvedTimezoneIdentifier,
      startMs: milliseconds(from: start),
      endMs: milliseconds(from: end)
    )
  }

  private static func utcOffsetMinutes(timezoneIdentifier: String, atMilliseconds milliseconds: Int64) -> Int? {
    guard let timeZone = TimeZone(identifier: timezoneIdentifier) else { return nil }
    let date = Date(timeIntervalSince1970: TimeInterval(milliseconds) / 1_000)
    return timeZone.secondsFromGMT(for: date) / 60
  }

  private static func localDayKey(for date: Date, calendar: Calendar) -> String {
    let components = calendar.dateComponents([.year, .month, .day], from: date)
    return String(format: "%04d-%02d-%02d", components.year ?? 0, components.month ?? 0, components.day ?? 0)
  }

  private static func dayKey(fromMilliseconds milliseconds: Int64) -> String {
    String(isoString(fromMilliseconds: milliseconds).prefix(10))
  }

  private static func utcStartOfDayMilliseconds(for date: Date) -> Int64 {
    var calendar = Calendar(identifier: .gregorian)
    calendar.locale = Locale(identifier: "en_US_POSIX")
    calendar.timeZone = TimeZone(secondsFromGMT: 0)!
    return milliseconds(from: calendar.startOfDay(for: date))
  }

  private static func milliseconds(fromUTCStartOfDayKey day: String) -> Int64 {
    var components = DateComponents()
    components.calendar = Calendar(identifier: .gregorian)
    components.timeZone = TimeZone(secondsFromGMT: 0)
    let parts = day.split(separator: "-").compactMap { Int($0) }
    guard parts.count == 3 else { return 0 }
    components.year = parts[0]
    components.month = parts[1]
    components.day = parts[2]
    return milliseconds(from: components.date ?? Date(timeIntervalSince1970: 0))
  }

  private static func milliseconds(from date: Date) -> Int64 {
    Int64((date.timeIntervalSince1970 * 1_000).rounded())
  }

  private static func isoString(fromMilliseconds milliseconds: Int64) -> String {
    let date = Date(timeIntervalSince1970: TimeInterval(milliseconds) / 1_000)
    return isoFormatter.string(from: date)
  }

  private static let isoFormatter: DateFormatter = {
    let formatter = DateFormatter()
    formatter.calendar = Calendar(identifier: .gregorian)
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.timeZone = TimeZone(secondsFromGMT: 0)
    formatter.dateFormat = "yyyy-MM-dd'T'HH:mm:ss.SSS'Z'"
    return formatter
  }()

  private static func merging(_ lhs: [String: Any], _ rhs: [String: Any]) -> [String: Any] {
    var result = lhs
    for (key, value) in rhs {
      result[key] = value
    }
    return result
  }

  private static func nestedDictionary(_ dictionary: [String: Any], _ path: String...) -> [String: Any]? {
    var current: Any = dictionary
    for key in path {
      guard let object = current as? [String: Any], let next = object[key] else { return nil }
      current = next
    }
    return current as? [String: Any]
  }

  private static func numberValue(_ value: Any?) -> Double? {
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
      return value.doubleValue.isFinite ? value.doubleValue : nil
    default:
      return nil
    }
  }

  private static func boolValue(_ value: Any?) -> Bool? {
    switch value {
    case let value as Bool:
      return value
    case let value as NSNumber:
      return value.boolValue
    default:
      return nil
    }
  }

  private static func errorMessage(_ error: Error) -> String {
    if case BullRustBridgeError.methodFailed(let message) = error {
      return message
    }
    return String(describing: error)
  }
}

private struct MetricWindow {
  let dateKey: String
  let timezone: String
  let startMs: Int64
  let endMs: Int64

  var startISO: String { LocalHomeServiceWindowISO.string(fromMilliseconds: startMs) }
  var endISO: String { LocalHomeServiceWindowISO.string(fromMilliseconds: endMs) }

  var bridgeObject: [String: Any] {
    [
      "date_key": dateKey,
      "timezone": timezone,
      "start_iso": startISO,
      "end_iso": endISO,
      "start_time": startISO,
      "end_time": endISO,
      "start_time_unix_ms": startMs,
      "end_time_unix_ms": endMs,
    ]
  }
}

private enum LocalHomeServiceWindowISO {
  static func string(fromMilliseconds milliseconds: Int64) -> String {
    let date = Date(timeIntervalSince1970: TimeInterval(milliseconds) / 1_000)
    return formatter.string(from: date)
  }

  private static let formatter: DateFormatter = {
    let formatter = DateFormatter()
    formatter.calendar = Calendar(identifier: .gregorian)
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.timeZone = TimeZone(secondsFromGMT: 0)
    formatter.dateFormat = "yyyy-MM-dd'T'HH:mm:ss.SSS'Z'"
    return formatter
  }()
}

private struct DatedReport {
  let day: String
  let report: [String: Any]
}

private final class BridgeBox: @unchecked Sendable {
  let bridge: BullRustBridge

  init(_ bridge: BullRustBridge) {
    self.bridge = bridge
  }
}
