import Foundation
import UIKit

struct MovementPacketSample {
  let capturedAt: Date
  let packetK: Int?
  let bodySummaryKind: String
  let heartRateBPM: Int?
  let axisCount: Int
  let parsedSampleCount: Int
  let rawPeakRange: Double
  let rawPeakAbs: Double
  let accelerometerPeakRange: Double
  let gyroscopePeakRange: Double
  let accelerometerVectorRange: Double
  let motionIntensity: Double
  let deviceTimestampSeconds: Int?
  let deviceTimestampSubseconds: Int?

  var isMoving: Bool {
    parsedSampleCount > 0 && motionIntensity >= 0.10
  }

  static func fromParsedFrame(
    _ parsed: [String: Any],
    capturedAt: Date,
    fallbackHeartRate: Int?
  ) -> MovementPacketSample? {
    guard
      let payload = parsed["parsed_payload"] as? [String: Any],
      payload["kind"] as? String == "data_packet",
      let body = payload["body_summary"] as? [String: Any],
      let bodyKind = body["kind"] as? String,
      bodyKind == "raw_motion_k10",
      let axes = body["axes"] as? [[String: Any]]
    else {
      return nil
    }

    var axisCount = 0
    var parsedSampleCount = 0
    var rawPeakRange = 0.0
    var rawPeakAbs = 0.0
    var accelerometerPeakRange = 0.0
    var gyroscopePeakRange = 0.0
    var accelerometerRangeSquaredTotal = 0.0

    for axis in axes {
      let parsedCount = intValue(axis["parsed_count"]) ?? 0
      guard parsedCount > 0 else {
        continue
      }
      axisCount += 1
      parsedSampleCount += parsedCount

      let axisMin = intValue(axis["min"])
      let axisMax = intValue(axis["max"])
      if let axisMin, let axisMax {
        let range = Double(axisMax - axisMin)
        rawPeakRange = max(rawPeakRange, range)
        rawPeakAbs = max(rawPeakAbs, Double(max(abs(axisMin), abs(axisMax))))
        if let name = axis["name"] as? String {
          if name.hasPrefix("accelerometer_") {
            accelerometerPeakRange = max(accelerometerPeakRange, range)
            accelerometerRangeSquaredTotal += range * range
          } else if name.hasPrefix("gyroscope_") {
            gyroscopePeakRange = max(gyroscopePeakRange, range)
          }
        }
      } else if let preview = axis["preview"] as? [Any] {
        for value in preview.compactMap({ intValue($0) }) {
          rawPeakAbs = max(rawPeakAbs, Double(abs(value)))
        }
      }
    }

    guard parsedSampleCount > 0 else {
      return nil
    }

    let bodyHeartRate = intValue(body["heart_rate"])
    let accelerometerVectorRange = sqrt(accelerometerRangeSquaredTotal)
    let accelerometerIntensity = accelerometerVectorRange / 8192.0
    let rawIntensity = rawPeakRange / 32767.0
    return MovementPacketSample(
      capturedAt: capturedAt,
      packetK: intValue(payload["packet_k"]),
      bodySummaryKind: bodyKind,
      heartRateBPM: bodyHeartRate ?? fallbackHeartRate,
      axisCount: axisCount,
      parsedSampleCount: parsedSampleCount,
      rawPeakRange: rawPeakRange,
      rawPeakAbs: rawPeakAbs,
      accelerometerPeakRange: accelerometerPeakRange,
      gyroscopePeakRange: gyroscopePeakRange,
      accelerometerVectorRange: accelerometerVectorRange,
      motionIntensity: min(1, max(rawIntensity, accelerometerIntensity)),
      deviceTimestampSeconds: intValue(payload["timestamp_seconds"]),
      deviceTimestampSubseconds: intValue(payload["timestamp_subseconds"])
    )
  }

