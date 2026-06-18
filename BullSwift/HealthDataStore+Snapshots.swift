import Darwin
import Foundation
import SwiftUI
import UIKit

extension HealthDataStore {
  /// Compute packet-derived scores once if they have not been computed yet, so
  /// the Health/Sleep cards populate when the screen opens instead of requiring
  /// the developer "Run Packet-Derived Scores" button.
  func computePacketScoresIfNeeded() {
    // Always refresh from the server when not already in flight; the cache keeps
    // the cards populated while the fetch runs.
    guard !packetScoresComputeInFlight else {
      return
    }
    runPacketScores()
  }

  /// Load the server-computed score reports for the most recent day. The server
  /// runs the same parser/algorithms and stores each family's full report; the
  /// app reads them verbatim into `packetScoreReports` rather than recomputing
  /// on-device. No on-device fallback by design — if the server has nothing yet,
  /// the screens show honest unavailable states.
  func runPacketScores() {
    packetScoresComputeInFlight = true
    packetScoreStatus = "Loading scores from server..."
    Task { [weak self] in
      let reports = await Self.fetchServerScoreReports()
      guard let self else { return }
      if let sleep = reports["sleep"] {
        self.packetScoreReports["sleep"] = sleep
        self.refreshPrimarySleepFromScoreReport()
        self.loadNightlySleepHistory()
      }
      if let strain = reports["strain"] { self.packetScoreReports["strain"] = strain }
      if let recovery = reports["recovery"] { self.packetScoreReports["recovery"] = recovery }
      if let stress = reports["stress"] { self.packetScoreReports["stress"] = stress }
      if !reports.isEmpty {
        Self.saveReportsCache(self.packetScoreReports, name: "scores")
      }
      self.packetScoresComputeInFlight = false
      self.packetScoreStatus = reports.isEmpty
        ? "No server-computed scores yet"
        : "Scores loaded from server"
    }
  }

  /// Fetch the latest stored report for each score family from the server.
  /// `nonisolated` so the network work runs off the main actor.
  nonisolated static func fetchServerScoreReports() async -> [String: [String: Any]] {
    guard let token = CoachAuthKeychain.load() else { return [:] }
    var out: [String: [String: Any]] = [:]
    for family in ["recovery", "sleep", "strain", "stress"] {
      if let report = await fetchServerScoreReport(family: family, token: token) {
        out[family] = report
      }
    }
    return out
  }

  /// GET /v1/data/<family>?limit=1 and pull the newest row's `raw` (the full
  /// score report the parse pipeline stored). Returns nil on any failure so the
  /// caller leaves that family's screen in its unavailable state.
  nonisolated private static func fetchServerScoreReport(
    family: String,
    token: String
  ) async -> [String: Any]? {
    var components = URLComponents(
      url: CoachAPIConfiguration.baseURL.appendingPathComponent("v1/data/\(family)"),
      resolvingAgainstBaseURL: false
    )
    components?.queryItems = [URLQueryItem(name: "limit", value: "1")]
    guard let url = components?.url else { return nil }
    var request = URLRequest(url: url)
    request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    request.timeoutInterval = 30
    guard
      let (data, response) = try? await URLSession.shared.data(for: request),
      let http = response as? HTTPURLResponse, http.statusCode == 200,
      let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      let rows = json["rows"] as? [[String: Any]],
      let raw = rows.first?["raw"] as? [String: Any]
    else { return nil }
    return raw
  }

  // MARK: - Server report cache
  //
  // The score/input report maps come from the server and are otherwise in-memory
  // only, so a relaunch shows "--" until the network fetch returns. Persisting
  // the last successful fetch to disk lets the app paint the last-known values
  // immediately on launch, then refresh from the server in the background.

  nonisolated static func reportsCacheURL(_ name: String) -> URL? {
    guard let dir = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first else {
      return nil
    }
    return dir.appendingPathComponent("bull-reports-\(name).json")
  }

  nonisolated static func saveReportsCache(_ reports: [String: [String: Any]], name: String) {
    guard
      !reports.isEmpty,
      let url = reportsCacheURL(name),
      JSONSerialization.isValidJSONObject(reports),
      let data = try? JSONSerialization.data(withJSONObject: reports)
    else { return }
    try? data.write(to: url, options: .atomic)
  }

