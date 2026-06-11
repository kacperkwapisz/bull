import Darwin
import Foundation
import SwiftUI
import UIKit

struct HealthView: View {
  @EnvironmentObject private var model: BullAppModel
  @EnvironmentObject private var calibration: CalibrationManager
  @ObservedObject var store: HealthDataStore
  @Environment(\.colorScheme) private var colorScheme
  @State private var cachedLandingSnapshots: [HealthMetricSnapshot] = []
  @State private var cachedVitalSnapshots: [HealthMetricSnapshot] = []
  @State private var lastLiveRefresh = Date.distantPast

  var body: some View {
    let _ = Self.bullPrintChangesIfEnabled()
    ScrollView {
      LazyVStack(alignment: .leading, spacing: 22) {
        if calibration.uiSnapshot.shouldShowBanner {
          CalibrationBanner(
            snapshot: calibration.uiSnapshot,
            palette: SleepV2Palette(colorScheme: colorScheme)
          )
        }

        HealthActivityOverviewSection(
          steps: store.whoopStepsDisplayText(),
          activeEnergy: store.whoopActiveCaloriesDisplayText(),
          stepsFreshness: store.whoopStepsStatusText(),
          stepsSource: store.whoopStepsSource(),
          activeEnergyFreshness: store.whoopActiveCaloriesStatusText(),
          activeEnergySource: store.whoopActiveCaloriesSource(),
          heartRateValue: liveHeartRateValue,
          heartRateStatus: liveHeartRateStatus,
          heartRateSource: liveHeartRateSource
        )

        HealthVitalsPreviewSection(snapshots: cachedVitalSnapshots)

        HealthRouteShortcutSection(
          title: "Explore Health",
          snapshots: snapshots(for: [.sleep, .recovery, .strain, .stress, .cardioLoad, .energyBank])
        )
      }
      .padding(.horizontal, 16)
      .padding(.vertical, 18)
    }
    .bullScreenBackground()
    .navigationTitle("Health")
    .navigationBarTitleDisplayMode(.inline)
    .toolbarBackground(.hidden, for: .navigationBar)
    .navigationDestination(for: HealthRoute.self) { route in
      if route.isUserFacing {
        HealthRouteContentView(route: route, store: store)
      } else {
        Text("This screen is available from More → Developer.")
          .foregroundStyle(.secondary)
          .padding()
      }
    }
    .refreshable {
      await refreshUserData()
    }
    .onAppear {
      model.recordUIAction("page.opened", detail: "Health")
      calibration.ensureStarted(connectedAt: model.ble.connectedAt)
      store.loadBridgeCatalogsIfNeeded()
      store.refreshHeartRateTimeline()
      refreshSnapshots()
    }
    .onChange(of: model.ble.liveHeartRateBPM) { _, _ in
      guard Date().timeIntervalSince(lastLiveRefresh) > 5 else { return }
      refreshSnapshots()
    }
    .onChange(of: store.catalogStatus) { _, _ in
      refreshSnapshots()
    }
  }

  private func refreshSnapshots() {
    calibration.refreshUISnapshot(store: store, isBandConnected: model.ble.isConnectedForUserBaseline)
    lastLiveRefresh = Date()
    cachedLandingSnapshots = store.landingSnapshots(
      liveHeartRateBPM: model.ble.liveHeartRateBPM,
      liveHeartRateSource: model.ble.liveHeartRateSource,
      liveHeartRateUpdatedAt: model.ble.liveHeartRateUpdatedAt
    )
    cachedVitalSnapshots = Array(store.healthMonitorSnapshots().prefix(4))
  }

  private var liveHeartRateValue: String {
    guard let bpm = model.ble.liveHeartRateBPM else {
      return "--"
    }
    return "\(bpm) bpm"
  }

  private var liveHeartRateStatus: String {
    guard model.ble.liveHeartRateBPM != nil else {
      return humanizedHomeStatus(store.heartRateTimelineStatus)
    }
    return HealthDataStore.relativeText(for: model.ble.liveHeartRateUpdatedAt) ?? "Live"
  }

  private var liveHeartRateSource: HealthDataSource {
    model.ble.liveHeartRateBPM == nil
      ? .unavailable("BLE heart-rate stream waiting")
      : .live(model.ble.liveHeartRateSource)
  }

  private func snapshots(for routes: [HealthRoute]) -> [HealthMetricSnapshot] {
    routes.compactMap { route in
      cachedLandingSnapshots.first { $0.route == route } ?? store.snapshot(for: route)
    }
  }

  @MainActor
  private func refreshUserData() async {
    store.loadBridgeCatalogsIfNeeded()
    store.refreshHeartRateTimeline()
    refreshSnapshots()
  }
}
