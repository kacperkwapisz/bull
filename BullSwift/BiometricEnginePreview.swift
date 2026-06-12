import SwiftUI

// MARK: - View model

/// Drives the Biometric Engine preview by calling the new Rust bridge methods
/// (P5/P5c/P1 engine) directly and rendering whatever they return — including
/// honest calibrating / insufficient-data states when there is not yet enough
/// device data. Each call is independent so one failure never blanks the screen.
@MainActor
final class BiometricEnginePreviewModel: ObservableObject {
  struct BaselineState {
    let hrvMean: Double?
    let hrvNights: Int
    let hrvTrust: String
    let rhrMean: Double?
    let rhrNights: Int
    let rhrTrust: String
  }

  struct RecoveryState {
    let score: Double?
    let trustLevel: String
    let colourBand: String
    let zHrv: Double?
    let zRhr: Double?
  }

  struct ReadinessState {
    let acwr: Double?
    let acwrZone: String
    let monotony: Double?
    let monotonyHigh: Bool
    let level: String
    let insufficient: Bool
    let dayCount: Int
  }

  @Published var baseline: BaselineState?
  @Published var baselineError: String?
  @Published var recovery: RecoveryState?
  @Published var recoveryNote: String?
  @Published var readiness: ReadinessState?
  @Published var readinessError: String?
  @Published var isLoading = false

  func load(store: HealthDataStore, deviceID: String?) {
    isLoading = true
    let bridge = store.bridge
    let databasePath = store.databasePath
    let baseArgs = store.bridgeBaseArgs(requireTrustedEvidence: false)
    let recoveryArgs = baseArgs.merging(store.recoveryScoreBridgeArgs()) { _, new in new }
    let cachedRecovery = store.packetScoreReports["recovery"]
    let cachedStrain = store.packetScoreReports["strain"]

    DispatchQueue.global(qos: .userInitiated).async {
      // 1) EWMA personal baseline (database_path only — always available).
      var baseline: BaselineState?
      var baselineError: String?
      do {
        let report = try bridge.request(
          method: "store.ewma_baseline_fold_history",
          args: ["database_path": databasePath]
        )
        baseline = Self.parseBaseline(report)
      } catch {
        baselineError = Self.shortError(error)
      }

      // 2) Recovery v1 — needs today's HRV RMSSD + resting HR. Source them from a
      //    fresh recovery feature report; fall back to any cached one.
      var recovery: RecoveryState?
      var recoveryNote: String?
      let recoveryReport = (try? bridge.request(
        method: "metrics.recovery_score_from_features",
        args: recoveryArgs
      )) ?? cachedRecovery
      let hrv = recoveryReport.flatMap(Self.extractHrvRmssd)
      let rhr = recoveryReport.flatMap(Self.extractRestingHr)
      if let hrv, let rhr, let deviceID {
        do {
          let report = try bridge.request(
            method: "metrics.bull_recovery_v1",
            args: [
              "database_path": databasePath,
              "device_id": deviceID,
              "date_key": Self.todayKey(),
              "hrv_rmssd_ms": hrv,
              "resting_hr_bpm": rhr,
            ]
          )
          recovery = Self.parseRecovery(report)
        } catch {
          recoveryNote = Self.shortError(error)
        }
      } else {
        recoveryNote = deviceID == nil
          ? "Connect your band to compute recovery."
          : "Waiting for a night with HRV and resting heart rate."
      }

      // 3) Readiness v1 — acute:chronic load. Assemble recent daily strain points.
      var readiness: ReadinessState?
      var readinessError: String?
      let dailyStrain = Self.extractDailyStrain(cachedStrain)
      do {
        let report = try bridge.request(
          method: "metrics.bull_readiness_v1",
          args: ["daily_strain": dailyStrain]
        )
        readiness = Self.parseReadiness(report, dayCount: dailyStrain.count)
      } catch {
        readinessError = Self.shortError(error)
      }

      DispatchQueue.main.async {
        self.baseline = baseline
        self.baselineError = baselineError
        self.recovery = recovery
        self.recoveryNote = recoveryNote
        self.readiness = readiness
        self.readinessError = readinessError
        self.isLoading = false
      }
    }
  }

  // MARK: Parsing helpers

  nonisolated private static func parseBaseline(_ report: [String: Any]) -> BaselineState {
    let hrv = report["hrv"] as? [String: Any]
    let rhr = report["resting_hr"] as? [String: Any]
    return BaselineState(
      hrvMean: number(hrv?["mean"]).flatMap { $0 > 0 ? $0 : nil },
      hrvNights: Int(number(hrv?["night_count"]) ?? 0),
      hrvTrust: (hrv?["trust"] as? String) ?? "calibrating",
      rhrMean: number(rhr?["mean"]).flatMap { $0 > 0 ? $0 : nil },
      rhrNights: Int(number(rhr?["night_count"]) ?? 0),
      rhrTrust: (rhr?["trust"] as? String) ?? "calibrating"
    )
  }