  nonisolated static func loadReportsCache(_ name: String) -> [String: [String: Any]] {
    guard
      let url = reportsCacheURL(name),
      let data = try? Data(contentsOf: url),
      let json = try? JSONSerialization.jsonObject(with: data) as? [String: [String: Any]]
    else { return [:] }
    return json
  }

  /// Populate the report maps from the on-disk cache so launch shows last-known
  /// values instantly. The server refresh overwrites these once it returns.
  func loadCachedServerReports() {
    let scores = Self.loadReportsCache("scores")
    if !scores.isEmpty {
      packetScoreReports = scores
      refreshPrimarySleepFromScoreReport()
    }
    let inputs = Self.loadReportsCache("inputs")
    if !inputs.isEmpty {
      packetInputReports = inputs
    }
  }

  /// Fetch the server-computed packet-derived input report map (HRV, resting HR,
  /// steps, energy, motion, vital events, daily/hourly rollups). The server runs
  /// the same bull-core methods over the user's data and stores the map; the app
  /// reads it verbatim into `packetInputReports` rather than computing on-device.
  /// `nonisolated` so the network work runs off the main actor. Returns an empty
  /// map on any failure so screens fall back to honest unavailable states.
  /// Read-through query against the server-side store for display surfaces that
  /// used to read the local store directly (nightly sleep, biometric streams,
  /// recorded activity). Returns the method `result` object, or nil on failure /
  /// honest-empty (no server store yet).
  nonisolated static func fetchServerQuery(method: String, args: [String: Any] = [:]) async -> [String: Any]? {
    guard let token = CoachAuthKeychain.load() else { return nil }
    let url = CoachAPIConfiguration.baseURL.appendingPathComponent("v1/data/query")
    var request = URLRequest(url: url)
    request.httpMethod = "POST"
    request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    request.timeoutInterval = 30
    let body: [String: Any] = ["method": method, "args": args]
    guard
      JSONSerialization.isValidJSONObject(body),
      let httpBody = try? JSONSerialization.data(withJSONObject: body)
    else { return nil }
    request.httpBody = httpBody
    guard
      let (data, response) = try? await URLSession.shared.data(for: request),
      let http = response as? HTTPURLResponse, http.statusCode == 200,
      let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
    else { return nil }
    return json["result"] as? [String: Any]
  }

  nonisolated static func fetchServerInputReports() async -> [String: [String: Any]] {
    guard let token = CoachAuthKeychain.load() else { return [:] }
    let url = CoachAPIConfiguration.baseURL.appendingPathComponent("v1/data/inputs")
    var request = URLRequest(url: url)
    request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    request.timeoutInterval = 30
    guard
      let (data, response) = try? await URLSession.shared.data(for: request),
      let http = response as? HTTPURLResponse, http.statusCode == 200,
      let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      let reports = json["reports"] as? [String: [String: Any]]
    else { return [:] }
    return reports
  }

  func runReferenceComparisons() {
    referenceComparisonReports = [:]
    for family in ["hrv", "sleep", "strain", "stress"] {
      referenceRunStatusByFamily[family] = "blocked | real comparison inputs not wired"
    }
  }

  func importCalibrationLabels() {
    calibrationLabelsImported = true
  }

  func calibrate() {
    calibrationRunComplete = true
  }

  var algorithmFamilies: [String] {
    let families = Set(algorithmDefinitions.map(\.family))
      .union(["recovery", "sleep", "strain", "stress", "hrv"])
    return families.sorted()
  }

  func algorithms(for family: String) -> [HealthAlgorithmDefinition] {
    algorithmDefinitions.filter { $0.family == family }
  }

