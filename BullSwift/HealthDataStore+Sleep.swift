import Darwin
import Foundation
import SwiftUI
import UIKit

extension HealthDataStore {
  func refreshPrimarySleepFromScoreReport() {
    guard let detail = Self.primarySleepDetail(fromSleepReport: packetScoreReports["sleep"]) else {
      return
    }
    primarySleepDetail = detail
  }

  /// One-shot read-only diagnostic: ask the bridge for a compact DB overview
  /// (table counts, on-disk size, decoded-frame packet distribution, and the
  /// sleep report's blocking reasons) and write it as a small JSON file next to
  /// the database so it can be pulled off-device without exporting the store.
  func writeDebugOverview() {
    let bridge = self.bridge
    let databasePath = self.databasePath
    packetInputQueue.async {
      guard let report = try? bridge.request(
        method: "debug.db_overview",
        args: ["database_path": databasePath]
      ) else {
        return
      }
      guard JSONSerialization.isValidJSONObject(report),
            let data = try? JSONSerialization.data(
              withJSONObject: report,
              options: [.prettyPrinted, .sortedKeys]
            ) else {
        return
      }
      let directory = (databasePath as NSString).deletingLastPathComponent
      let outURL = URL(fileURLWithPath: directory).appendingPathComponent("bull-diag.json")
      try? data.write(to: outURL, options: .atomic)
    }
  }

  /// Load persisted nightly sleep records (newest first) so sleep trends are
  /// backed by real accumulated history instead of placeholder rows.
  func loadNightlySleepHistory(limit: Int = 30) {
    // Local-first: nightly sleep detail is read from the on-device store when
    // local compute is enabled, with the server query kept as a fallback.
    Task { [weak self] in
      let report = await Self.queryLocalOrServer(method: "sleep.list_nightly", args: ["limit": limit])
      let records = Self.nightlySleepRecords(from: report)
      self?.nightlySleepHistory = records
    }
  }

  /// Confidence below which a nightly record is shown as "needs review" rather
  /// than as a settled score. Mirrors the server's low-confidence,
  /// motion-only/unconfirmed acceptance band.
  static let nightlySleepNeedsReviewConfidence: Double = 0.45

  static func nightlySleepRecords(from report: [String: Any]?) -> [NightlySleepRecord] {
    guard let report else {
      return []
    }
    let parsed: [NightlySleepRecord] = array(report["nights"]).compactMap { row in
      guard let id = row["nightly_sleep_id"] as? String,
            let dateKey = row["date_key"] as? String else {
        return nil
      }
      let startMs = (doubleValue(row["start_time_unix_ms"]) ?? 0)
      let confidence = doubleValue(row["confidence"]) ?? 0
      return NightlySleepRecord(
        id: id,
        dateKey: dateKey,
        startTimeUnixMs: Int64(startMs),
        score: doubleValue(row["score_0_to_100"]),
        sleepDurationMinutes: doubleValue(row["sleep_duration_minutes"]),
        timeInBedMinutes: doubleValue(row["time_in_bed_minutes"]),
        heartRateDipPercent: doubleValue(row["heart_rate_dip_percent"]),
        confidence: confidence,
        algorithmId: row["algorithm_id"] as? String,
        needsReview: confidence < nightlySleepNeedsReviewConfidence
      )
    }
    return dedupeNightlySleepRecords(parsed, rows: array(report["nights"]))
  }

  /// Render-only safety net (the server already surfaces one main window per
  /// day): keep a single best record per `date_key`, dropping stale
  /// old-algorithm rows when a current one exists for the same day. "Best" =
  /// not stale, then highest confidence, then most recent start. Records whose
  /// `date_key` differs (distinct days) and naps all survive.
  static func dedupeNightlySleepRecords(
    _ records: [NightlySleepRecord],
    rows: [[String: Any]]
  ) -> [NightlySleepRecord] {
    // Map id → stale flag from the raw rows' quality flags.
    var staleById: [String: Bool] = [:]
    for row in rows {
      guard let id = row["nightly_sleep_id"] as? String else { continue }
      let flags = (row["quality_flags_json"] as? String) ?? ""
      staleById[id] = flags.contains("segmented_to_most_recent_night")
    }
    let isStale = { (r: NightlySleepRecord) -> Bool in staleById[r.id] ?? false }

    var bestByDay: [String: NightlySleepRecord] = [:]
    for r in records {
      guard let current = bestByDay[r.dateKey] else {
        bestByDay[r.dateKey] = r
        continue
      }
      let preferNew: Bool
      if isStale(current) != isStale(r) {
        preferNew = !isStale(r)
      } else if r.confidence != current.confidence {
        preferNew = r.confidence > current.confidence
      } else {
        preferNew = r.startTimeUnixMs > current.startTimeUnixMs
      }
      if preferNew {
        bestByDay[r.dateKey] = r
      }
    }
    return bestByDay.values.sorted { $0.startTimeUnixMs > $1.startTimeUnixMs }
  }

