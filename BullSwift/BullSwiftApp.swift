import SwiftUI

@main
struct BullSwiftApp: App {
  @Environment(\.scenePhase) private var scenePhase
  @StateObject private var model = BullAppModel()
  @StateObject private var router = AppRouter()

  init() {
    BullTheme.configureAppearance()
  }

  var body: some Scene {
    WindowGroup {
      RootView()
        .environmentObject(model)
        .environmentObject(model.packetMonitor)
        .environmentObject(model.ble.messageStore)
        .environmentObject(router)
        .onOpenURL { url in
          if model.handleDebugCommandDeepLink(url) {
            router.selectedTab = .more
          } else {
            _ = router.handleDeepLink(url)
          }
        }
        .onChange(of: scenePhase) { _, phase in
          switch phase {
          case .active:
            model.handleAppLifecycleChange("active")
          case .inactive:
            model.handleAppLifecycleChange("inactive")
          case .background:
            model.handleAppLifecycleChange("background")
          @unknown default:
            model.handleAppLifecycleChange("unknown")
          }
        }
    }
  }
}