  func landingSnapshots(
    liveHeartRateBPM: Int?,
    liveHeartRateSource: String,
    liveHeartRateUpdatedAt: Date?,
    stableDailyMetrics: Bool = false
  ) -> [HealthMetricSnapshot] {
    let _signpost = bullSignpostBegin(BullSignpost.ui, "landingSnapshots")
    defer { bullSignpostEnd(_signpost) }
    var snapshots = Self.baseLandingSnapshots
    if let index = snapshots.firstIndex(where: { $0.route == .sleep }) {
      snapshots[index] = sleepSnapshot(base: snapshots[index])
    }
    if let index = snapshots.firstIndex(where: { $0.route == .recovery }) {
      snapshots[index] = recoverySnapshot(base: snapshots[index])
    }
    if let index = snapshots.firstIndex(where: { $0.route == .strain }) {
      snapshots[index] = strainSnapshot(base: snapshots[index])
    }
    if let index = snapshots.firstIndex(where: { $0.route == .stress }) {
      snapshots[index] = stressSnapshot(base: snapshots[index], allowLiveFallbacks: !stableDailyMetrics)
    }
    if let index = snapshots.firstIndex(where: { $0.route == .cardioLoad }) {
      snapshots[index] = cardioLoadSnapshot(base: snapshots[index])
    }
    if let index = snapshots.firstIndex(where: { $0.route == .energyBank }) {
      snapshots[index] = energyBankSnapshot(base: snapshots[index], allowLiveFallbacks: !stableDailyMetrics)
    }
    if let liveHeartRateBPM,
       let index = snapshots.firstIndex(where: { $0.id == "health-monitor" }) {
      snapshots[index] = HealthMetricSnapshot(
        id: "health-monitor",
        route: .healthMonitor,
        group: .today,
        title: "Health Monitor",
        value: "\(liveHeartRateBPM)",
        unit: "bpm",
        status: "Live HR",
        freshness: Self.relativeText(for: liveHeartRateUpdatedAt) ?? "Now",
        provenance: liveHeartRateSource,
        source: .live("BLE heart rate stream"),
        systemImage: "heart.text.square",
        tint: .red,
        trend: snapshots[index].trend
      )
    }
    return snapshots
  }

  func healthMonitorSnapshots(
    restingHeartRateEstimateBPM: Double? = nil,
    restingHeartRateEstimateSampleCount: Int = 0,
    restingHeartRateEstimateUpdatedAt: Date? = nil,
    restingHeartRateEstimateSource: String = "ble.hr.standard.low_quartile",
    allowLiveFallbacks: Bool = true
  ) -> [HealthMetricSnapshot] {
    let _signpost = bullSignpostBegin(BullSignpost.ui, "healthMonitorSnapshots")
    defer { bullSignpostEnd(_signpost) }
    if previewMissingData {
      return Self.baseHealthMonitorSnapshots.map { snapshot in
        HealthMetricSnapshot(
          id: snapshot.id,
          route: snapshot.route,
          group: snapshot.group,
          title: snapshot.title,
          value: "--",
          unit: snapshot.unit,
          status: "Unavailable",
          freshness: "No local data",
          provenance: "preview missing data",
          source: .unavailable("preview missing data"),
          systemImage: snapshot.systemImage,
          tint: snapshot.tint,
          trend: HealthTrendModel(id: snapshot.trend.id, title: snapshot.trend.title, rangeLabel: "No data", summary: "No trend data", analysis: "No local data has been captured for this trend yet.", resources: snapshot.trend.resources, points: [])
        )
      }
    }
    var snapshots = Self.baseHealthMonitorSnapshots.map {
      packetBackedHealthMonitorSnapshot(base: $0, allowLiveFallbacks: allowLiveFallbacks)
    }
    if allowLiveFallbacks,
       let index = snapshots.firstIndex(where: { $0.id == "resting-hr" }),
       snapshots[index].source.kind == .unavailable,
       let sample = Self.liveHRDerivedRestingHeartRateSample(
        bpm: restingHeartRateEstimateBPM,
        sampleCount: restingHeartRateEstimateSampleCount,
        updatedAt: restingHeartRateEstimateUpdatedAt,
        source: restingHeartRateEstimateSource
       ) {
      snapshots[index] = liveHRDerivedRestingHeartRateHealthMonitorSnapshot(
        base: snapshots[index],
        sample: sample
      )
    }
    if let index = snapshots.firstIndex(where: { $0.id == "health-sleep" }) {
      snapshots[index] = sleepHealthMonitorSnapshot(base: snapshots[index])
    }
    return snapshots
  }

