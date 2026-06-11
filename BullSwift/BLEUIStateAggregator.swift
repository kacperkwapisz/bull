import Foundation

struct BLELiveHeartRateSnapshot {
  let bpm: Int?
  let source: String
  let updatedAt: Date?
  let syncAt: Date?
}

struct BLERestingHeartRateSnapshot {
  let bpm: Double?
  let sampleCount: Int
  let source: String
  let updatedAt: Date?
}

struct BLEHRVSnapshot {
  let rmssd: Double?
  let rrIntervalCount: Int
  let sampleCount: Int
  let source: String
  let updatedAt: Date?
}

struct BLEUIStateSnapshot {
  let liveHeartRate: BLELiveHeartRateSnapshot?
  let restingHeartRate: BLERestingHeartRateSnapshot?
  let hrv: BLEHRVSnapshot?
  let lastSyncAt: Date?
  let coalescedUpdateCount: Int
}

final class BLEUIStateAggregator {
  var onSnapshot: ((BLEUIStateSnapshot) -> Void)?

  private let queue = DispatchQueue(label: "com.bull.swift.ble-ui-state", qos: .utility)
  private let publishInterval: TimeInterval
  private var publishScheduled = false
  private var lastPublishedAt = Date.distantPast

  private var pendingLiveHeartRate: BLELiveHeartRateSnapshot?
  private var pendingRestingHeartRate: BLERestingHeartRateSnapshot?
  private var pendingHRV: BLEHRVSnapshot?
  private var pendingLastSyncAt: Date?
  private var coalescedUpdateCount = 0

  init(publishInterval: TimeInterval) {
    self.publishInterval = publishInterval
  }

  func publishLiveHeartRate(bpm: Int, source: String, updatedAt: Date) {
    queue.async { [weak self] in
      guard let self else {
        return
      }
      self.recordCoalescedUpdateIfNeeded(self.pendingLiveHeartRate != nil)
      self.pendingLiveHeartRate = BLELiveHeartRateSnapshot(
        bpm: bpm,
        source: source,
        updatedAt: updatedAt,
        syncAt: updatedAt
      )
      self.publishLastSyncAtLocked(updatedAt)
      self.schedulePublish(now: Date())
    }
  }

  func publishRestingHeartRate(bpm: Double, sampleCount: Int, source: String, updatedAt: Date) {
    queue.async { [weak self] in
      guard let self else {
        return
      }
      self.recordCoalescedUpdateIfNeeded(self.pendingRestingHeartRate != nil)
      self.pendingRestingHeartRate = BLERestingHeartRateSnapshot(
        bpm: bpm,
        sampleCount: sampleCount,
        source: source,
        updatedAt: updatedAt
      )
      self.schedulePublish(now: Date())
    }
  }

  func publishHRV(
    rmssd: Double,
    rrIntervalCount: Int,
    sampleCount: Int,
    source: String,
    updatedAt: Date
  ) {
    queue.async { [weak self] in
      guard let self else {
        return
      }
      self.recordCoalescedUpdateIfNeeded(self.pendingHRV != nil)
      self.pendingHRV = BLEHRVSnapshot(
        rmssd: rmssd,
        rrIntervalCount: rrIntervalCount,
        sampleCount: sampleCount,
        source: source,
        updatedAt: updatedAt
      )
      self.schedulePublish(now: Date())
    }
  }

  func publishLastSyncAt(_ date: Date) {
    queue.async { [weak self] in
      guard let self else {
        return
      }
      self.publishLastSyncAtLocked(date)
      self.schedulePublish(now: Date())
    }
  }

  private func publishLastSyncAtLocked(_ date: Date) {
    if let pendingLastSyncAt, pendingLastSyncAt >= date {
      return
    }
    recordCoalescedUpdateIfNeeded(pendingLastSyncAt != nil)
    pendingLastSyncAt = date
  }

  private func recordCoalescedUpdateIfNeeded(_ coalesced: Bool) {
    if coalesced {
      coalescedUpdateCount += 1
    }
  }

  private func schedulePublish(now: Date) {
    let elapsed = now.timeIntervalSince(lastPublishedAt)
    guard elapsed < publishInterval else {
      publish(now: now)
      return
    }
    guard !publishScheduled else {
      return
    }

    publishScheduled = true
    queue.asyncAfter(deadline: .now() + (publishInterval - elapsed)) { [weak self] in
      self?.publish(now: Date())
    }
  }

  private func publish(now: Date) {
    publishScheduled = false
    let snapshot = BLEUIStateSnapshot(
      liveHeartRate: pendingLiveHeartRate,
      restingHeartRate: pendingRestingHeartRate,
      hrv: pendingHRV,
      lastSyncAt: pendingLastSyncAt,
      coalescedUpdateCount: coalescedUpdateCount
    )
    pendingLiveHeartRate = nil
    pendingRestingHeartRate = nil
    pendingHRV = nil
    pendingLastSyncAt = nil
    coalescedUpdateCount = 0
    lastPublishedAt = now
    onSnapshot?(snapshot)
  }
}
