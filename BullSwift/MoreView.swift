import Foundation
import CryptoKit
import SwiftUI
import UIKit

#if canImport(HealthKit)
import HealthKit
#endif

struct MoreView: View {
  @EnvironmentObject private var model: BullAppModel
  @EnvironmentObject private var router: AppRouter
  @ObservedObject private var healthStore: HealthDataStore
  @StateObject private var store: MoreDataStore
  @AppStorage(OnboardingStorage.firstName) private var profileFirstName = ""
  @AppStorage(OnboardingStorage.unitSystem) private var profileUnitSystemRaw = "imperial"
  @AppStorage(OnboardingStorage.heightMm) private var profileHeightMm = 0
  @AppStorage(OnboardingStorage.weightGrams) private var profileWeightGrams = 0
  @AppStorage("bull.notif.enabled") private var notificationsEnabled = true
  @State private var recalculateStatus = ""

  @MainActor
  init(healthStore: HealthDataStore) {
    self.healthStore = healthStore
    _store = StateObject(wrappedValue: MoreDataStore())
  }

  @MainActor
  init(healthStore: HealthDataStore, store: MoreDataStore) {
    self.healthStore = healthStore
    _store = StateObject(wrappedValue: store)
  }

  var body: some View {
    List {
      Section {
        NavigationLink(value: MoreRoute.profile) {
          MoreGreetingHeader(
            firstName: profileFirstName,
            profileSummary: profileSummary
          )
        }
        .accessibilityLabel("Update profile")
      }

      Section {
        NavigationLink {
          JournalView()
        } label: {
          Label("Journal", systemImage: "book.closed")
        }
        .accessibilityLabel("Journal and behavior insights")
      } header: {
        Text("Journal")
      } footer: {
        Text("Log daily behaviors and see what's helping or hurting your recovery and sleep.")
      }

      Section("Biometric Engine") {
        NavigationLink {
          RrHrConsistencyDebugView(healthStore: healthStore)
        } label: {
          Label("HRV Scale Check", systemImage: "checkmark.seal")
        }
        .accessibilityLabel("HRV scale consistency check")

        NavigationLink {
          DeviceBiometricsView(healthStore: healthStore)
        } label: {
          Label("Device Biometrics", systemImage: "drop.fill")
        }
        .accessibilityLabel("Device biometric streams")

        Button {
          recalculateStatus = "Clearing..."
          store.clearCachedSleepScores { status in
            recalculateStatus = status
          }
        } label: {
          Label("Recalculate Sleep Scores", systemImage: "arrow.clockwise")
        }
        if !recalculateStatus.isEmpty {
          Text(recalculateStatus)
            .font(.caption)
            .foregroundStyle(.secondary)
        }
      }

      Section("Device") {
        routeRows(MoreRoute.deviceRoutes)
      }

      Section("App") {
        routeRows(MoreRoute.appRoutes)
      }

      Section("Settings") {
        routeRows(MoreRoute.settingsRoutes)
      }

      Section {
        Toggle("Recovery, battery & sync alerts", isOn: $notificationsEnabled)
      } header: {
        Text("Notifications")
      } footer: {
        Text("Local alerts for a fresh morning recovery score, a low band battery, and a band that has stopped syncing.")
      }

      Section("Support") {
        routeRows(MoreRoute.supportRoutes)
      }

      Section("Developer") {
        routeRows(MoreRoute.developerRoutes)
      }
    }
    .listStyle(.insetGrouped)
    .bullListBackground()
    .navigationTitle("More")
    .navigationBarTitleDisplayMode(.inline)
    .toolbarBackground(.hidden, for: .navigationBar)
    .navigationDestination(for: MoreRoute.self) { route in
      destination(for: route)
    }
    .onAppear {
      model.recordUIAction("page.opened", detail: "More")
      store.refreshBridgeStatus(model: model)
      store.refreshRecentCaptureSessions()
    }
    .onChange(of: notificationsEnabled) { _, enabled in
      BullNotificationScheduler.shared.setEnabled(enabled)
    }
  }

  private var routeStatus: MoreRouteStatus {
    store.routeStatus(ble: model.ble, model: model)
  }

  @ViewBuilder
  private func routeRows(_ routes: [MoreRoute]) -> some View {
    ForEach(routes) { route in
      NavigationLink(value: route) {
        MoreRouteRow(route: route, status: routeStatus[keyPath: route.statusKeyPath])
      }
      .accessibilityLabel(route.title)
    }
  }

  @ViewBuilder
  private func destination(for route: MoreRoute) -> some View {
    switch route {
    case .device:
      DeviceView()
    case .profile:
      MoreProfileView()
    case .connectionLab:
      ConnectionView()
    case .capture:
      MoreCaptureView(store: store)
    case .localStore:
      MoreLocalStoreView(store: store)
    case .healthSync:
      MoreHealthSyncView(store: store)
    case .rawExport:
      MoreRawExportView(store: store)
    case .algorithms:
      MoreAlgorithmsView(store: store, healthStore: healthStore) {
        router.openHealth(.algorithms)
      }
    case .debug:
      MoreDebugView(store: store)
    case .privacy:
      MorePrivacyView(store: store)
    case .support:
      MoreSupportView(store: store)
    case .about:
      MoreAboutView(store: store)
    case .developer:
      MoreDeveloperView(routes: MoreRoute.developerToolRoutes, routeStatus: routeStatus)
    }
  }

  private var profileSummary: String {
    let height = MoreProfileFormatting.heightText(millimeters: profileHeightMm, unitSystemRaw: profileUnitSystemRaw)
    let weight = MoreProfileFormatting.weightText(grams: profileWeightGrams, unitSystemRaw: profileUnitSystemRaw)
    let parts = [height, weight].filter { !$0.isEmpty }
    return parts.isEmpty ? "Update profile" : parts.joined(separator: " | ")
  }
}