  nonisolated private static func parseRecovery(_ report: [String: Any]) -> RecoveryState {
    RecoveryState(
      score: number(report["score_0_to_100"]),
      trustLevel: (report["trust_level"] as? String) ?? "calibrating",
      colourBand: (report["colour_band"] as? String) ?? "amarelo",
      zHrv: number(report["z_hrv"]),
      zRhr: number(report["z_rhr"])
    )
  }

  nonisolated private static func parseReadiness(_ report: [String: Any], dayCount: Int) -> ReadinessState {
    ReadinessState(
      acwr: number(report["acwr"]),
      acwrZone: (report["acwr_zone"] as? String) ?? "unknown",
      monotony: number(report["monotony"]),
      monotonyHigh: (report["monotony_high"] as? Bool) ?? false,
      level: (report["level"] as? String) ?? "unknown",
      insufficient: (report["insufficient_data"] as? Bool) ?? true,
      dayCount: dayCount
    )
  }

  nonisolated private static func extractHrvRmssd(_ report: [String: Any]) -> Double? {
    for key in ["local_hrv_rmssd_ms", "hrv_rmssd_ms", "rmssd_ms"] {
      if let v = number(report[key]), v > 0 { return v }
    }
    return nil
  }

  nonisolated private static func extractRestingHr(_ report: [String: Any]) -> Double? {
    for key in ["resting_hr_bpm", "local_resting_hr_bpm"] {
      if let v = number(report[key]), v > 0 { return v }
    }
    return nil
  }

  /// Build `[[unix_seconds, strain], ...]` from any cached strain report. A single
  /// point is enough to render an honest "building" readiness state.
  nonisolated private static func extractDailyStrain(_ report: [String: Any]?) -> [[Double]] {
    guard let report else { return [] }
    if let series = report["daily_strain"] as? [[Any]] {
      return series.compactMap { pair in
        guard pair.count >= 2, let ts = number(pair[0]), let s = number(pair[1]) else { return nil }
        return [ts, s]
      }
    }
    if let strain = number(report["strain_0_to_21"]) ?? number(report["score_0_to_21"]) {
      return [[Date().timeIntervalSince1970, strain]]
    }
    return []
  }

  nonisolated private static func shortError(_ error: Error) -> String {
    let text = String(describing: error)
    return text.count > 96 ? "\(text.prefix(96))…" : text
  }

  nonisolated private static func number(_ value: Any?) -> Double? {
    if let d = value as? Double { return d }
    if let i = value as? Int { return Double(i) }
    if let n = value as? NSNumber { return n.doubleValue }
    return nil
  }

  nonisolated private static func todayKey() -> String {
    let formatter = DateFormatter()
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.dateFormat = "yyyy-MM-dd"
    return formatter.string(from: Date())
  }
}

// MARK: - View

