import SwiftUI

// MARK: - View model

/// Runs `metrics.rr_hr_consistency` against locally captured frames and renders
/// the verdict. The verifier proves the V24 history RR-interval scale using the
/// band's own co-located heart rate only (no third-party or official labels), so
/// a `verified` result is the evidence needed to start trusting HRV.
@MainActor
final class RrHrConsistencyDebugModel: ObservableObject {
  struct FrameEvidence: Identifiable {
    let id = UUID()
    let frameID: String
    let reportedHr: Double
    let impliedHr: Double
    let meanRrMs: Double
    let absErrorBpm: Double
    let consistent: Bool
  }

  struct Result {
    let verdict: String
    let candidateFrameCount: Int
    let eligibleFrameCount: Int
    let consistentFrameCount: Int
    let consistencyRatio: Double
    let meanAbsErrorBpm: Double?
    let blockers: [String]
    let nextActions: [String]
    let evidence: [FrameEvidence]
  }

  @Published var result: Result?
  @Published var error: String?
  @Published var isLoading = false

  func run(store: HealthDataStore) {
    isLoading = true
    error = nil
    let bridge = store.bridge
    let databasePath = store.databasePath

    DispatchQueue.global(qos: .userInitiated).async {
      var result: Result?
      var errorText: String?
      do {
        let report = try bridge.request(
          method: "metrics.rr_hr_consistency",
          args: [
            "database_path": databasePath,
            "start": "0000",
            "end": "9999",
          ]
        )
        result = Self.parse(report)
      } catch {
        errorText = Self.shortError(error)
      }

      DispatchQueue.main.async {
        self.result = result
        self.error = errorText
        self.isLoading = false
      }
    }
  }

  // MARK: Parsing

  nonisolated private static func parse(_ report: [String: Any]) -> Result {
    let evidenceRaw = report["evidence"] as? [[String: Any]] ?? []
    let evidence = evidenceRaw.map { row in
      FrameEvidence(
        frameID: (row["frame_id"] as? String) ?? "—",
        reportedHr: number(row["reported_hr_bpm"]) ?? 0,
        impliedHr: number(row["implied_hr_bpm"]) ?? 0,
        meanRrMs: number(row["mean_rr_ms"]) ?? 0,
        absErrorBpm: number(row["abs_error_bpm"]) ?? 0,
        consistent: (row["consistent"] as? Bool) ?? false
      )
    }
    return Result(
      verdict: (report["verdict"] as? String) ?? "unknown",
      candidateFrameCount: Int(number(report["candidate_frame_count"]) ?? 0),
      eligibleFrameCount: Int(number(report["eligible_frame_count"]) ?? 0),
      consistentFrameCount: Int(number(report["consistent_frame_count"]) ?? 0),
      consistencyRatio: number(report["consistency_ratio"]) ?? 0,
      meanAbsErrorBpm: number(report["mean_abs_error_bpm"]),
      blockers: (report["blockers"] as? [String]) ?? [],
      nextActions: (report["next_actions"] as? [String]) ?? [],
      evidence: evidence
    )
  }

  nonisolated private static func shortError(_ error: Error) -> String {
    let text = String(describing: error)
    return text.count > 140 ? "\(text.prefix(140))…" : text
  }

  nonisolated private static func number(_ value: Any?) -> Double? {
    if let d = value as? Double { return d }
    if let i = value as? Int { return Double(i) }
    if let n = value as? NSNumber { return n.doubleValue }
    return nil
  }
}

// MARK: - View

