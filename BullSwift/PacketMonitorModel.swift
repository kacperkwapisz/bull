import Foundation

@MainActor
final class PacketMonitorModel: ObservableObject {
  @Published var lastParsedFrameSummary = "No notification frames parsed"
  @Published var movementPacketStatus = "No movement packets"
  @Published var latestWhoopEventStatus = "No WHOOP events"
  @Published var latestSkinTemperatureCandidateStatus = "No skin temperature events"
  @Published var latestWhoopDataPacketStatus = "No WHOOP data packets"
  @Published var latestHistoryTemperatureCandidateStatus = "No history temperature packets"
  @Published var latestRespiratoryRateCandidateStatus = "No respiratory rate candidates"
  @Published var latestPulseInformationPacketStatus = "No pulse information packets"
  @Published var latestOpticalPacketStatus = "No optical packets"
  @Published var latestRawResearchPacketStatus = "No raw/research packets"
  @Published var latestRealtimeStatusPacketStatus = "No realtime status packets"
  @Published var performancePipelineStatus = "No pipeline samples"
  @Published var liveDeviceDataSummary = "No live WHOOP data points"
  @Published var recentDeviceSignalPoints: [DeviceSignalPoint] = []

  func apply(
    _ snapshot: PacketUIStateSnapshot,
    maxRecentDeviceSignalPoints: Int,
    publishInterval: TimeInterval
  ) {
    // Equality guards: this object is injected app-wide as an @EnvironmentObject
    // and publishes every ~0.2s. Only assign when a value actually changed so we
    // don't fire objectWillChange (and re-render consumers) on repeated strings.
    if let status = snapshot.lastParsedFrameSummary, status != lastParsedFrameSummary {
      lastParsedFrameSummary = status
    }
    if let status = snapshot.movementPacketStatus, status != movementPacketStatus {
      movementPacketStatus = status
    }
    if let status = snapshot.whoopEventStatus, status != latestWhoopEventStatus {
      latestWhoopEventStatus = status
    }
    if let status = snapshot.skinTemperatureCandidateStatus, status != latestSkinTemperatureCandidateStatus {
      latestSkinTemperatureCandidateStatus = status
    }
    if let status = snapshot.whoopDataPacketStatus, status != latestWhoopDataPacketStatus {
      latestWhoopDataPacketStatus = status
    }
    if let status = snapshot.historyTemperatureCandidateStatus, status != latestHistoryTemperatureCandidateStatus {
      latestHistoryTemperatureCandidateStatus = status
    }
    if let status = snapshot.respiratoryRateCandidateStatus, status != latestRespiratoryRateCandidateStatus {
      latestRespiratoryRateCandidateStatus = status
    }
    if let status = snapshot.pulseInformationPacketStatus, status != latestPulseInformationPacketStatus {
      latestPulseInformationPacketStatus = status
    }
    if let status = snapshot.opticalPacketStatus, status != latestOpticalPacketStatus {
      latestOpticalPacketStatus = status
    }
    if let status = snapshot.rawResearchPacketStatus, status != latestRawResearchPacketStatus {
      latestRawResearchPacketStatus = status
    }
    if let status = snapshot.realtimeStatusPacketStatus, status != latestRealtimeStatusPacketStatus {
      latestRealtimeStatusPacketStatus = status
    }
    if let status = snapshot.performancePipelineStatus, status != performancePipelineStatus {
      performancePipelineStatus = status
    }
    if !snapshot.deviceSignalPoints.isEmpty {
      for point in snapshot.deviceSignalPoints {
        recentDeviceSignalPoints.insert(point, at: 0)
      }
      if recentDeviceSignalPoints.count > maxRecentDeviceSignalPoints {
        recentDeviceSignalPoints.removeLast(recentDeviceSignalPoints.count - maxRecentDeviceSignalPoints)
      }
    }
    if let summary = snapshot.liveDeviceDataSummary, summary != liveDeviceDataSummary {
      liveDeviceDataSummary = summary
    }
    if snapshot.coalescedStatusUpdateCount > 0 {
      let summary = snapshot.coalescedStatusUpdateSummary ?? "unknown"
      performancePipelineStatus = "ui coalesced \(snapshot.coalescedStatusUpdateCount) status update(s) before publish (\(summary); reason=publish_interval_\(publishInterval)s) | \(performancePipelineStatus)"
    }
    if snapshot.droppedDeviceSignalPointCount > 0 {
      performancePipelineStatus = "ui signal preview dropped \(snapshot.droppedDeviceSignalPointCount) stale point(s) | \(performancePipelineStatus)"
    }
  }
}