  func snapshot(for route: HealthRoute) -> HealthMetricSnapshot {
    let snapshot = Self.baseLandingSnapshots.first { $0.route == route }
      ?? Self.baseLandingSnapshots[0]
    if route == .sleep && !previewMissingData {
      return sleepSnapshot(base: snapshot)
    }
    if route == .recovery {
      return recoverySnapshot(base: snapshot)
    }
    if route == .strain && !previewMissingData {
      return strainSnapshot(base: snapshot)
    }
    if route == .stress && !previewMissingData {
      return stressSnapshot(base: snapshot)
    }
    if route == .cardioLoad && !previewMissingData {
      return cardioLoadSnapshot(base: snapshot)
    }
    if route == .energyBank && !previewMissingData {
      return energyBankSnapshot(base: snapshot)
    }
    guard previewMissingData else {
      return snapshot
    }
    return HealthMetricSnapshot(
      id: snapshot.id,
      route: snapshot.route,
      group: snapshot.group,
      title: snapshot.title,
      value: "--",
      unit: snapshot.unit,
      status: "No data",
      freshness: "Missing",
      provenance: "preview missing data",
      source: .unavailable("preview missing data"),
      systemImage: snapshot.systemImage,
      tint: snapshot.tint,
      trend: HealthTrendModel(id: snapshot.trend.id, title: snapshot.trend.title, rangeLabel: "No data", summary: "No trend data", analysis: "No local data has been captured for this trend yet.", resources: snapshot.trend.resources, points: [])
    )
  }

  func strainSnapshot(for date: Date, calendar: Calendar = .current) -> HealthMetricSnapshot {
    let base = Self.baseLandingSnapshots.first { $0.route == .strain } ?? Self.baseLandingSnapshots[0]
    let snapshot = strainSnapshot(base: base)
    guard calendar.isDate(calendar.startOfDay(for: date), inSameDayAs: calendar.startOfDay(for: Date())) else {
      return zeroStrainSnapshot(
        base: snapshot,
        freshness: ScoreDateTimeline.dateLabel(for: date, calendar: calendar),
        provenance: "No local strain history for selected date",
        sourceDetail: "selected date has no local strain history"
      )
    }
    return snapshot
  }

  func sleepSnapshot(base snapshot: HealthMetricSnapshot) -> HealthMetricSnapshot {
    if let output = Self.map(packetScoreReports["sleep"], "score_result", "output") {
      let scoreText = Self.numberText(output["score_0_to_100"], fractionDigits: 0) ?? snapshot.value
      return HealthMetricSnapshot(
        id: snapshot.id,
        route: snapshot.route,
        group: snapshot.group,
        title: snapshot.title,
        value: scoreText,
        unit: "%",
        status: Self.sleepQualityLabel(score: Self.doubleValue(output["score_0_to_100"])),
        freshness: "Latest",
        provenance: "metrics.sleep_score_from_features",
        source: .bridge("bull.sleep.v1"),
        systemImage: snapshot.systemImage,
        tint: snapshot.tint,
        trend: snapshot.trend
      )
    }
    if let primarySleepDetail {
      return HealthMetricSnapshot(
        id: snapshot.id,
        route: snapshot.route,
        group: snapshot.group,
        title: snapshot.title,
        value: primarySleepDetail.durationText,
        unit: "",
        status: primarySleepDetail.qualityText,
        freshness: primarySleepDetail.dateLabel,
        provenance: primarySleepDetail.source.detail,
        source: primarySleepDetail.source,
        systemImage: snapshot.systemImage,
        tint: snapshot.tint,
        trend: snapshot.trend
      )
    }
    return snapshot
  }

