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
    let bridge = self.bridge
    let databasePath = self.databasePath
    packetInputQueue.async { [weak self] in
      let report = try? bridge.request(
        method: "sleep.list_nightly",
        args: [
          "database_path": databasePath,
          "limit": limit,
        ]
      )
      let records = Self.nightlySleepRecords(from: report)
      DispatchQueue.main.async { [weak self] in
        self?.nightlySleepHistory = records
      }
    }
  }

  static func nightlySleepRecords(from report: [String: Any]?) -> [NightlySleepRecord] {
    guard let report else {
      return []
    }
    return array(report["nights"]).compactMap { row in
      guard let id = row["nightly_sleep_id"] as? String,
            let dateKey = row["date_key"] as? String else {
        return nil
      }
      let startMs = (doubleValue(row["start_time_unix_ms"]) ?? 0)
      return NightlySleepRecord(
        id: id,
        dateKey: dateKey,
        startTimeUnixMs: Int64(startMs),
        score: doubleValue(row["score_0_to_100"]),
        sleepDurationMinutes: doubleValue(row["sleep_duration_minutes"]),
        timeInBedMinutes: doubleValue(row["time_in_bed_minutes"]),
        heartRateDipPercent: doubleValue(row["heart_rate_dip_percent"]),
        confidence: doubleValue(row["confidence"]) ?? 0
      )
    }
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
