import Darwin
import Foundation
import SwiftUI
import UIKit

@MainActor
final class HealthDataStore: ObservableObject {
  @Published var algorithmDefinitions: [HealthAlgorithmDefinition]
  @Published var referenceDefinitions: [HealthAlgorithmDefinition]
  @Published var selectedAlgorithmByFamily: [String: String]
  @Published var catalogStatus = "Metric catalog not loaded"
  @Published var catalogSource = HealthDataSource.unavailable("metric registry not loaded")
  @Published var packetInputStatus = "No run"
  @Published var packetScoreStatus = "No run"
  @Published var bandSleepImportStatus = "No band sync yet"
  @Published var externalSleepImportStatus = "External sleep imports disabled"
  @Published var referenceRunStatusByFamily: [String: String] = [:]
  @Published var primarySleepDetail: PrimarySleepDetail?
  @Published var calibrationTargetFamily = "recovery"
  @Published var calibrationLabelsImported = false
  @Published var calibrationRunComplete = false
  @Published var heartRateHourlyRanges: [HeartRateHourlyRange] = []
  @Published var heartRateTimelineStatus = "No HR samples stored"

  let bridge = BullRustBridge()
  let heartRateSeriesStore = HeartRateSeriesStore.shared
  var attemptedCatalogLoad = false
  var previewMissingData = false
  // The forbidden-source-marker scan deep-reads every string in every metric
  // row. Metric rows are Swift value-type dictionaries with no stable object
  // identity, so per-row/per-container caches keyed on identity never hit and
  // the scan re-ran for every row on every SwiftUI body pass (an on-device
  // Time Profiler trace attributed 22 s of a 42 s main-thread total to it
  // during sync). It only needs to run when the reports change, so the
  // display-safe rows are filtered once here and read everywhere else.
  var packetInputReports: [String: [String: Any]] = [:] {
    didSet { rebuildDisplaySafeMetrics() }
  }
  private static let displaySafeFamilies = ["daily_recovery", "daily_activity", "hourly_activity"]
  private var displaySafeMetricsByFamily: [String: [[String: Any]]] = [:]

  private func rebuildDisplaySafeMetrics() {
    var result: [String: [[String: Any]]] = [:]
    for family in Self.displaySafeFamilies {
      result[family] = Self.array(packetInputReports[family]?["metrics"])
        .filter { Self.localHealthMetricRowIsDisplaySafe($0) }
    }
    displaySafeMetricsByFamily = result
  }

  /// Display-safe metric rows for a family, filtered once when the packet-input
  /// reports were stored. Callers must not re-apply the display-safety filter.
  func displaySafeMetrics(family: String) -> [[String: Any]] {
    displaySafeMetricsByFamily[family] ?? []
  }
  var packetScoreReports: [String: [String: Any]] = [:]
  var referenceComparisonReports: [String: [String: Any]] = [:]
  var packetInputRefreshWorkItem: DispatchWorkItem?
  var packetInputRunID: UUID?
  var packetInputIsRunning = false
  var heartRateTimelineRefreshID: UUID?
  var heartRateSeriesUpdateObserver: NSObjectProtocol?
  let packetInputQueue = DispatchQueue(label: "com.bull.swift.health.packet-inputs", qos: .utility)
  let heartRateTimelineQueue = DispatchQueue(label: "com.bull.swift.health.heart-rate-timeline", qos: .utility)
  lazy var databasePath = HealthDataStore.defaultDatabasePath()

  static let liveHRVRMSSDDefaultsKey = "bull.swift.liveHRVRMSSD"
  static let liveHRVRRIntervalCountDefaultsKey = "bull.swift.liveHRVRRIntervalCount"
  static let liveHRVRMSSDSampleCountDefaultsKey = "bull.swift.liveHRVRMSSDSampleCount"
  static let liveHRVUpdatedAtDefaultsKey = "bull.swift.liveHRVUpdatedAt"
  static let liveHRVSourceDefaultsKey = "bull.swift.liveHRVSource"
  static let restingHeartRateEstimateBPMDefaultsKey = "bull.swift.restingHeartRateEstimateBPM"
  static let restingHeartRateEstimateSampleCountDefaultsKey = "bull.swift.restingHeartRateEstimateSampleCount"
  static let restingHeartRateEstimateUpdatedAtDefaultsKey = "bull.swift.restingHeartRateEstimateUpdatedAt"
  static let restingHeartRateEstimateSourceDefaultsKey = "bull.swift.restingHeartRateEstimateSource"

