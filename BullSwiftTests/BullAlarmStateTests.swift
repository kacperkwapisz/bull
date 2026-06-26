import XCTest
@testable import BullSwift

final class BullAlarmStateTests: XCTestCase {
  func test_persistedAlarmSnapshot_roundTripsThroughDefaults() throws {
    let defaults = UserDefaults(suiteName: "BullAlarmStateTests")!
    defaults.removePersistentDomain(forName: "BullAlarmStateTests")
    defer { defaults.removePersistentDomain(forName: "BullAlarmStateTests") }

    let snapshot = BullAlarmStateSnapshot(
      alarmID: 1,
      scheduledAt: Date(timeIntervalSince1970: 1_700_000_000),
      isEnabled: true,
      lastConfirmedSource: "strap-event"
    )

    BullAlarmStateStore(defaults: defaults).save(snapshot)

    let loaded = BullAlarmStateStore(defaults: defaults).load()
    XCTAssertEqual(loaded, snapshot)
  }
}
