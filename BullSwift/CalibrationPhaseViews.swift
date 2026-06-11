import SwiftUI

extension BullBLEClient {
  var isConnectedForUserBaseline: Bool {
    connectedAt != nil
  }
}

enum CalibrationMetricRoute: String, CaseIterable, Hashable {
  case sleep
  case recovery
  case strain
  case stress

  var healthRoute: HealthRoute {
    switch self {
    case .sleep: .sleep
    case .recovery: .recovery
    case .strain: .strain
    case .stress: .stress
    }
  }

  var title: String {
    healthRoute.title
  }

  var systemImage: String {
    healthRoute.systemImage
  }

  func actionLine(dayIndex: Int, daysRequired: Int) -> String {
    let day = min(max(dayIndex, 1), daysRequired)
    switch self {
    case .sleep:
      return "Night \(day) of \(daysRequired) — wear your band to bed for Sleep."
    case .recovery:
      return "Morning \(day) of \(daysRequired) — keep the band on overnight for Recovery."
    case .strain:
      return "Day \(day) of \(daysRequired) — move normally; Strain fills in as you go."
    case .stress:
      return "Day \(day) of \(daysRequired) — heart-rate history builds your Stress baseline."
    }
  }
}

/// Immutable baseline UI state — computed once per data refresh, not on every body pass.
struct CalibrationUISnapshot: Equatable {
  var shouldShowBanner: Bool
  var showConnectBandPrompt: Bool
  var isComplete: Bool
  var hasStartedBaseline: Bool
  var dayIndex: Int
  var daysRequired: Int
  /// Ring fill: share of core metrics that have enough local history (0…1).
  var metricsReadyProgress: CGFloat
  var metricsReadyCount: Int
  var metricsTotalCount: Int
  var homeActionLine: String
  var metricReady: [CalibrationMetricRoute: Bool]
  var shouldCelebrateCompletion: Bool

  static let inactive = CalibrationUISnapshot(
    shouldShowBanner: false,
    showConnectBandPrompt: false,
    isComplete: true,
    hasStartedBaseline: false,
    dayIndex: 1,
    daysRequired: 4,
    metricsReadyProgress: 1,
    metricsReadyCount: 4,
    metricsTotalCount: 4,
    homeActionLine: "",
    metricReady: Dictionary(uniqueKeysWithValues: CalibrationMetricRoute.allCases.map { ($0, true) }),
    shouldCelebrateCompletion: false
  )

  /// Full hero replacement only when this metric still lacks enough data.
  func shouldShowHeroOverlay(for route: CalibrationMetricRoute) -> Bool {
    !isComplete && !(metricReady[route] ?? true)
  }

  func isMetricReady(_ route: CalibrationMetricRoute) -> Bool {
    metricReady[route] ?? true
  }

  var isInUserBaselinePhase: Bool {
    hasStartedBaseline && !isComplete
  }

  /// Home tri-row line while baseline is building (banner may be dismissed).
  var homeTriVerdict: String? {
    if showConnectBandPrompt {
      return "Connect your band to start building your baseline."
    }
    guard isInUserBaselinePhase, !homeActionLine.isEmpty else { return nil }
    return homeActionLine
  }

  func coachTipMessage(for route: CalibrationMetricRoute) -> String {
    route.actionLine(dayIndex: dayIndex, daysRequired: daysRequired)
  }

  static func metricRoute(for healthRoute: HealthRoute) -> CalibrationMetricRoute? {
    CalibrationMetricRoute.allCases.first { $0.healthRoute == healthRoute }
  }
}

@MainActor
final class CalibrationManager: ObservableObject {
  static let shared = CalibrationManager()

  @AppStorage("bull.calibration.startDateUnix") private var startDateUnix: Double = 0
  @AppStorage("bull.calibration.daysRequired") var daysRequired: Int = 4 {
    didSet { uiSnapshot = Self.inactivePlaceholder(daysRequired: daysRequired) }
  }
  @AppStorage("bull.calibration.manuallyComplete") private var manuallyComplete = false
  @AppStorage("bull.calibration.completionCelebrated") private var completionCelebrated = false
  @AppStorage("bull.calibration.bannerDismissed") var bannerDismissed = false