struct BiometricEnginePreviewView: View {
  @ObservedObject var healthStore: HealthDataStore
  @EnvironmentObject private var model: BullAppModel
  @StateObject private var engine = BiometricEnginePreviewModel()

  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: 16) {
        Text("Computed live from your band's own sensor data. Metrics calibrate as more nights are recorded; honest empty states show until then.")
          .font(.system(size: 13))
          .foregroundStyle(.secondary)
          .padding(.horizontal, 4)

        baselineCard
        recoveryCard
        readinessCard

        if engine.isLoading {
          HStack(spacing: 8) {
            ProgressView()
            Text("Computing…").font(.system(size: 13)).foregroundStyle(.secondary)
          }
          .frame(maxWidth: .infinity)
          .padding(.top, 4)
        }
      }
      .padding(16)
    }
    .bullScreenBackground()
    .navigationTitle("Biometric Engine")
    .navigationBarTitleDisplayMode(.inline)
    .onAppear {
      model.recordUIAction("page.opened", detail: "BiometricEnginePreview")
      engine.load(store: healthStore, deviceID: model.ble.activeDeviceIdentifier?.uuidString)
    }
    .refreshable {
      engine.load(store: healthStore, deviceID: model.ble.activeDeviceIdentifier?.uuidString)
    }
  }

  // MARK: Cards

  private var baselineCard: some View {
    EngineCard(icon: "waveform.path.ecg.rectangle", tint: .green, title: "Personal Baseline") {
      if let b = engine.baseline {
        EngineMetricRow(
          label: "HRV (RMSSD)",
          value: b.hrvMean.map { "\(Int($0.rounded())) ms" } ?? "Calibrating",
          caption: "\(b.hrvNights) nights · \(trustLabel(b.hrvTrust))"
        )
        Divider().overlay(Color.primary.opacity(0.06))
        EngineMetricRow(
          label: "Resting HR",
          value: b.rhrMean.map { "\(Int($0.rounded())) bpm" } ?? "Calibrating",
          caption: "\(b.rhrNights) nights · \(trustLabel(b.rhrTrust))"
        )
      } else if let err = engine.baselineError {
        EngineUnavailable(text: err)
      } else {
        EngineUnavailable(text: "No nights recorded yet.")
      }
    }
  }

  private var recoveryCard: some View {
    EngineCard(icon: "heart.fill", tint: colourTint(engine.recovery?.colourBand), title: "Recovery v1") {
      if let r = engine.recovery {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
          Text(r.score.map { "\(Int($0.rounded()))" } ?? "—")
            .font(.system(size: 38, weight: .heavy, design: .rounded))
            .foregroundStyle(colourTint(r.colourBand))
          Text(r.score == nil ? "Calibrating" : "/ 100")
            .font(.system(size: 15, weight: .semibold))
            .foregroundStyle(.secondary)
        }
        EngineMetricRow(
          label: "Confidence",
          value: trustLabel(r.trustLevel),
          caption: zCaption(r.zHrv, r.zRhr)
        )
      } else {
        EngineUnavailable(text: engine.recoveryNote ?? "Waiting for data.")
      }
    }
  }

  private var readinessCard: some View {
    EngineCard(icon: "bolt.heart.fill", tint: .orange, title: "Readiness v1") {
      if let r = engine.readiness, !r.insufficient {
        EngineMetricRow(
          label: "State",
          value: readinessLabel(r.level),
          caption: r.acwr.map { String(format: "Load ratio %.2f · %@", $0, zoneLabel(r.acwrZone)) } ?? zoneLabel(r.acwrZone)
        )
        if let m = r.monotony {
          Divider().overlay(Color.primary.opacity(0.06))
          EngineMetricRow(
            label: "Monotony",
            value: String(format: "%.2f", m),
            caption: r.monotonyHigh ? "Elevated — vary your training" : "Healthy variety"
          )
        }
      } else {
        EngineUnavailable(text: "Building — \(engine.readiness?.dayCount ?? 0) of 28 days of training load.")
      }
    }
  }

  // MARK: Formatting

  private func trustLabel(_ raw: String) -> String {
    switch raw {
    case "trusted": "Trusted"
    case "provisional": "Provisional"
    default: "Calibrating"
    }
  }

  private func zCaption(_ zHrv: Double?, _ zRhr: Double?) -> String {
    var parts: [String] = []
    if let zHrv { parts.append(String(format: "HRV z %+.1f", zHrv)) }
    if let zRhr { parts.append(String(format: "RHR z %+.1f", zRhr)) }
    return parts.isEmpty ? "Against your baseline" : parts.joined(separator: " · ")
  }

  private func colourTint(_ band: String?) -> Color {
    switch band {
    case "verde": Color(red: 0.20, green: 0.68, blue: 0.27)
    case "vermelho": Color(red: 0.90, green: 0.26, blue: 0.21)
    default: Color(red: 0.95, green: 0.62, blue: 0.10)
    }
  }

  private func readinessLabel(_ raw: String) -> String {
    raw.replacingOccurrences(of: "_", with: " ").capitalized
  }

  private func zoneLabel(_ raw: String) -> String {
    raw.replacingOccurrences(of: "_", with: " ").capitalized
  }
}

// MARK: - Reusable card pieces

private struct EngineCard<Content: View>: View {
  let icon: String
  let tint: Color
  let title: String
  @ViewBuilder var content: Content

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack(spacing: 10) {
        Image(systemName: icon)
          .font(.system(size: 15, weight: .semibold))
          .foregroundStyle(tint)
          .frame(width: 34, height: 34)
          .background(tint.opacity(0.16), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        Text(title)
          .font(.system(size: 17, weight: .bold))
          .foregroundStyle(.primary)
      }
      content
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .padding(16)
    .background(BullTheme.plainBackground, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    .overlay(
      RoundedRectangle(cornerRadius: 18, style: .continuous)
        .strokeBorder(Color.primary.opacity(0.06), lineWidth: 1)
    )
  }
}

private struct EngineMetricRow: View {
  let label: String
  let value: String
  let caption: String

  var body: some View {
    HStack(alignment: .firstTextBaseline) {
      VStack(alignment: .leading, spacing: 2) {
        Text(label).font(.system(size: 14, weight: .semibold)).foregroundStyle(.primary)
        Text(caption).font(.system(size: 12)).foregroundStyle(.secondary)
      }
      Spacer(minLength: 8)
      Text(value)
        .font(.system(size: 16, weight: .bold, design: .rounded))
        .foregroundStyle(.primary)
    }
  }
}

private struct EngineUnavailable: View {
  let text: String

  var body: some View {
    Text(text)
      .font(.system(size: 13))
      .foregroundStyle(.secondary)
      .frame(maxWidth: .infinity, alignment: .leading)
  }
}
