import Foundation
import UserNotifications

/// Local notification scheduler for the daily-driver moments that matter even
/// when the app is closed: a fresh morning recovery score, a low band battery,
/// and a band that has stopped sending data.
///
/// These are fired or scheduled locally through `UNUserNotificationCenter`,
/// triggered from foreground use or background Bluetooth wake-ups. De-dup state
/// lives in `UserDefaults` so each alert fires at most once per natural window
/// (one recovery alert per morning, one low-battery alert per drain cycle, one
/// staleness alert per quiet period).
@MainActor
final class BullNotificationScheduler {
  static let shared = BullNotificationScheduler()

  // MARK: Tunables

  /// Notify when the band battery is at or below this percentage.
  static let lowBatteryThreshold = 15
  /// Only re-arm the low-battery alert after the battery climbs back above this.
  static let lowBatteryRearmThreshold = 25
  /// Fire a staleness alert when no band data has arrived for this long. Pushed
  /// forward on every data receipt, so it only fires after a genuine quiet gap
  /// (band off-wrist/dead/out of range), not on a brief disconnect.
  static let syncStalenessThreshold: TimeInterval = 12 * 60 * 60
  /// Re-arm the staleness timer at most this often, so high-rate data receipts
  /// don't rewrite the pending request constantly.
  static let stalenessRescheduleThrottle: TimeInterval = 15 * 60
  /// In a background task, only trust a persisted battery reading this fresh.
  static let batteryFreshnessForBackground: TimeInterval = 90 * 60
  /// Recovery is a morning moment; suppress the alert outside this local-hour
  /// window so an overnight sync can't ping at 3 a.m.
  static let recoveryMorningStartHour = 5
  static let recoveryMorningEndHour = 11

  // MARK: Identifiers / keys

  private enum RequestID {
    static let recovery = "bull.notif.recovery"
    static let staleness = "bull.notif.staleness"
    static let lowBattery = "bull.notif.low-battery"
  }

  private enum Key {
    static let enabled = "bull.notif.enabled"
    static let recoveryDay = "bull.notif.recovery.last-day"
    static let lowBatteryArmed = "bull.notif.low-battery.armed"
    static let batteryLastPercent = "bull.notif.battery.last-percent"
    static let batteryLastAt = "bull.notif.battery.last-at"
  }

  private let center = UNUserNotificationCenter.current()
  private let defaults = UserDefaults.standard
  private var lastStalenessRescheduleAt = Date.distantPast

  private init() {}

  // MARK: Master toggle

  /// User-facing master switch; defaults on until explicitly disabled.
  var isEnabled: Bool {
    defaults.object(forKey: Key.enabled) == nil ? true : defaults.bool(forKey: Key.enabled)
  }

  func setEnabled(_ enabled: Bool) {
    defaults.set(enabled, forKey: Key.enabled)
    if enabled {
      ensureAuthorization()
    } else {
      center.removeAllPendingNotificationRequests()
      center.removeAllDeliveredNotifications()
    }
  }

  // MARK: Authorization

  /// Request notification permission only if it has never been asked; never
  /// re-prompts a user who already decided.
  func ensureAuthorization() {
    center.getNotificationSettings { [weak self] settings in
      guard settings.authorizationStatus == .notDetermined else { return }
      self?.center.requestAuthorization(options: [.alert, .badge, .sound]) { _, _ in }
    }
  }

  // MARK: Recovery ready

  /// Fire a morning recovery alert at most once per day, only inside the morning
  /// window. `score` is the 0–100 recovery percentage, or nil when unavailable.
  func evaluateRecovery(score: Int?, now: Date = Date(), calendar: Calendar = .current) {
    guard isEnabled, let score, (0...100).contains(score) else { return }
    let dayKey = Self.dayKey(now, calendar)
    guard defaults.string(forKey: Key.recoveryDay) != dayKey else { return }
    let hour = calendar.component(.hour, from: now)
    guard hour >= Self.recoveryMorningStartHour, hour < Self.recoveryMorningEndHour else { return }
    defaults.set(dayKey, forKey: Key.recoveryDay)
    post(
      id: RequestID.recovery,
      title: "Recovery \(score)%",
      body: Self.recoveryBody(score: score)
    )
  }

  // MARK: Sync staleness

