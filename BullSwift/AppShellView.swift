import SwiftUI

struct AppShellView: View {
  // Intentionally does NOT observe BullAppModel: doing so re-ran this body on every
  // model tick (~1.3/s) and recreated the tab-content structs (new closure identity
  // -> SwiftUI `@self changed` cascade re-rendering the whole Home tree). Tab
  // selection is already logged via each tab's `page.opened` onAppear.
  @EnvironmentObject private var router: AppRouter
  @StateObject private var healthStore = HealthDataStore()
  @StateObject private var calibrationManager = CalibrationManager.shared
  @State private var homeHealthPath: [HealthRoute] = []
  @State private var homeSelectedDate = Date()

  var body: some View {
    TabView(selection: tabSelection) {
      ForEach(BullAppTab.allCases) { tab in
        tabNavigationStack(for: tab)
        .tabItem {
          Label(tab.title, systemImage: tab.systemImage)
        }
        .tag(tab)
      }
    }
    .environmentObject(calibrationManager)
  }

  private var tabSelection: Binding<BullAppTab> {
    Binding {
      router.selectedTab
    } set: { newTab in
      if newTab == router.selectedTab {
        router.reselect(newTab)
        return
      }
      router.selectedTab = newTab
    }
  }

  @ViewBuilder
  private func tabNavigationStack(for tab: BullAppTab) -> some View {
    if tab == .home {
      NavigationStack(path: $homeHealthPath) {
        tabContent(for: tab)
          .navigationDestination(for: HealthRoute.self) { route in
            if route.isUserFacing {
              HealthRouteDestinationView(route: route, store: healthStore, selectedDate: $homeSelectedDate)
            } else {
              Text("Open this tool from More → Developer.")
                .foregroundStyle(.secondary)
                .padding()
            }
          }
      }
    } else if tab == .health {
      NavigationStack(path: $router.healthPath) {
        tabContent(for: tab)
      }
    } else if tab == .more {
      NavigationStack(path: $router.morePath) {
        tabContent(for: tab)
      }
    } else {
      NavigationStack {
        tabContent(for: tab)
      }
    }
  }

  @ViewBuilder
  private func tabContent(for tab: BullAppTab) -> some View {
    switch tab {
    case .home:
      HomeDashboardView(
        healthStore: healthStore,
        selectedDate: $homeSelectedDate,
        openHealthRoute: openHomeHealthRoute
      )
    case .health:
      HealthView(store: healthStore)
    case .coach:
      CoachView(healthStore: healthStore)
    case .more:
      MoreView(healthStore: healthStore)
    }
  }

  private func openHomeHealthRoute(_ route: HealthRoute) {
    homeHealthPath = [route]
  }
}

enum BullAppTab: String, CaseIterable, Identifiable {
  case home
  case health
  case coach
  case more

  var id: String { rawValue }

  var title: String {
    switch self {
    case .home: "Home"
    case .health: "Health"
    case .coach: "Coach"
    case .more: "More"
    }
  }

  var systemImage: String {
    switch self {
    case .home: "house"
    case .health: "heart.text.square"
    case .coach: "sparkles"
    case .more: "ellipsis.circle"
    }
  }

}