  static func fromCompactSummary(
    _ compact: NotificationFrameCompactSummary,
    capturedAt: Date,
    fallbackHeartRate: Int?
  ) -> MovementPacketSample? {
    guard compact.payloadKind == "data_packet",
          compact.bodyKind == "raw_motion_k10",
          let movement = compact.movement else {
      return nil
    }

    return MovementPacketSample(
      capturedAt: capturedAt,
      packetK: compact.packetK,
      bodySummaryKind: compact.bodyKind ?? "raw_motion_k10",
      heartRateBPM: compact.heartRateBPM ?? fallbackHeartRate,
      axisCount: movement.axisCount,
      parsedSampleCount: movement.parsedSampleCount,
      rawPeakRange: movement.rawPeakRange,
      rawPeakAbs: movement.rawPeakAbs,
      accelerometerPeakRange: movement.accelerometerPeakRange,
      gyroscopePeakRange: movement.gyroscopePeakRange,
      accelerometerVectorRange: movement.accelerometerVectorRange,
      motionIntensity: movement.motionIntensity,
      deviceTimestampSeconds: nil,
      deviceTimestampSubseconds: nil
    )
  }

  func logSummary(packetCount: Int) -> String {
    let hrText = heartRateBPM.map { "\($0)bpm" } ?? "hr=?"
    let packetText = packetK.map { "k=\($0)" } ?? "k=?"
    let deviceTime = deviceTimestampSeconds.map { "device_ts=\($0).\(deviceTimestampSubseconds ?? 0)" } ?? "device_ts=?"
    return "#\(packetCount) \(bodySummaryKind) \(packetText) \(isMoving ? "moving" : "quiet") axes=\(axisCount) samples=\(parsedSampleCount) acc_vec=\(Int(accelerometerVectorRange.rounded())) acc_peak=\(Int(accelerometerPeakRange.rounded())) gyro_peak=\(Int(gyroscopePeakRange.rounded())) peak=\(Int(rawPeakAbs.rounded())) intensity=\(String(format: "%.3f", motionIntensity)) \(hrText) \(deviceTime)"
  }

  private static func intValue(_ value: Any?) -> Int? {
    if let int = value as? Int {
      return int
    }
    if let number = value as? NSNumber {
      return number.intValue
    }
    if let string = value as? String {
      return Int(string)
    }
    return nil
  }
}

struct MovementPacketValidation {
  var startedAt: Date?
  var timeout: TimeInterval = 45
  var packetCount = 0
  var movingPacketCount = 0
  var peakIntensity = 0.0
  var latestKind = "none"
  var latestHeartRate: Int?
  var firstPacketAt: Date?
  var lastPacketAt: Date?

  init() {}

  init(startedAt: Date, timeout: TimeInterval) {
    self.startedAt = startedAt
    self.timeout = timeout
  }

  mutating func ingest(_ sample: MovementPacketSample) {
    packetCount += 1
    if sample.isMoving {
      movingPacketCount += 1
    }
    peakIntensity = max(peakIntensity, sample.motionIntensity)
    latestKind = sample.bodySummaryKind
    latestHeartRate = sample.heartRateBPM
    if firstPacketAt == nil {
      firstPacketAt = sample.capturedAt
    }
    lastPacketAt = sample.capturedAt
  }

  var statusSummary: String {
    guard packetCount > 0 else {
      return "Listening for real WHOOP movement packets"
    }

    let motionText = movingPacketCount > 0 ? "\(movingPacketCount) moving" : "quiet"
    return "\(packetCount) movement packets, \(motionText), peak \(peakPercent)%"
  }

  var timeoutSummary: String {
    let seconds = Int(timeout.rounded())
    if packetCount == 0 {
      return "Failed: no K10 movement packets arrived in \(seconds)s"
    }
    if movingPacketCount == 0 {
      return "Failed: received \(packetCount) movement packets but all were quiet; move strap and run again"
    }
    return "Failed: timed out after \(packetCount) packets, \(movingPacketCount) moving"
  }

  var logSummary: String {
    let hrText = latestHeartRate.map { "hr=\($0)" } ?? "hr=?"
    let elapsedText = startedAt.map { "elapsed=\(String(format: "%.1f", Date().timeIntervalSince($0)))s" } ?? "elapsed=?"
    return "packets=\(packetCount) moving=\(movingPacketCount) peak=\(String(format: "%.3f", peakIntensity)) kind=\(latestKind) \(hrText) \(elapsedText)"
  }