struct RrHrConsistencyDebugView: View {
  @ObservedObject var healthStore: HealthDataStore
  @EnvironmentObject private var model: BullAppModel
  @StateObject private var verifier = RrHrConsistencyDebugModel()

  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: 16) {
        Text("Proves the band's V24 RR-interval scale by checking that 60000 / mean(RR) reproduces the device's own reported heart rate. Uses device-internal data only — no official or third-party labels. A verified result is the evidence to start trusting HRV.")
          .font(.system(size: 13))
          .foregroundStyle(.secondary)
          .padding(.horizontal, 4)

        if let r = verifier.result {
          verdictCard(r)
          countsCard(r)
          if !r.nextActions.isEmpty || !r.blockers.isEmpty {
            actionsCard(r)
          }
          if !r.evidence.isEmpty {
            evidenceCard(r)
          }
        } else if let err = verifier.error {
          RrHrCard(icon: "exclamationmark.triangle.fill", tint: .orange, title: "Could not run") {
            Text(err).font(.system(size: 13)).foregroundStyle(.secondary)
          }
        } else if !verifier.isLoading {
          RrHrCard(icon: "waveform.path.ecg", tint: .secondary, title: "Not run yet") {
            Text("Pull to refresh or tap Run to evaluate captured frames.")
              .font(.system(size: 13)).foregroundStyle(.secondary)
          }
        }

        if verifier.isLoading {
          HStack(spacing: 8) {
            ProgressView()
            Text("Evaluating captured frames…").font(.system(size: 13)).foregroundStyle(.secondary)
          }
          .frame(maxWidth: .infinity)
          .padding(.top, 4)
        }
      }
      .padding(16)
    }
    .bullScreenBackground()
    .navigationTitle("HRV Scale Check")
    .navigationBarTitleDisplayMode(.inline)
    .toolbar {
      ToolbarItem(placement: .topBarTrailing) {
        Button("Run") { verifier.run(store: healthStore) }
          .disabled(verifier.isLoading)
      }
    }
    .onAppear {
      model.recordUIAction("page.opened", detail: "RrHrConsistencyDebug")
      verifier.run(store: healthStore)
    }
    .refreshable {
      verifier.run(store: healthStore)
    }
  }

  // MARK: Cards

  private func verdictCard(_ r: RrHrConsistencyDebugModel.Result) -> some View {
    RrHrCard(icon: verdictIcon(r.verdict), tint: verdictTint(r.verdict), title: "Verdict") {
      Text(verdictLabel(r.verdict))
        .font(.system(size: 26, weight: .heavy, design: .rounded))
        .foregroundStyle(verdictTint(r.verdict))
      Text(verdictExplanation(r.verdict))
        .font(.system(size: 13)).foregroundStyle(.secondary)
    }
  }

  private func countsCard(_ r: RrHrConsistencyDebugModel.Result) -> some View {
    RrHrCard(icon: "list.number", tint: .blue, title: "Frames") {
      RrHrMetricRow(label: "Candidate V24 frames", value: "\(r.candidateFrameCount)")
      Divider().overlay(Color.primary.opacity(0.06))
      RrHrMetricRow(label: "Eligible (HR + RR)", value: "\(r.eligibleFrameCount)")
      Divider().overlay(Color.primary.opacity(0.06))
      RrHrMetricRow(
        label: "Consistent",
        value: "\(r.consistentFrameCount) (\(Int((r.consistencyRatio * 100).rounded()))%)"
      )
      if let mean = r.meanAbsErrorBpm {
        Divider().overlay(Color.primary.opacity(0.06))
        RrHrMetricRow(label: "Mean HR error", value: String(format: "%.1f bpm", mean))
      }
    }
  }

  private func actionsCard(_ r: RrHrConsistencyDebugModel.Result) -> some View {
    RrHrCard(icon: "arrow.right.circle.fill", tint: .indigo, title: "Next") {
      ForEach(Array(r.blockers.enumerated()), id: \.offset) { _, b in
        Label(humanize(b), systemImage: "lock.fill")
          .font(.system(size: 13)).foregroundStyle(.secondary)
      }
      ForEach(Array(r.nextActions.enumerated()), id: \.offset) { _, a in
        Label(a, systemImage: "checkmark.circle")
          .font(.system(size: 13)).foregroundStyle(.primary)
      }
    }
  }

  private func evidenceCard(_ r: RrHrConsistencyDebugModel.Result) -> some View {
    RrHrCard(icon: "tablecells", tint: .teal, title: "Sample frames") {
      ForEach(r.evidence.prefix(12)) { e in
        HStack(alignment: .firstTextBaseline) {
          Image(systemName: e.consistent ? "checkmark.circle.fill" : "xmark.circle.fill")
            .font(.system(size: 12))
            .foregroundStyle(e.consistent ? Color.green : Color.red)
          VStack(alignment: .leading, spacing: 1) {
            Text(String(format: "HR %.0f vs implied %.0f bpm", e.reportedHr, e.impliedHr))
              .font(.system(size: 13, weight: .semibold))
            Text(String(format: "mean RR %.0f ms · err %.1f bpm", e.meanRrMs, e.absErrorBpm))
              .font(.system(size: 11)).foregroundStyle(.secondary)
          }
          Spacer(minLength: 4)
        }
      }
    }
  }

  // MARK: Formatting

  private func verdictLabel(_ raw: String) -> String {
    switch raw {
    case "verified": "Verified"
    case "inconsistent": "Inconsistent"
    case "insufficient_data": "Insufficient data"
    default: "Unknown"
    }
  }

  private func verdictExplanation(_ raw: String) -> String {
    switch raw {
    case "verified": "RR intervals reproduce device HR in milliseconds. The V24 RR field can be promoted to a trusted HRV source."
    case "inconsistent": "Implied HR disagrees with reported HR too often. Do not treat the V24 RR field as milliseconds yet."
    default: "Not enough worn frames carrying both HR and RR. Wear the band and sync, then re-run."
    }
  }

  private func verdictIcon(_ raw: String) -> String {
    switch raw {
    case "verified": "checkmark.seal.fill"
    case "inconsistent": "xmark.seal.fill"
    default: "hourglass"
    }
  }

  private func verdictTint(_ raw: String) -> Color {
    switch raw {
    case "verified": Color(red: 0.20, green: 0.68, blue: 0.27)
    case "inconsistent": Color(red: 0.90, green: 0.26, blue: 0.21)
    default: Color(red: 0.95, green: 0.62, blue: 0.10)
    }
  }

  private func humanize(_ raw: String) -> String {
    raw.replacingOccurrences(of: "_", with: " ").capitalized
  }
}

// MARK: - Reusable card pieces

private struct RrHrCard<Content: View>: View {
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

private struct RrHrMetricRow: View {
  let label: String
  let value: String

  var body: some View {
    HStack(alignment: .firstTextBaseline) {
      Text(label).font(.system(size: 14, weight: .semibold)).foregroundStyle(.primary)
      Spacer(minLength: 8)
      Text(value)
        .font(.system(size: 16, weight: .bold, design: .rounded))
        .foregroundStyle(.primary)
    }
  }
}