  func sleepHealthMonitorSnapshot(base snapshot: HealthMetricSnapshot) -> HealthMetricSnapshot {
    if let primarySleepDetail {
      return HealthMetricSnapshot(
        id: snapshot.id,
        route: snapshot.route,
        group: snapshot.group,
        title: snapshot.title,
        value: primarySleepDetail.durationText,
        unit: "",
        status: primarySleepDetail.qualityText,
        freshness: primarySleepDetail.dateLabel,
        provenance: primarySleepDetail.source.detail,
        source: primarySleepDetail.source,
        systemImage: snapshot.systemImage,
        tint: snapshot.tint,
        trend: snapshot.trend
      )
    }
    if let output = Self.map(packetScoreReports["sleep"], "score_result", "output"),
       let duration = Self.doubleValue(output["sleep_duration_minutes"]) {
      return HealthMetricSnapshot(
        id: snapshot.id,
        route: snapshot.route,
        group: snapshot.group,
        title: snapshot.title,
        value: Self.minutesText(duration),
        unit: "",
        status: Self.sleepQualityLabel(score: Self.doubleValue(output["score_0_to_100"])),
        freshness: "Latest",
        provenance: "metrics.sleep_score_from_features",
        source: .bridge("bull.sleep.v1"),
        systemImage: snapshot.systemImage,
        tint: snapshot.tint,
        trend: snapshot.trend
      )
    }
    return snapshot
  }

  func recoverySnapshot(base snapshot: HealthMetricSnapshot) -> HealthMetricSnapshot {
    guard !usesPreviewPacketData,
          let score = recoveryScoreValue(),
          let scoreText = Self.numberText(score, fractionDigits: 0) else {
      return HealthMetricSnapshot(
        id: snapshot.id,
        route: snapshot.route,
        group: snapshot.group,
        title: snapshot.title,
        value: "--",
        unit: "%",
        status: "No data",
        freshness: "No recovery score",
        provenance: "metrics.recovery_score_from_features",
        source: .unavailable("recovery score not available"),
        systemImage: snapshot.systemImage,
        tint: snapshot.tint,
        trend: Self.emptyTrend(from: snapshot.trend, packetCount: packetEvidenceFrameCount())
      )
    }

    return HealthMetricSnapshot(
      id: snapshot.id,
      route: snapshot.route,
      group: snapshot.group,
      title: snapshot.title,
      value: scoreText,
      unit: "%",
      status: Self.recoveryQualityLabel(score: score),
      freshness: "Latest",
      provenance: "metrics.recovery_score_from_features",
      source: .bridge("bull.recovery.v0"),
      systemImage: snapshot.systemImage,
      tint: snapshot.tint,
      trend: recoveryScoreTrend(base: snapshot.trend, currentScore: score)
    )
  }

  var usesPreviewPacketData: Bool {
    packetInputStatus.hasPrefix("Preview") || packetScoreStatus.hasPrefix("Preview")
  }

  func recoveryScoreValue() -> Double? {
    guard !usesPreviewPacketData else {
      return nil
    }
    return Self.doubleValue(Self.map(packetScoreReports["recovery"], "score_result", "output")?["score_0_to_100"])
  }

  func recoveryScoreTrend(base trend: HealthTrendModel, currentScore: Double) -> HealthTrendModel {
    HealthTrendModel(
      id: trend.id,
      title: trend.title,
      rangeLabel: "\(Self.numberText(currentScore, fractionDigits: 0) ?? "0")%",
      summary: "Latest packet-derived recovery score",
      analysis: "Packet-derived recovery score from the local bridge.",
      resources: trend.resources,
      points: []
    )
  }

  func strainScore0To100(for date: Date = Date(), calendar: Calendar = .current) -> Double {
    guard calendar.isDate(calendar.startOfDay(for: date), inSameDayAs: calendar.startOfDay(for: Date())) else {
      return 0
    }
    return currentStrainScore0To21().map(Self.strainPercent) ?? 0
  }

  func strainScoreDisplayText(for date: Date = Date(), calendar: Calendar = .current) -> String {
    let score = strainScore0To100(for: date, calendar: calendar)
    guard score > 0 else {
      return "--"
    }
    return Self.numberText(score, fractionDigits: 0) ?? "0"
  }

  func strainStatusText(for date: Date = Date(), calendar: Calendar = .current) -> String {
    guard calendar.isDate(calendar.startOfDay(for: date), inSameDayAs: calendar.startOfDay(for: Date())),
          let rawScore = currentStrainScore0To21() else {
      return "No strain data"
    }
    return Self.strainStatusLabel(score: Self.strainPercent(rawScore))
  }

