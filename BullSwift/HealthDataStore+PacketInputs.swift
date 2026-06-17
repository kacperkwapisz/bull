import Foundation

extension HealthDataStore {
  /// Stable identifier for the locally connected device's surfaced biometric
  /// streams. Every physiological sample is derived from the connected device's
  /// own live sensor data over Bluetooth; this id keys the typed sample tables
  /// (gravity, gravity2, SpO2, skin temp, resp) so ingest and read-back agree.
  /// Single connected device today; revisit if multi-device support is added.
  nonisolated static let localBiometricDeviceID = "bull.device.local.v1"

  /// `yyyy-MM-dd` calendar-day key for `date` in the given calendar's timezone.
  static func metricDateKey(for date: Date, calendar inputCalendar: Calendar = .current) -> String {
    var calendar = inputCalendar
    calendar.locale = Locale(identifier: "en_US_POSIX")
    let start = calendar.startOfDay(for: date)
    let formatter = DateFormatter()
    formatter.calendar = calendar
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.timeZone = calendar.timeZone
    formatter.dateFormat = "yyyy-MM-dd"
    return formatter.string(from: start)
  }

  /// Normalize the onboarding gender value to the sex tokens the energy model
  /// accepts, or nil when unset/unknown (so compute degrades honestly).
  nonisolated static func normalizedProfileSex(_ rawValue: String) -> String? {
    switch rawValue {
    case "female", "male":
      return rawValue
    default:
      return nil
    }
  }
}
