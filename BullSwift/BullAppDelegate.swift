import BackgroundTasks
import UIKit
import UserNotifications

/// App delegate adaptor for the things SwiftUI's `App` lifecycle can't host
/// directly: APNs remote-notification registration, foreground presentation of
/// local/remote alerts, and registering the background battery-check task.
final class BullAppDelegate: NSObject, UIApplicationDelegate, UNUserNotificationCenterDelegate {
  func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
  ) -> Bool {
    UNUserNotificationCenter.current().delegate = self
    // Must be registered before launch finishes.
    BullBackgroundTasks.registerHandlers()
    BullBackgroundTasks.schedule()
    application.registerForRemoteNotifications()
    return true
  }

  func application(
    _ application: UIApplication,
    didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data
  ) {
    BullPushTokenUploader.register(deviceToken: deviceToken)
  }

  func application(
    _ application: UIApplication,
    didFailToRegisterForRemoteNotificationsWithError error: Error
  ) {
    // Best-effort; push simply stays inactive until a later successful register.
  }

  // Show recovery (and other) alerts as a banner even when the app is foreground.
  func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    willPresent notification: UNNotification,
    withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
  ) {
    completionHandler([.banner, .sound])
  }
}

/// Background processing task that re-checks the band battery while the app is
/// suspended, complementing the live BLE observer.
enum BullBackgroundTasks {
  static let batteryTaskID = "com.bull.swift.battery-check"
  static let interval: TimeInterval = 2 * 60 * 60

  static func registerHandlers() {
    BGTaskScheduler.shared.register(forTaskWithIdentifier: batteryTaskID, using: nil) { task in
      guard let processingTask = task as? BGProcessingTask else {
        task.setTaskCompleted(success: false)
        return
      }
      handleBatteryTask(processingTask)
    }
  }

  static func schedule() {
    let request = BGProcessingTaskRequest(identifier: batteryTaskID)
    request.requiresNetworkConnectivity = false
    request.requiresExternalPower = false
    request.earliestBeginDate = Date(timeIntervalSinceNow: interval)
    try? BGTaskScheduler.shared.submit(request)
  }

  private static func handleBatteryTask(_ task: BGProcessingTask) {
    schedule() // chain the next occurrence
    task.expirationHandler = {
      task.setTaskCompleted(success: false)
    }
    Task { @MainActor in
      BullNotificationScheduler.shared.reevaluateBatteryInBackground()
      task.setTaskCompleted(success: true)
    }
  }
}