  func strainTargetDisplayText() -> String {
    "--"
  }

  func strainDurationDisplayText() -> String {
    "--"
  }

  func strainEnergyDisplayText(for date: Date = Date(), calendar: Calendar = .current) -> String {
    whoopTotalCaloriesDisplayText(for: date, calendar: calendar)
  }

  func strainActivityCountText(for date: Date = Date(), calendar: Calendar = .current) -> String {
    whoopStepsDisplayText(for: date, calendar: calendar)
  }

  func whoopStepsDisplayText(for date: Date = Date(), calendar: Calendar = .current) -> String {
    guard let metric = stepMetric(for: date, calendar: calendar),
          let steps = Self.intValue(metric["steps"]) else {
      return "--"
    }
    return Self.groupedIntegerText(steps)
  }

  func whoopActiveCaloriesDisplayText(for date: Date = Date(), calendar: Calendar = .current) -> String {
    energyKcalDisplayText(key: "active_kcal", date: date, calendar: calendar)
  }

  func whoopTotalCaloriesDisplayText(for date: Date = Date(), calendar: Calendar = .current) -> String {
    energyKcalDisplayText(key: "total_kcal", date: date, calendar: calendar)
  }

  func whoopStepsStatusText() -> String {
    if let metric = todayStepMetric() {
      return stepMetricStatus(metric)
    }

    if let latest = Self.preferredStepMetric(from: dailyActivityMetrics()),
       let dateKey = latest["date_key"] as? String {
      return "No today step metric | latest stored \(dateKey)"
    }

    if let report = packetInputReports["step_counter_rollup"] {
      return humanizedHomeStatus(firstPacketAction(in: report) ?? "Steps still syncing from your band")
    }

    if let report = packetInputReports["step_counter_ingest"] {
      let persisted = Self.intValue(report["persisted_sample_count"]) ?? 0
      let candidates = Self.intValue(report["counter_candidate_count"]) ?? 0
      if persisted > 0 {
        return "Steps are syncing — check back after your band finishes syncing."
      }
      if candidates > 0 {
        return "Steps are syncing from your band."
      }
    }

    if let motionReport = packetInputReports["motion"] {
      let total = Self.intValue(motionReport["feature_count"]) ?? 0
      let trusted = Self.intValue(motionReport["trusted_feature_count"]) ?? 0
      if total > 0 {
        return "Steps are updating from your band."
      }
      return "Steps will appear after your band syncs."
    }

    if packetInputStatus == "No run" {
      return "Sync your band to load steps"
    }
    return humanizedHomeStatus(packetInputStatus)
  }

  func whoopStepsSource(for date: Date = Date(), calendar: Calendar = .current) -> HealthDataSource {
    if let metric = stepMetric(for: date, calendar: calendar) {
      switch metric["source_kind"] as? String {
      case "device_counter":
        return .bridgeDeviceCounter("daily_activity_metrics WHOOP step counter")
      case "local_estimate":
        return .localEstimate("daily_activity_metrics validated raw-motion steps")
      default:
        return .unavailable("unsupported step metric source")
      }
    }
    guard calendar.isDate(calendar.startOfDay(for: date), inSameDayAs: calendar.startOfDay(for: Date())) else {
      return .unavailable("selected date has no stored WHOOP step metric")
    }
    if let report = packetInputReports["step_counter_rollup"] {
      return .unavailable(firstPacketAction(in: report) ?? "WHOOP step counter rollup blocked")
    }
    if packetInputReports["motion"] == nil {
      return .unavailable("WHOOP step extraction pending")
    }
    return .unavailable("WHOOP step counter or validated local estimate not available")
  }