  private var peakPercent: Int {
    Int((peakIntensity * 100).rounded())
  }
}

struct SkippedNotificationDiagnostic {
  let message: String
  let rollup: String?
}

final class SkippedNotificationDiagnostics {
  private let lock = NSLock()
  private var totalCount = 0
  private var countsByCharacteristic: [String: Int] = [:]
  private var countsByReason: [String: Int] = [:]
  private let payloadHexByteLimit = 256

  func record(_ event: BullNotificationEvent) -> SkippedNotificationDiagnostic {
    let reason = Self.frameSkipReason(value: event.value, deviceType: event.rustDeviceType)
    lock.lock()
    totalCount += 1
    countsByCharacteristic[event.characteristicUUID, default: 0] += 1
    countsByReason[reason.key, default: 0] += 1
    let count = totalCount
    let characteristicCounts = countsByCharacteristic
    let reasonCounts = countsByReason
    lock.unlock()

    let standardHeartRateText = event.characteristicUUID.uppercased() == "2A37" ? " standard_hr=true" : ""
    let message = "#\(count) char=\(event.characteristicUUID) service=\(event.serviceUUID) type=\(event.rustDeviceType) bytes=\(event.value.count) reason=\(reason.detail)\(standardHeartRateText) hex=\(Self.hexForLog(event.value, maxBytes: payloadHexByteLimit))"

    let rollup: String?
    if count == 10 || count.isMultiple(of: 50) {
      rollup = "total=\(count) chars=[\(Self.topSummary(characteristicCounts))] reasons=[\(Self.topSummary(reasonCounts))]"
    } else {
      rollup = nil
    }

    return SkippedNotificationDiagnostic(message: message, rollup: rollup)
  }

  private static func frameSkipReason(value: Data, deviceType: String) -> (key: String, detail: String) {
    var bytes = Array(value)
    var offset = 0
    let headerLength = deviceType == "GEN4" ? 4 : 8

    guard !bytes.isEmpty else {
      return ("empty", "empty")
    }

    while let startIndex = bytes.firstIndex(of: 0xaa) {
      if startIndex > 0 {
        offset += startIndex
        bytes.removeFirst(startIndex)
      }

      guard bytes.count >= headerLength else {
        return (
          "incomplete_header",
          "incomplete_header aa_offset=\(offset) remaining=\(bytes.count) header=\(headerLength)"
        )
      }

      let declaredLength: Int
      if deviceType == "GEN4" {
        declaredLength = Int(bytes[1]) | Int(bytes[2]) << 8
      } else {
        declaredLength = Int(bytes[2]) | Int(bytes[3]) << 8
      }

      guard declaredLength >= 4 else {
        bytes.removeFirst()
        offset += 1
        continue
      }

      let expectedLength = declaredLength + headerLength
      guard bytes.count >= expectedLength else {
        return (
          "incomplete_frame",
          "incomplete_frame aa_offset=\(offset) declared=\(declaredLength) expected=\(expectedLength) got=\(bytes.count)"
        )
      }

      return ("complete_frame_unexpected", "complete_frame_unexpected")
    }

    return ("no_aa", "no_aa first=\(String(format: "%02x", bytes[0]))")
  }

  private static func hexForLog(_ data: Data, maxBytes: Int) -> String {
    guard data.count > maxBytes else {
      return data.hexString
    }
    let prefix = Data(data.prefix(maxBytes)).hexString
    return "\(prefix)...(+\(data.count - maxBytes) bytes)"
  }

  private static func topSummary(_ counts: [String: Int]) -> String {
    counts
      .sorted { lhs, rhs in
        if lhs.value == rhs.value {
          return lhs.key < rhs.key
        }
        return lhs.value > rhs.value
      }
      .prefix(5)
      .map { "\($0.key):\($0.value)" }
      .joined(separator: ",")
  }
}