  /// Call whenever fresh band data arrives. Pushes the staleness alert out to
  /// `now + syncStalenessThreshold`; if data stops, the last-armed request fires
  /// after the quiet gap. Throttled so frequent receipts don't churn the
  /// pending request. The pending time-interval request is delivered even if the
  /// app is later suspended.
  func noteDataActivity(now: Date = Date()) {
    guard isEnabled else { return }
    guard now.timeIntervalSince(lastStalenessRescheduleAt) >= Self.stalenessRescheduleThrottle else { return }
    lastStalenessRescheduleAt = now
    center.getNotificationSettings { [weak self] settings in
      guard let self else { return }
      guard settings.authorizationStatus == .authorized
        || settings.authorizationStatus == .provisional
        || settings.authorizationStatus == .ephemeral else { return }
      self.center.removePendingNotificationRequests(withIdentifiers: [RequestID.staleness])
      let content = UNMutableNotificationContent()
      content.title = "Band hasn't synced"
      content.body = "Bull hasn't received data from your band in a while. Open Bull or check the connection."
      content.sound = .default
      let trigger = UNTimeIntervalNotificationTrigger(
        timeInterval: Self.syncStalenessThreshold,
        repeats: false
      )
      self.center.add(
        UNNotificationRequest(identifier: RequestID.staleness, content: content, trigger: trigger)
      )
    }
  }

  // MARK: Battery

  /// Fire one low-battery alert per drain cycle; re-arm only after the battery
  /// recovers above the re-arm threshold so it does not nag while hovering low.
  func batteryChanged(percent: Int?, now: Date = Date()) {
    guard isEnabled, let percent else { return }
    // Persist the latest reading so a background task can re-evaluate even when
    // the live BLE observer isn't running.
    defaults.set(percent, forKey: Key.batteryLastPercent)
    defaults.set(now, forKey: Key.batteryLastAt)
    if percent >= Self.lowBatteryRearmThreshold {
      defaults.set(true, forKey: Key.lowBatteryArmed)
      return
    }
    guard percent <= Self.lowBatteryThreshold, isLowBatteryArmed else { return }
    defaults.set(false, forKey: Key.lowBatteryArmed)
    post(
      id: RequestID.lowBattery,
      title: "Band battery \(percent)%",
      body: "Charge your band soon to avoid a gap in tonight's capture."
    )
  }

  private var isLowBatteryArmed: Bool {
    defaults.object(forKey: Key.lowBatteryArmed) == nil ? true : defaults.bool(forKey: Key.lowBatteryArmed)
  }

  /// Re-run the low-battery check from the last persisted reading, used by the
  /// background processing task. Ignores readings older than the freshness
  /// window so a stale value can't fire a wrong alert.
  func reevaluateBatteryInBackground(now: Date = Date()) {
    guard isEnabled,
      let percent = defaults.object(forKey: Key.batteryLastPercent) as? Int,
      let at = defaults.object(forKey: Key.batteryLastAt) as? Date,
      now.timeIntervalSince(at) <= Self.batteryFreshnessForBackground else { return }
    batteryChanged(percent: percent, now: now)
  }

  // MARK: Helpers

  private func post(id: String, title: String, body: String) {
    center.getNotificationSettings { [weak self] settings in
      guard let self else { return }
      guard settings.authorizationStatus == .authorized
        || settings.authorizationStatus == .provisional
        || settings.authorizationStatus == .ephemeral else { return }
      let content = UNMutableNotificationContent()
      content.title = title
      content.body = body
      content.sound = .default
      self.center.add(
        UNNotificationRequest(identifier: id, content: content, trigger: nil)
      )
    }
  }

  private static func dayKey(_ date: Date, _ calendar: Calendar) -> String {
    let c = calendar.dateComponents([.year, .month, .day], from: date)
    return String(format: "%04d-%02d-%02d", c.year ?? 0, c.month ?? 0, c.day ?? 0)
  }

  private static func recoveryBody(score: Int) -> String {
    switch score {
    case 67...100:
      return "You're primed — your body is ready to take on strain today."
    case 34..<67:
      return "Moderate recovery — train with awareness and don't overreach."
    default:
      return "Low recovery — prioritize rest and keep today's strain gentle."
    }
  }
}