  @Published private(set) var uiSnapshot = CalibrationUISnapshot.inactive

  private let metricRequiredCount = 3
  private let metricsTotal = CalibrationMetricRoute.allCases.count

  var startDate: Date? {
    guard startDateUnix > 0 else { return nil }
    return Date(timeIntervalSince1970: startDateUnix)
  }

  /// Starts the user baseline clock only after a real band connection timestamp exists.
  func ensureStarted(connectedAt: Date?) {
    guard startDateUnix <= 0, let connectedAt else { return }
    startDateUnix = connectedAt.timeIntervalSince1970
    bannerDismissed = false
    completionCelebrated = false
    manuallyComplete = false
  }

  func refreshUISnapshot(store: HealthDataStore, isBandConnected: Bool) {
    let next = bullSignpostMeasure(BullSignpost.ui, "calibrationUISnapshot") {
      buildUISnapshot(store: store, isBandConnected: isBandConnected)
    }
    if manuallyComplete != next.isComplete {
      manuallyComplete = next.isComplete
    }
    if uiSnapshot != next {
      uiSnapshot = next
    }
  }

  func markCompletionCelebrated() {
    completionCelebrated = true
    var copy = uiSnapshot
    copy.shouldCelebrateCompletion = false
    uiSnapshot = copy
  }

  private func buildUISnapshot(store: HealthDataStore, isBandConnected: Bool) -> CalibrationUISnapshot {
    let readyMap = Dictionary(
      uniqueKeysWithValues: CalibrationMetricRoute.allCases.map { route in
        (route, store.calibrationMetricReady(route.healthRoute, requiredCount: metricRequiredCount))
      }
    )
    let readyCount = readyMap.values.filter { $0 }.count
    let allReady = readyCount == metricsTotal
    let complete = manuallyComplete || allReady
    let started = startDate != nil

    let dayIdx: Int
    if let start = startDate {
      let calendar = Calendar.current
      let startDay = calendar.startOfDay(for: start)
      let today = calendar.startOfDay(for: Date())
      let elapsed = calendar.dateComponents([.day], from: startDay, to: today).day ?? 0
      dayIdx = min(max(elapsed + 1, 1), daysRequired)
    } else {
      dayIdx = 1
    }

    let metricsProgress = CGFloat(readyCount) / CGFloat(max(metricsTotal, 1))
    let showConnect = !isBandConnected && !complete && !started
    let showBanner = started && !complete && !bannerDismissed

    return CalibrationUISnapshot(
      shouldShowBanner: showBanner,
      showConnectBandPrompt: showConnect,
      isComplete: complete,
      hasStartedBaseline: started,
      dayIndex: dayIdx,
      daysRequired: daysRequired,
      metricsReadyProgress: metricsProgress,
      metricsReadyCount: readyCount,
      metricsTotalCount: metricsTotal,
      homeActionLine: homeActionLine(readyMap: readyMap, started: started),
      metricReady: readyMap,
      shouldCelebrateCompletion: complete && !completionCelebrated
    )
  }

  private func homeActionLine(readyMap: [CalibrationMetricRoute: Bool], started: Bool) -> String {
    guard started else { return "" }
    if readyMap[.recovery] != true {
      return "Wear your band tonight — Recovery is still building your baseline."
    }
    if readyMap[.sleep] != true {
      return "Wear your band to bed tonight — Sleep is almost ready."
    }
    if readyMap[.strain] != true {
      return "Move normally today — Strain needs a little more from your band."
    }
    if readyMap[.stress] != true {
      return "Keep the band on today — Stress is learning your rhythm."
    }
    return "You're almost there — one more quiet day on the wrist."
  }

  private static func inactivePlaceholder(daysRequired: Int) -> CalibrationUISnapshot {
    var inactive = CalibrationUISnapshot.inactive
    inactive.daysRequired = daysRequired
    return inactive
  }
}

