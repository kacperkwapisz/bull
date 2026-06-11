import SwiftUI

struct HomeDashboardView: View {
  @EnvironmentObject private var model: BullAppModel
  @EnvironmentObject private var router: AppRouter
  @ObservedObject var healthStore: HealthDataStore
  @Binding var selectedDate: Date
  let openHealthRoute: (HealthRoute) -> Void
  @State private var showingScoreDatePicker = false
  @State private var showingCardioLoadSheet = false
  @State private var selectedHealthMonitorTrend: HealthMetricSnapshot?
  @State private var cachedLandingSnapshots: [HealthMetricSnapshot] = []
  @State private var cachedCardioLoadDays: [CardioLoadDay] = []
  @State private var cachedHealthMonitorSnapshots: [HealthMetricSnapshot] = []

  var body: some View {
    let _ = Self.bullPrintChangesIfEnabled()
    // Read the snapshots cached by refreshSnapshots() once per render — avoids
    // recomputing healthStore.landingSnapshots(…) on every SwiftUI body pass.
    let cached = cachedLandingSnapshots
    ScrollView {
      LazyVStack(alignment: .leading, spacing: 18) {
        HomeScoreTriRow(
          strain: datedHomeSnapshot(for: .strain, in: cached),
          recovery: datedHomeSnapshot(for: .recovery, in: cached),
          sleep: datedHomeSnapshot(for: .sleep, in: cached),
          open: openHealth
        )

        if let coachTip = homeCoachTip(using: cached) {
          CoachTipCard(tip: coachTip, showsSource: false) {
            openCoach(coachTip.prompt)
          }
        }

        HomeStressEnergySection(
          stress: landingSnapshot(for: .stress, in: cached),
          energy: landingSnapshot(for: .energyBank, in: cached),
          openStress: { openHealth(.stress) }
        )

        HomeCardioLoadWidget(
          snapshot: landingSnapshot(for: .cardioLoad),
          days: cachedCardioLoadDays
        ) {
          showingCardioLoadSheet = true
          model.recordUIAction("health.sheet.opened", detail: "Cardio Load home widget")
        }

        HomeHealthMonitorSection(
          snapshots: cachedHealthMonitorSnapshots,
          openSnapshot: openHealthMonitorSnapshot
        )

        HomeTimelineSection(
          activities: model.homeActivityTimelineItems,
          openActivity: { openHealth(.strain) }
        )

      }
      .padding(.horizontal, 16)
      .padding(.vertical, 18)
    }
    .scrollClipDisabled()
    .bullScreenBackground()
    .navigationTitle("Today")
    .navigationBarTitleDisplayMode(.inline)
    .toolbarBackground(.hidden, for: .navigationBar)
    .overlay(alignment: .top) {
      HomeTopScrollFade()
        .allowsHitTesting(false)
    }
    .safeAreaInset(edge: .bottom, alignment: .trailing) {
      HomeStartActivityFloatingButton(session: model.activitySession)
        .padding(.trailing, 18)
        .padding(.bottom, 10)
    }
    .toolbar {
      ToolbarItem(placement: .principal) {
        ScoreDateTitleButton(
          title: homeTitle,
          subtitle: nil,
          action: { showingScoreDatePicker = true }
        )
      }
      ToolbarItem(placement: .topBarTrailing) {
        NavigationLink {
          DeviceView()
        } label: {
          Image(systemName: "applewatch")
            .font(.system(size: 17, weight: .semibold))
            .symbolRenderingMode(.monochrome)
            .foregroundStyle(deviceToolbarTint)
        }
        .accessibilityLabel("Device")
        .accessibilityValue(deviceToolbarAccessibilityValue)
      }
    }
    .onAppear {
      model.recordUIAction("page.opened", detail: "Home")
      refreshSnapshots()
    }
    .task {
      healthStore.loadBridgeCatalogsIfNeeded()
      model.refreshActivityTimeline(for: selectedDate)
      refreshSnapshots()
    }
    .onChange(of: selectedDate) { _, newValue in
      model.refreshActivityTimeline(for: newValue)
      refreshSnapshots()
    }
    .onChange(of: model.ble.liveHeartRateBPM) { _, _ in
      refreshSnapshots()
    }
    .onChange(of: healthStore.catalogStatus) { _, _ in
      refreshSnapshots()
    }
    .sheet(isPresented: $showingScoreDatePicker) {
      let cached = cachedLandingSnapshots
      ScoreDatePickerSheet(
        title: "Daily Scores",
        routes: [.sleep, .recovery, .strain],
        snapshots: scorePickerSnapshots(using: cached),
        selectedDate: $selectedDate
      )
    }
    .sheet(isPresented: $showingCardioLoadSheet) {
      CardioLoadSheet(store: healthStore)
    }
    .sheet(item: $selectedHealthMonitorTrend) { snapshot in
      SleepV2BevelTrendSheet(snapshot: snapshot)
    }
  }

