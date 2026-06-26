import XCTest
@testable import BullSwift

final class BullBatteryPackParsingTests: XCTestCase {
  func test_commandResponseDoesNotTreatRevisionAsPercent() throws {
    var body = Array(repeating: UInt8(0), count: 28)
    body[0] = 1
    body[8..<14] = Array("Puffin".utf8)
    body[26] = 12
    body[27] = 1

    let info = try XCTUnwrap(BullBLEClient.parseBatteryPackCommandResponseBody(body))

    XCTAssertNil(info.percent)
    XCTAssertEqual(info.type, .puffin)
    XCTAssertEqual(info.colorway, "Full Black")
    XCTAssertEqual(info.deviceName, "Puffin")
  }

  func test_eventBodyParsesScaledPercent() throws {
    var body = Array(repeating: UInt8(0), count: 27)
    body[7..<13] = Array("Puffin".utf8)
    body[23] = 0x8A
    body[24] = 0x02
    body[25] = 1
    body[26] = 12

    let info = try XCTUnwrap(BullBLEClient.parseBatteryPackEventBody(body))

    XCTAssertEqual(info.percent, 65)
    XCTAssertEqual(info.type, .puffin)
    XCTAssertEqual(info.colorway, "Full Black")
    XCTAssertEqual(info.deviceName, "Puffin")
  }

  func test_metadataOnlyUpdatePreservesKnownPercent() throws {
    let ble = BullBLEClient(startCentral: false)
    ble.applyBatteryPackInfo(
      BatteryPackInfo(percent: 65, type: .puffin, colorway: nil, deviceName: nil),
      source: "test",
      capturedAt: Date()
    )

    ble.applyBatteryPackInfo(
      BatteryPackInfo(percent: nil, type: .penguin, colorway: "Full Black", deviceName: nil),
      source: "test",
      capturedAt: Date()
    )

    XCTAssertEqual(ble.batteryPackPercent, 65)
    XCTAssertEqual(ble.batteryPackType, .penguin)
    XCTAssertEqual(ble.batteryPackColorway, "Full Black")
  }
}