// MARK: - Progress ring

struct CalibrationProgressRing: View, Equatable {
  let progress: CGFloat
  let centerTitle: String
  let centerSubtitle: String?
  let accent: Color
  let track: Color
  var size: CGFloat = 52
  var lineWidth: CGFloat = 5

  var body: some View {
    ZStack {
      Circle()
        .stroke(track.opacity(0.35), lineWidth: lineWidth)
      Circle()
        .trim(from: 0, to: min(max(progress, 0), 1))
        .stroke(
          accent,
          style: StrokeStyle(lineWidth: lineWidth, lineCap: .round)
        )
        .rotationEffect(.degrees(-90))
      VStack(spacing: 0) {
        Text(centerTitle)
          .font(.system(size: size * 0.28, weight: .semibold, design: .rounded))
          .foregroundStyle(accent)
        if let centerSubtitle {
          Text(centerSubtitle)
            .font(.system(size: size * 0.16, weight: .medium, design: .rounded))
            .foregroundStyle(accent.opacity(0.72))
        }
      }
    }
    .frame(width: size, height: size)
  }
}

// MARK: - Home banner

struct CalibrationBanner: View {
  let snapshot: CalibrationUISnapshot
  let palette: SleepV2Palette
  var onDismiss: (() -> Void)?

  var body: some View {
    HStack(alignment: .center, spacing: 14) {
      CalibrationProgressRing(
        progress: snapshot.metricsReadyProgress,
        centerTitle: "\(snapshot.metricsReadyCount)/\(snapshot.metricsTotalCount)",
        centerSubtitle: nil,
        accent: palette.accent,
        track: palette.separator
      )
      .equatable()

      VStack(alignment: .leading, spacing: 4) {
        Text("Getting to know you")
          .font(.subheadline.weight(.semibold))
          .foregroundStyle(palette.text)
        Text("Day \(snapshot.dayIndex) of \(snapshot.daysRequired)")
          .font(.caption.weight(.medium))
          .foregroundStyle(palette.secondaryText)
        Text(snapshot.homeActionLine)
          .font(.caption)
          .foregroundStyle(palette.mutedText)
          .fixedSize(horizontal: false, vertical: true)
      }

      Spacer(minLength: 0)

      if let onDismiss {
        Button(action: onDismiss) {
          Image(systemName: "xmark")
            .font(.caption.weight(.semibold))
            .foregroundStyle(palette.mutedText)
            .padding(8)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Dismiss baseline banner")
      }
    }
    .padding(14)
    .background(
      RoundedRectangle(cornerRadius: 16, style: .continuous)
        .fill(palette.surface.opacity(palette.light ? 0.92 : 0.78))
        .shadow(color: palette.shadow.opacity(0.12), radius: 10, y: 4)
    )
  }
}

struct ConnectBandBaselinePrompt: View {
  let palette: SleepV2Palette

  var body: some View {
    NavigationLink {
      DeviceView()
    } label: {
      HStack(spacing: 12) {
        Image(systemName: "applewatch")
          .font(.title3.weight(.semibold))
          .foregroundStyle(palette.accent)
        VStack(alignment: .leading, spacing: 4) {
          Text("Connect your band")
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(palette.text)
          Text("Open Device to pair and start your baseline.")
            .font(.caption)
            .foregroundStyle(palette.mutedText)
            .multilineTextAlignment(.leading)
        }
        Spacer()
        Image(systemName: "chevron.right")
          .font(.caption.weight(.bold))
          .foregroundStyle(palette.mutedText)
      }
      .padding(14)
      .background(
        RoundedRectangle(cornerRadius: 16, style: .continuous)
          .fill(palette.surface.opacity(palette.light ? 0.92 : 0.78))
      )
    }
    .buttonStyle(.plain)
  }
}

struct BaselineCompletionSheet: View {
  let onDismiss: () -> Void

