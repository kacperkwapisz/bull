import SwiftUI

// MARK: - View model

/// Surfaces the raw per-second biometric streams the connected device exposes
/// over Bluetooth (SpO2, skin temperature, respiration, gravity) after they have
/// been decoded and ingested into the local typed sample tables. Every value is
/// uncalibrated and derived solely from the device's own sensors; nothing is
/// imported from third-party health stores. Honest unavailable states show until
/// the device has streamed data.
@MainActor
final class DeviceBiometricsModel: ObservableObject {
  struct StreamSummary {
    let spo2Count: Int
    let latestSpo2Pct: Double?
    let skinTempCount: Int
    let latestSkinTempC: Double?
    let respCount: Int
    let gravityCount: Int
    let gravity2Count: Int
  }

  @Published var summary: StreamSummary?
  @Published var errorText: String?
  @Published var isLoading = false

  func load(store: HealthDataStore) {
    isLoading = true
    let bridge = store.bridge
    let databasePath = store.databasePath
    let deviceID = HealthDataStore.localBiometricDeviceID

    DispatchQueue.global(qos: .userInitiated).async {
      // Read-only: counts + latest raw readings are computed with SQL aggregates
      // in Rust, so the full sample history is never paged across the bridge
      // (avoids large allocations after a long historical sync). Ingest is owned
      // by the packet-input pipeline; this surface never writes.
      var summary: StreamSummary?
      var errorText: String?
      do {
        let rollup = try bridge.request(
          method: "biometrics.stream_summary",
          args: [
            "database_path": databasePath,
            "device_id": deviceID,
          ]
        )

        // Latest SpO2 via the uncalibrated raw conversion bridge (one tiny call).
        var latestSpo2: Double?
        if let red = Self.intValue(rollup["latest_spo2_red"]),
          let ir = Self.intValue(rollup["latest_spo2_ir"]) {
          let converted = try? bridge.request(
            method: "biometrics.spo2_from_raw",
            args: ["red": red, "ir": ir]
          )
          latestSpo2 = Self.number(converted?["spo2_pct"])
        }

        // Latest skin temperature: raw ADC / 128 = degrees Celsius (uncalibrated).
        let latestSkinTempC = Self.number(rollup["latest_skin_temp_raw"]).map { $0 / 128.0 }

        summary = StreamSummary(
          spo2Count: Self.intValue(rollup["spo2_count"]) ?? 0,
          latestSpo2Pct: latestSpo2,
          skinTempCount: Self.intValue(rollup["skin_temp_count"]) ?? 0,
          latestSkinTempC: latestSkinTempC,
          respCount: Self.intValue(rollup["resp_count"]) ?? 0,
          gravityCount: Self.intValue(rollup["gravity_count"]) ?? 0,
          gravity2Count: Self.intValue(rollup["gravity2_count"]) ?? 0
        )
      } catch {
        errorText = Self.shortError(error)
      }

      DispatchQueue.main.async {
        self.summary = summary
        self.errorText = errorText
        self.isLoading = false
      }
    }
  }

  // MARK: Parsing helpers

  nonisolated private static func intValue(_ value: Any?) -> Int? {
    if let i = value as? Int { return i }
    if let d = value as? Double { return Int(d) }
    if let n = value as? NSNumber { return n.intValue }
    return nil
  }

  nonisolated private static func number(_ value: Any?) -> Double? {
    if let d = value as? Double { return d }
    if let i = value as? Int { return Double(i) }
    if let n = value as? NSNumber { return n.doubleValue }
    return nil
  }

  nonisolated private static func shortError(_ error: Error) -> String {
    let text = String(describing: error)
    return text.count > 96 ? "\(text.prefix(96))…" : text
  }
}

// MARK: - View

struct DeviceBiometricsView: View {
  @ObservedObject var healthStore: HealthDataStore
  @EnvironmentObject private var model: BullAppModel
  @StateObject private var engine = DeviceBiometricsModel()

  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: 16) {
        Text("Per-second streams parsed from your band's own sensor data over Bluetooth and stored locally. All values are uncalibrated; honest empty states show until the band has streamed data.")
          .font(.system(size: 13))
          .foregroundStyle(.secondary)
          .padding(.horizontal, 4)

        opticalCard
        motionCard

        if engine.isLoading {
          HStack(spacing: 8) {
            ProgressView()
            Text("Reading device streams…")
              .font(.system(size: 13))
              .foregroundStyle(.secondary)
          }
          .frame(maxWidth: .infinity)
          .padding(.top, 4)
        }

        if let err = engine.errorText {
          DBUnavailable(text: err)
        }
      }
      .padding(16)
    }
    .bullScreenBackground()
    .navigationTitle("Device Biometrics")
    .navigationBarTitleDisplayMode(.inline)
    .onAppear {
      model.recordUIAction("page.opened", detail: "DeviceBiometrics")
      engine.load(store: healthStore)
    }
    .refreshable {
      engine.load(store: healthStore)
    }
  }

  private var opticalCard: some View {
    DBCard(icon: "drop.fill", tint: .red, title: "Optical & Temperature") {
      if let s = engine.summary {
        DBMetricRow(
          label: "SpO₂",
          value: s.latestSpo2Pct.map { String(format: "%.0f%%", $0) } ?? (s.spo2Count > 0 ? "—" : "No data"),
          caption: "\(s.spo2Count) samples · uncalibrated"
        )
        Divider().overlay(Color.primary.opacity(0.06))
        DBMetricRow(
          label: "Skin temperature",
          value: s.latestSkinTempC.map { String(format: "%.1f °C", $0) } ?? (s.skinTempCount > 0 ? "—" : "No data"),
          caption: "\(s.skinTempCount) samples · uncalibrated"
        )
        Divider().overlay(Color.primary.opacity(0.06))
        DBMetricRow(
          label: "Respiration",
          value: s.respCount > 0 ? "\(s.respCount) samples" : "No data",
          caption: "raw · uncalibrated"
        )
      } else if engine.errorText == nil {
        DBUnavailable(text: "No optical data streamed yet.")
      }
    }
  }

  private var motionCard: some View {
    DBCard(icon: "gyroscope", tint: .blue, title: "Gravity (motion)") {
      if let s = engine.summary {
        DBMetricRow(
          label: "Primary gravity",
          value: s.gravityCount > 0 ? "\(s.gravityCount) samples" : "No data",
          caption: "feeds sleep staging"
        )
        Divider().overlay(Color.primary.opacity(0.06))
        DBMetricRow(
          label: "Secondary gravity",
          value: s.gravity2Count > 0 ? "\(s.gravity2Count) samples" : "No data",
          caption: "present in longer historical bodies"
        )
      } else if engine.errorText == nil {
        DBUnavailable(text: "No motion data streamed yet.")
      }
    }
  }
}

// MARK: - Reusable card pieces

private struct DBCard<Content: View>: View {
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

private struct DBMetricRow: View {
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

private struct DBUnavailable: View {
  let text: String

  var body: some View {
    Text(text)
      .font(.system(size: 13))
      .foregroundStyle(.secondary)
      .frame(maxWidth: .infinity, alignment: .leading)
  }
}