  private func scorePickerSnapshots(using cached: [HealthMetricSnapshot]) -> [HealthMetricSnapshot] {
    [
      homeSnapshot(for: .sleep, in: cached),
      homeSnapshot(for: .recovery, in: cached),
      homeSnapshot(for: .strain, in: cached),
    ]
  }

  private var homeTitle: String {
    ScoreDateTimeline.dateLabel(for: selectedDate)
  }

  private var deviceToolbarTint: Color {
    deviceToolbarConnected ? .green : .red
  }

  private var deviceToolbarAccessibilityValue: String {
    deviceToolbarConnected ? "Connected" : "Disconnected"
  }

  private var deviceToolbarConnected: Bool {
    let state = model.ble.connectionState.lowercased()
    return state == "ready" || state == "connected"
  }

  /// Coach card for the Home surface. Hidden while there is no Recovery score —
  /// the hero already carries the single next action, and the alpha screen should
  /// stay calm rather than repeat itself.
  private func homeCoachTip(using cached: [HealthMetricSnapshot]) -> CoachInlineTip? {
    let recovery = homeSnapshot(for: .recovery, in: cached)
    guard recovery.source.kind != .unavailable else {
      return nil
    }
    let base = CoachTipFactory.homeTip(healthStore: healthStore, appModel: model)
    let sleep = homeSnapshot(for: .sleep, in: cached)
    let strain = homeSnapshot(for: .strain, in: cached)
    return CoachInlineTip(
      id: base.id,
      title: "Coach",
      message: "Sleep \(sleep.displayValue) \u{00B7} Recovery \(recovery.displayValue) \u{00B7} Strain \(strain.displayValue). Ask Coach how to spend today.",
      source: "",
      prompt: base.prompt,
      systemImage: base.systemImage,
      tint: base.tint
    )
  }

  private func refreshSnapshots() {
    cachedLandingSnapshots = healthStore.landingSnapshots(
      liveHeartRateBPM: model.ble.liveHeartRateBPM,
      liveHeartRateSource: model.ble.liveHeartRateSource,
      liveHeartRateUpdatedAt: model.ble.liveHeartRateUpdatedAt,
      stableDailyMetrics: true
    )
    cachedCardioLoadDays = healthStore.cardioLoadWeeklyPoints()
    cachedHealthMonitorSnapshots = healthStore.healthMonitorSnapshots(allowLiveFallbacks: false)
  }

  private func landingSnapshot(for route: HealthRoute) -> HealthMetricSnapshot {
    cachedLandingSnapshots.first { $0.route == route } ?? healthStore.snapshot(for: route)
  }

  private func landingSnapshot(for route: HealthRoute, in snapshots: [HealthMetricSnapshot]) -> HealthMetricSnapshot {
    snapshots.first { $0.route == route } ?? healthStore.snapshot(for: route)
  }

  private func homeSnapshot(for route: HealthRoute, in snapshots: [HealthMetricSnapshot]) -> HealthMetricSnapshot {
    let snapshot = landingSnapshot(for: route, in: snapshots)
    guard route == .strain, snapshot.unit != "%" else {
      return snapshot
    }
    let rawValue = firstNumber(in: snapshot.displayValue) ?? firstNumber(in: snapshot.value) ?? 0
    let percent = min(max(Int((rawValue / 21 * 100).rounded()), 0), 100)
    return HealthMetricSnapshot(
      id: snapshot.id,
      route: snapshot.route,
      group: snapshot.group,
      title: snapshot.title,
      value: "\(percent)",
      unit: "%",
      status: snapshot.status,
      freshness: snapshot.freshness,
      provenance: snapshot.provenance,
      source: snapshot.source,
      systemImage: snapshot.systemImage,
      tint: snapshot.tint,
      trend: snapshot.trend
    )
  }

  private func datedHomeSnapshot(for route: HealthRoute, in snapshots: [HealthMetricSnapshot]) -> HealthMetricSnapshot {
    ScoreDateTimeline.datedSnapshot(from: homeSnapshot(for: route, in: snapshots), date: selectedDate)
  }

  private func openHealth(_ route: HealthRoute) {
    openHealthRoute(route)
    model.recordUIAction("health.deep_link.opened", detail: route.title)
  }

  private func openHealthMonitorSnapshot(_ snapshot: HealthMetricSnapshot) {
    if snapshot.id == "resting-hr" {
      selectedHealthMonitorTrend = snapshot
    } else {
      openHealth(.healthMonitor)
    }
  }

  private func openCoach(_ prompt: String) {
    router.openCoach(prompt: prompt)
    model.recordUIAction("coach.opened", detail: "Home daily score card")
  }
}