  var body: some View {
    VStack(spacing: 20) {
      Image(systemName: "checkmark.circle.fill")
        .font(.system(size: 56))
        .foregroundStyle(.green)
      Text("You're all set")
        .font(.title2.weight(.semibold))
      Text("Your scores will keep sharpening as you wear your band. Bull has enough to speak honestly about Sleep, Recovery, Strain, and Stress.")
        .font(.body)
        .foregroundStyle(.secondary)
        .multilineTextAlignment(.center)
        .padding(.horizontal, 8)
      Button("Continue", action: onDismiss)
        .buttonStyle(.borderedProminent)
        .padding(.top, 8)
    }
    .padding(28)
    .presentationDetents([.medium])
  }
}

// MARK: - Metric hero overlay

struct CalibrationHeroOverlay: View, Equatable {
  let palette: SleepV2Palette
  let route: CalibrationMetricRoute
  let dayIndex: Int
  let daysRequired: Int
  let metricsProgress: CGFloat
  let metricsReadyCount: Int
  let metricsTotalCount: Int
  var ringSize: CGFloat = 188

  var body: some View {
    VStack(spacing: 14) {
      CalibrationProgressRing(
        progress: metricsProgress,
        centerTitle: "\(metricsReadyCount)/\(metricsTotalCount)",
        centerSubtitle: "Day \(dayIndex)",
        accent: palette.accent,
        track: palette.separator,
        size: ringSize * 0.72,
        lineWidth: max(6, ringSize * 0.04)
      )
      .equatable()

      Text(route.title)
        .font(.title3.weight(.semibold))
        .foregroundStyle(palette.text)

      Text(route.actionLine(dayIndex: dayIndex, daysRequired: daysRequired))
        .font(.subheadline)
        .multilineTextAlignment(.center)
        .foregroundStyle(palette.secondaryText)
        .padding(.horizontal, 24)
    }
    .frame(maxWidth: .infinity)
    .accessibilityElement(children: .combine)
  }
}

/// Wraps a metric hero gauge with optional baseline overlay and completion fade.
struct CalibrationHeroContainer<Hero: View>: View {
  let snapshot: CalibrationUISnapshot
  let route: CalibrationMetricRoute
  let palette: SleepV2Palette
  var ringSize: CGFloat = 188
  var onCelebrateCompletion: (() -> Void)?
  @ViewBuilder let hero: () -> Hero

  @State private var revealRealHero = false

  private var showOverlay: Bool {
    snapshot.shouldShowHeroOverlay(for: route) && !revealRealHero
  }

  var body: some View {
    ZStack {
      hero()
        .opacity(showOverlay ? 0 : 1)
        .scaleEffect(showOverlay ? 0.96 : 1)
        .animation(.spring(response: 0.32, dampingFraction: 0.86), value: showOverlay)

      if showOverlay {
        CalibrationHeroOverlay(
          palette: palette,
          route: route,
          dayIndex: snapshot.dayIndex,
          daysRequired: snapshot.daysRequired,
          metricsProgress: snapshot.metricsReadyProgress,
          metricsReadyCount: snapshot.metricsReadyCount,
          metricsTotalCount: snapshot.metricsTotalCount,
          ringSize: ringSize
        )
        .equatable()
        .transition(.opacity.combined(with: .scale(scale: 0.98)))
      }
    }
    .onAppear {
      syncRevealState(animated: false)
    }
    .onChange(of: snapshot) { _, _ in
      syncRevealState(animated: true)
    }
  }

  private func syncRevealState(animated: Bool) {
    let shouldReveal = snapshot.isComplete || snapshot.isMetricReady(route)
    guard shouldReveal else {
      revealRealHero = false
      return
    }
    guard !revealRealHero else { return }
    if animated {
      withAnimation(.spring(response: 0.28, dampingFraction: 0.82)) {
        revealRealHero = true
      }
    } else {
      revealRealHero = true
    }
    if snapshot.shouldCelebrateCompletion {
      onCelebrateCompletion?()
    }
  }
}