  static func primarySleepDetail(fromSleepReport report: [String: Any]?) -> PrimarySleepDetail? {
    guard let report,
          let output = map(report, "score_result", "output") else {
      return nil
    }
    let window = map(report, "sleep_window")
    let input = map(report, "sleep_v1_input") ?? map(report, "sleep_input")
    let start = bridgeDate(input?["start_time"] ?? window?["start_time"])
    let end = bridgeDate(input?["end_time"] ?? window?["end_time"])
    let duration = doubleValue(output["sleep_duration_minutes"])
      ?? doubleValue(window?["sleep_duration_minutes"])
      ?? doubleValue(input?["sleep_duration_minutes"])
      ?? 0
    let timeInBed = doubleValue(output["time_in_bed_minutes"])
      ?? doubleValue(window?["time_in_bed_minutes"])
      ?? doubleValue(input?["time_in_bed_minutes"])
      ?? duration
    let score = numberText(output["score_0_to_100"], fractionDigits: 0) ?? "--"
    let stages = sleepStageSegments(from: output)
    let idSuffix = start.map { "\(Int($0.timeIntervalSince1970))" } ?? "latest"

    return PrimarySleepDetail(
      id: "primary-sleep-\(idSuffix)",
      dateLabel: start.map(dateLabel) ?? "Latest",
      startLabel: start.map(timeLabel) ?? "--",
      endLabel: end.map(timeLabel) ?? "--",
      durationText: minutesText(duration),
      timeInBedText: minutesText(timeInBed),
      scoreText: score,
      qualityText: sleepQualityLabel(score: doubleValue(output["score_0_to_100"])),
      source: .bridge("metrics.sleep_score_from_features"),
      stages: stages
    )
  }

  static func sleepStageSegments(from output: [String: Any]) -> [HealthSleepStageSegment] {
    let stageRows = array(output["stage_segments"])
    if !stageRows.isEmpty {
      return stageRows.enumerated().compactMap { index, row in
        let stage = row["stage_kind"] as? String ?? row["stage"] as? String ?? "core"
        let duration = doubleValue(row["duration_minutes"]) ?? 0
        guard duration > 0 else {
          return nil
        }
        let start = bridgeDate(row["start_time"])
        let end = bridgeDate(row["end_time"])
        return HealthSleepStageSegment(
          id: "bridge-stage-\(index)-\(stage)",
          stage: stage,
          startLabel: start.map(timeLabel) ?? "--",
          endLabel: end.map(timeLabel) ?? "--",
          durationMinutes: duration,
          confidence: doubleValue(row["confidence_0_to_1"]),
          source: .bridge("sleep_v1 output stage_segments")
        )
      }
    }

    guard let minutesByStage = output["stage_minutes"] as? [String: Any] else {
      return []
    }
    return ["awake", "rem", "core", "deep"].compactMap { stage in
      guard let minutes = doubleValue(minutesByStage[stage]),
            minutes > 0 else {
        return nil
      }
      return HealthSleepStageSegment(
        id: "bridge-stage-total-\(stage)",
        stage: stage,
        startLabel: "--",
        endLabel: "--",
        durationMinutes: minutes,
        confidence: doubleValue(output["stage_segment_confidence_0_to_1"]),
        source: .bridge("sleep_v1 output stage_minutes")
      )
    }
  }

  /// Display-only schedule fields from the latest primary sleep window and sleep score output.
  /// Wind-down is not modeled in the app; it stays unavailable until a real source exists.
  struct SleepScheduleDisplay {
    let windDownLabel: String
    let bedtimeLabel: String
    let wakeLabel: String
    let sleepNeededLabel: String
    let hasTimelineData: Bool
    let awaitingCaption: String?

    static let unavailable = SleepScheduleDisplay(
      windDownLabel: "--:--",
      bedtimeLabel: "--:--",
      wakeLabel: "--:--",
      sleepNeededLabel: "--",
      hasTimelineData: false,
      awaitingCaption: "Awaiting sleep data"
    )
  }

  func sleepScheduleDisplay() -> SleepScheduleDisplay {
    guard let sleep = primarySleep() else {
      return .unavailable
    }
    let bedtime = sleep.startLabel == "--" ? "--:--" : sleep.startLabel
    let wake = sleep.endLabel == "--" ? "--:--" : sleep.endLabel
    let hasBedWake = bedtime != "--:--" && wake != "--:--"
    let sleepNeeded = Self.sleepNeededLabel(fromSleepReport: packetScoreReports["sleep"])
    return SleepScheduleDisplay(
      windDownLabel: "--:--",
      bedtimeLabel: bedtime,
      wakeLabel: wake,
      sleepNeededLabel: sleepNeeded ?? "--",
      hasTimelineData: hasBedWake,
      awaitingCaption: hasBedWake ? nil : "Awaiting sleep data"
    )
  }

  static func sleepNeededLabel(fromSleepReport report: [String: Any]?) -> String? {
    guard let output = Self.map(report, "score_result", "output"),
          let minutes = Self.doubleValue(output["sleep_need_minutes"]),
          minutes > 0 else {
      return nil
    }
    return Self.minutesText(minutes)
  }

  static func sleepQualityLabel(score: Double?) -> String {
    guard let score else {
      return "No score"
    }
    if score >= 85 {
      return "Optimal"
    }
    if score >= 70 {
      return "Good"
    }
    if score >= 50 {
      return "Needs attention"
    }
    return "Low"
  }

  static func recoveryQualityLabel(score: Double?) -> String {
    guard let score else {
      return "No data"
    }
    if score >= 67 {
      return "Recovered"
    }
    if score >= 34 {
      return "Moderate recovery"
    }
    if score > 0 {
      return "Low recovery"
    }
    return "No data"
  }

  static func strainStatusLabel(score: Double?) -> String {
    guard let score, score > 0 else {
      return "No strain data"
    }
    if score >= 70 {
      return "High strain"
    }
    if score >= 40 {
      return "Moderate strain"
    }
    return "Low strain"
  }

  static func strainPercent(_ rawScore0To21: Double) -> Double {
    min(max(rawScore0To21 / 21.0 * 100.0, 0), 100)
  }

  static func stressStatusLabel(score: Double?) -> String {
    guard let score else {
      return "No data"
    }
    if score >= 66 {
      return "High"
    }
    if score >= 33 {
      return "Medium"
    }
    return "Low"
  }

}