  init() {
    algorithmDefinitions = []
    referenceDefinitions = []
    selectedAlgorithmByFamily = [:]
    primarySleepDetail = nil
    refreshHeartRateTimeline()
    heartRateSeriesUpdateObserver = NotificationCenter.default.addObserver(
      forName: HeartRateSeriesStore.didUpdateNotification,
      object: nil,
      queue: .main
    ) { [weak self] _ in
      Task { @MainActor in
        self?.refreshHeartRateTimeline()
      }
    }
  }

  deinit {
    if let heartRateSeriesUpdateObserver {
      NotificationCenter.default.removeObserver(heartRateSeriesUpdateObserver)
    }
  }

  static func defaultDatabasePath() -> String {
    let baseDirectory = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
      ?? FileManager.default.temporaryDirectory
    let directory = baseDirectory.appendingPathComponent("BullSwift", isDirectory: true)
    try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    return directory.appendingPathComponent("bull.sqlite").path
  }

  var usesSampleData: Bool {
    false
  }

  var localDataSupportsExport: Bool {
    !packetInputReports.isEmpty || !packetScoreReports.isEmpty || !referenceComparisonReports.isEmpty
  }

  var localHealthExportText: String {
    [
      "Bull Health Export",
      "Catalog: \(catalogStatus)",
      "Band sleep import: \(bandSleepImportStatus)",
      "HealthKit metric import: disabled; profile weight only",
      "Packet inputs: \(packetInputStatus)",
      "Packet scores: \(packetScoreStatus)",
      "Readiness: \(metricInputReadinessSummary())",
      "Sleep: \(sleepFeatureScoreSummary())",
      "Recovery: \(recoveryFeatureScoreSummary())",
      "Strain: \(strainFeatureScoreSummary())",
      "Stress: \(stressFeatureScoreSummary())",
    ].joined(separator: "\n")
  }

  func loadBridgeCatalogsIfNeeded() {
    guard !attemptedCatalogLoad else {
      return
    }
    attemptedCatalogLoad = true
    refreshBridgeCatalogs()
  }

  func refreshPacketInputsIfNeeded() {
    guard packetInputReports.isEmpty, packetInputStatus == "No run" else {
      return
    }
    runPacketInputs()
  }

  func refreshHeartRateTimeline(for date: Date = Date()) {
    let refreshID = UUID()
    heartRateTimelineRefreshID = refreshID
    let store = heartRateSeriesStore
    heartRateTimelineQueue.async { [weak self] in
      let snapshot = store.timelineSnapshot(forDayContaining: date)
      Task { @MainActor in
        guard let self,
              self.heartRateTimelineRefreshID == refreshID else {
          return
        }
        self.heartRateHourlyRanges = snapshot.ranges
        self.heartRateTimelineStatus = snapshot.status
      }
    }
  }

  func heartRateHourlyTimelineRows(maxRows: Int = 8) -> [HealthSummaryRow] {
    let ranges = Array(heartRateHourlyRanges.suffix(maxRows)).reversed()
    guard !ranges.isEmpty else {
      return []
    }

    return ranges.map { range in
      let hour = range.hourStart.formatted(.dateTime.hour(.twoDigits(amPM: .abbreviated)))
      return HealthSummaryRow(
        "HR \(hour)",
        value: "\(range.minBPM)-\(range.maxBPM) bpm | avg \(range.averageBPM) | \(range.sampleCount) samples",
        source: .live("BLE heart-rate sample store"),
        systemImage: "heart"
      )
    }
  }