  func whoopActiveCaloriesStatusText() -> String {
    if let metric = energyMetric(for: Date(), valueKey: "active_kcal") {
      return energyMetricStatus(metric)
    }

    guard let report = packetInputReports["energy_rollup"] else {
      if let latest = Self.preferredDailyActivityMetric(
        from: dailyActivityMetricsWithValue("active_kcal"),
        valueKey: "active_kcal"
      ),
         let dateKey = latest["date_key"] as? String {
        return "No today calorie metric | latest stored \(dateKey)"
      }
      if packetInputStatus == "No run" {
        return "Sync your band to load active calories"
      }
      return humanizedHomeStatus(packetInputStatus)
    }
    if Self.boolValue(report["pass"]) == true,
       let confidence = Self.numberText(report["confidence"], fractionDigits: 2) {
      return "Local estimate | confidence \(confidence)"
    }
    return humanizedHomeStatus(firstPacketAction(in: report) ?? "Active calories still syncing from your band")
  }

  func whoopActiveCaloriesSource(
    for date: Date = Date(),
    calendar: Calendar = .current
  ) -> HealthDataSource {
    whoopEnergySource(for: date, calendar: calendar, valueKey: "active_kcal")
  }

  func whoopTotalCaloriesSource(
    for date: Date = Date(),
    calendar: Calendar = .current
  ) -> HealthDataSource {
    whoopEnergySource(for: date, calendar: calendar, valueKey: "total_kcal")
  }

  func whoopEnergySource(
    for date: Date,
    calendar: Calendar,
    valueKey: String
  ) -> HealthDataSource {
    if let metric = energyMetric(for: date, calendar: calendar, valueKey: valueKey) {
      return energyMetricSource(metric)
    }
    if let unavailable = preferredDailyActivityUnavailableMetric(metricID: valueKey, for: date, calendar: calendar) {
      return .unavailable(Self.activityUnavailableSourceDetail(unavailable))
    }
    guard calendar.isDate(calendar.startOfDay(for: date), inSameDayAs: calendar.startOfDay(for: Date())) else {
      return .unavailable("selected date has no stored WHOOP energy metric")
    }
    guard let report = packetInputReports["energy_rollup"] else {
      return .unavailable("metrics.energy_daily_rollup not run")
    }
    guard Self.boolValue(report["pass"]) == true else {
      return .unavailable("metrics.energy_daily_rollup blocked")
    }
    return .localEstimate("metrics.energy_daily_rollup")
  }

  func energyRollupSummary() -> String {
    guard let report = packetInputReports["energy_rollup"] else {
      return packetInputStatus == "No run" ? "No run" : packetInputStatus
    }
    let active = Self.numberText(report["active_kcal"], fractionDigits: 0) ?? "--"
    let resting = Self.numberText(report["resting_kcal"], fractionDigits: 0) ?? "--"
    let total = Self.numberText(report["total_kcal"], fractionDigits: 0) ?? "--"
    let confidence = Self.numberText(report["confidence"], fractionDigits: 2) ?? "0"
    return "\(Self.passStatus(report)) | active \(active) kcal | resting \(resting) kcal | total \(total) kcal | confidence \(confidence)"
  }

  func energyRollupProvenanceSummary() -> String {
    guard let report = packetInputReports["energy_rollup"] else {
      return ""
    }
    let written = Self.boolValue(report["daily_metric_written"]) == true ? "stored" : "not stored"
    let hrSamples = Self.intValue(report["heart_rate_sample_count"]) ?? 0
    let motionSamples = Self.intValue(report["motion_sample_count"]) ?? 0
    let coverage = Self.percentText(report["coverage_fraction"]) ?? "unknown"
    return "daily_metric=\(written) | HR=\(hrSamples) | motion=\(motionSamples) | coverage=\(coverage)"
  }

  func energyKcalDisplayText(
    key: String,
    date: Date = Date(),
    calendar: Calendar = .current
  ) -> String {
    if let metric = energyMetric(for: date, calendar: calendar, valueKey: key),
       let value = Self.doubleValue(metric[key]),
       value.isFinite {
      return "\(Self.groupedIntegerText(Int(value.rounded()))) kcal"
    }
    guard let report = packetInputReports["energy_rollup"],
          calendar.isDate(calendar.startOfDay(for: date), inSameDayAs: calendar.startOfDay(for: Date())),
          Self.boolValue(report["pass"]) == true,
          let value = Self.doubleValue(report[key]),
          value.isFinite else {
      return "-- kcal"
    }
    return "\(Self.groupedIntegerText(Int(value.rounded()))) kcal"
  }