  func refreshPacketInputsAfterCapture() {
    packetInputRefreshWorkItem?.cancel()
    let workItem = DispatchWorkItem { [weak self] in
      self?.runPacketInputs()
    }
    packetInputRefreshWorkItem = workItem
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.8, execute: workItem)
  }

  func refreshBridgeCatalogs() {
    catalogStatus = "Loading bridge catalog..."
    let bridge = self.bridge
    packetInputQueue.async { [weak self] in
      do {
        let algorithmsValue = try bridge.requestValue(method: "metrics.built_in_definitions")
        let referencesValue = try bridge.requestValue(method: "metrics.reference_definitions")
        let preferencesValue = try bridge.requestValue(method: "metrics.default_preferences")

        let parsedAlgorithms = Self.algorithmRows(from: algorithmsValue)
          .map { HealthAlgorithmDefinition(row: $0, source: .bridge("metrics.built_in_definitions")) }
        let parsedReferences = Self.algorithmRows(from: referencesValue)
          .map { HealthAlgorithmDefinition(row: $0, source: .bridge("metrics.reference_definitions")) }
        let parsedPreferences = Self.preferenceRows(from: preferencesValue)

        DispatchQueue.main.async { [weak self] in
          guard let self else { return }
          if !parsedAlgorithms.isEmpty {
            self.algorithmDefinitions = parsedAlgorithms
          }
          if !parsedReferences.isEmpty {
            self.referenceDefinitions = parsedReferences
          }
          if !parsedPreferences.isEmpty {
            self.selectedAlgorithmByFamily = parsedPreferences
          } else {
            self.selectedAlgorithmByFamily = Dictionary(
              uniqueKeysWithValues: self.algorithmDefinitions.map { ($0.family, $0.id) }
            )
          }
          self.catalogSource = .bridge("Rust metric registry")
          self.catalogStatus = "Bridge catalog loaded"
        }
      } catch {
        let shortErr = Self.shortError(error)
        DispatchQueue.main.async { [weak self] in
          guard let self else { return }
          self.algorithmDefinitions = []
          self.referenceDefinitions = []
          self.selectedAlgorithmByFamily = [:]
          self.catalogSource = .unavailable("Rust catalog unavailable")
          self.catalogStatus = "Metric catalog unavailable: \(shortErr)"
        }
      }
    }
  }

  func selectAlgorithm(_ algorithmID: String, for family: String) {
    selectedAlgorithmByFamily[family] = algorithmID
  }

  func runPacketInputs(completion: (() -> Void)? = nil) {
    guard !packetInputIsRunning else {
      packetInputStatus = "Packet-derived input extraction already running..."
      completion?()
      return
    }
    packetInputRefreshWorkItem?.cancel()
    let runID = UUID()
    packetInputRunID = runID
    packetInputIsRunning = true
    let databasePath = databasePath
    packetInputStatus = "Extracting packet-derived inputs..."

    packetInputQueue.async { [weak self] in
      let result = HealthDataStore.packetInputBridgeReports(databasePath: databasePath)
      DispatchQueue.main.async { [weak self] in
        guard let self, self.packetInputRunID == runID else {
          return
        }
        self.packetInputIsRunning = false
        switch result {
        case .success(let reports):
          self.packetInputReports = reports
          self.packetInputStatus = "Bridge packet-derived inputs extracted"
        case .failure(let error):
          self.packetInputStatus = "Bridge input extraction blocked: \(HealthDataStore.shortError(error))"
        }
        completion?()
      }
    }
  }

  func markBandSleepSyncRequested(automatic: Bool, canSync: Bool, detail: String) {
    if canSync {
      bandSleepImportStatus = automatic ? "Auto-syncing band sleep packets..." : "Syncing band sleep packets..."
    } else {
      bandSleepImportStatus = "Band sync unavailable: \(detail)"
    }
  }

  func markBandSleepSyncFailed(_ detail: String) {
    bandSleepImportStatus = "Band sync failed: \(detail)"
  }

  func refreshSleepAfterBandSync(packetCount: Int) {
    bandSleepImportStatus = "Band sync captured \(packetCount) packets | extracting sleep inputs..."
    runPacketInputs { [weak self] in
      guard let self else {
        return
      }
      self.runSleepScore()
      self.bandSleepImportStatus = "Band sync captured \(packetCount) packets | \(self.packetScoreStatus)"
    }
  }
}