  func todayStepMetric() -> [String: Any]? {
    stepMetric(for: Date())
  }

  func stepMetric(for date: Date, calendar: Calendar = .current) -> [String: Any]? {
    Self.preferredStepMetric(
      from: dailyActivityMetrics(forDateKey: Self.metricDateKey(for: date, calendar: calendar))
    )
  }

  func energyMetric(
    for date: Date,
    calendar: Calendar = .current,
    valueKey: String
  ) -> [String: Any]? {
    Self.preferredDailyActivityMetric(
      from: dailyActivityMetrics(forDateKey: Self.metricDateKey(for: date, calendar: calendar)),
      valueKey: valueKey
    )
  }

  func dailyActivityMetrics() -> [[String: Any]] {
    displaySafeMetrics(family: "daily_activity")
  }

  func dailyActivityMetrics(forDateKey dateKey: String) -> [[String: Any]] {
    dailyActivityMetrics().filter { $0["date_key"] as? String == dateKey }
  }

  func dailyActivityMetricsWithValue(_ valueKey: String) -> [[String: Any]] {
    dailyActivityMetrics().filter { Self.doubleValue($0[valueKey]) != nil }
  }

  func hourlyActivityMetrics() -> [[String: Any]] {
    displaySafeMetrics(family: "hourly_activity")
  }

  func hourlyActivityMetrics(forDateKey dateKey: String) -> [[String: Any]] {
    hourlyActivityMetrics().filter { $0["date_key"] as? String == dateKey }
  }

  func hourlyActivityMetricsWithValue(_ valueKey: String) -> [[String: Any]] {
    hourlyActivityMetrics().filter { Self.doubleValue($0[valueKey]) != nil }
  }

  func dailyActivityUnavailableMetrics(metricID: String? = nil) -> [[String: Any]] {
    dailyActivityMetrics()
      .filter { metric in
        guard metric["source_kind"] as? String == "unavailable",
              Self.doubleValue(metric["confidence"]) != nil else {
          return false
        }
        if let metricID {
          return Self.dailyActivityUnavailableMetric(metric, matches: metricID)
        }
        return true
      }
  }

  func preferredDailyActivityUnavailableMetric(
    metricID: String,
    for date: Date? = nil,
    calendar: Calendar = .current
  ) -> [String: Any]? {
    let dateKey = date.map { Self.metricDateKey(for: $0, calendar: calendar) }
    return dailyActivityUnavailableMetrics(metricID: metricID)
      .filter { metric in
        if let dateKey, metric["date_key"] as? String != dateKey {
          return false
        }
        return true
      }
      .sorted { lhs, rhs in
        let lhsEnd = Self.int64Value(lhs["end_time_unix_ms"]) ?? 0
        let rhsEnd = Self.int64Value(rhs["end_time_unix_ms"]) ?? 0
        if lhsEnd != rhsEnd {
          return lhsEnd > rhsEnd
        }
        let lhsUpdated = lhs["updated_at"] as? String ?? ""
        let rhsUpdated = rhs["updated_at"] as? String ?? ""
        return lhsUpdated > rhsUpdated
      }
      .first
  }

  static func dailyActivityUnavailableMetric(_ metric: [String: Any], matches metricID: String) -> Bool {
    if let inputsMetricID = jsonObject(fromJSONString: metric["inputs_json"])?["metric_id"] as? String,
       inputsMetricID == metricID {
      return true
    }
    let sanitizedMetricID = metricIDToken(metricID)
    let dailyMetricID = (metric["daily_metric_id"] as? String ?? "").lowercased()
    return dailyMetricID.contains(sanitizedMetricID)
  }

  static func activityUnavailableSourceDetail(_ metric: [String: Any]) -> String {
    let metricID = jsonObject(fromJSONString: metric["inputs_json"])?["metric_id"] as? String
      ?? metric["daily_metric_id"] as? String
      ?? "activity_metric"
    let blocker = firstActivityUnavailableBlocker(metric) ?? "metric unavailable"
    return "\(metricID) unavailable: \(blocker)"
  }

}
