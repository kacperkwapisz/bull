import Foundation
import UIKit


extension BullAppModel {
  /// The on-device store size above which launch recovery prunes deep
  /// (24-hour frame window) and forces a VACUUM. A healthy store stays well
  /// under this; crossing it means ingest outpaced retention and every bridge
  /// call is paying for it, so the launch pass trades old raw frames (whose
  /// nights are already summarized) for a responsive store.
  static let emergencyRecoveryFileBytesThreshold: Int64 = 700_000_000

  /// Launch-time storage hygiene: remove stale export artifacts, close
  /// capture-session rows orphaned by unclean process exits, prune + compact
  /// the Rust store (deep when oversized; curated daily summaries are never
  /// touched). Runs on the serial Rust startup queue so it finishes before
  /// other launch work contends for the store.
  func performLaunchStorageMaintenance() {
    let databasePath = HealthDataStore.defaultDatabasePath()
    let launchUnixMs = Int64((Date().timeIntervalSince1970 * 1000).rounded())
    rustStartupQueue.async { [weak self] in
      let cleanup = BullStorageJanitor.cleanUpLaunchArtifacts()

      let fileBytes = (try? FileManager.default.attributesOfItem(atPath: databasePath)[.size] as? Int64)
        .flatMap { $0 } ?? 0
      let oversized = fileBytes > Self.emergencyRecoveryFileBytesThreshold

      var maintenanceBody: String
      do {
        let rust = BullRustBridge()
        // No capture session can legitimately be active at launch, so every
        // still-active row from before this instant is an orphan.
        var args: [String: Any] = [
          "database_path": databasePath,
          "stale_sessions_started_before_unix_ms": launchUnixMs,
        ]
        if oversized {
          // Keep the last 24 hours of raw frames (tonight's scoring inputs);
          // older nights already live in the persisted daily summaries.
          let cutoff = ISO8601DateFormatter().string(from: Date().addingTimeInterval(-86_400))
          args["prune_captured_before"] = cutoff
          args["vacuum"] = true
        }
        let report = try rust.request(method: "store.emergency_recovery", args: args)
        let staleClosed = Self.lifecycleInt64Value(report["stale_sessions_finished"]) ?? 0
        let framesRemoved = Self.lifecycleInt64Value(report["frames_removed"]) ?? 0
        let maintenance = report["maintenance"] as? [String: Any]
        let fileBefore = Self.lifecycleInt64Value(maintenance?["file_bytes_before"]) ?? 0
        let fileAfter = Self.lifecycleInt64Value(maintenance?["file_bytes_after"]) ?? 0
        let vacuumed = (maintenance?["vacuumed"] as? Bool) ?? false
        maintenanceBody = String(
          format: "db=%.1fMB->%.1fMB oversized=%@ vacuumed=%@ stale_sessions_closed=%lld frames_removed=%lld",
          Double(fileBefore) / 1_000_000,
          Double(fileAfter) / 1_000_000,
          oversized ? "true" : "false",
          vacuumed ? "true" : "false",
          staleClosed,
          framesRemoved
        )
      } catch {
        maintenanceBody = "store.emergency_recovery failed: \(String(describing: error))"
      }

      DispatchQueue.main.async { [weak self] in
        guard let self else {
          return
        }
        self.ble.record(
          source: "storage.janitor",
          title: "launch.cleanup",
          body: cleanup.bodyText
        )
        self.ble.record(
          source: "storage.janitor",
          title: "launch.maintenance",
          body: maintenanceBody
        )
        // Raw spools whose sessions are finished move to the user's account
        // and are removed locally once the server confirms the upload.
        self.spoolArchiveUploader.archiveFinishedSessions()
        // Drain the captured raw-frame buffer to the user's account and prune
        // the synced+aged copies so the local store stays bounded. Forced: flush
        // everything on launch, including any sub-batch sliver held back during
        // capture.
        self.frameDrainUploader.drain(databasePath: databasePath, force: true)
        // Upload profile + timezone so server-side compute has weight/age/sex
        // for energy and the user's local day for daily rollups. (Curated
        // metrics are computed server-side now, so there's no device-side push.)
        self.metricSyncCoordinator.pushProfile()
      }
    }
  }

  private static func lifecycleInt64Value(_ value: Any?) -> Int64? {
    if let number = value as? NSNumber {
      return number.int64Value
    }
    if let text = value as? String {
      return Int64(text)
    }
    return nil
  }

  func handleAppLifecycleChange(_ phase: String) {
    let power = Self.currentOvernightPowerState()
    ble.record(source: "app.lifecycle", title: "scene_phase", body: "\(phase) | \(power.summary)")

    // Keep the server's copy of the user's profile/timezone current as the app
    // backgrounds, independent of the overnight-guard state below.
    if phase == "active" {
      // Upload the cached APNs token now that the user may have signed in.
      BullPushTokenUploader.uploadCachedTokenIfNeeded()
      refreshServerSyncStatus()
    }
    if phase == "background" || phase == "inactive" {
      // Re-arm the background battery check whenever we leave the foreground.
      BullBackgroundTasks.schedule()
      metricSyncCoordinator.pushProfile()
      // Flush any tail before iOS suspends us; capture-time drains stay batched.
      frameDrainUploader.drain(databasePath: HealthDataStore.defaultDatabasePath(), force: true)
    }

    guard overnightGuardActive else {
      return
    }

    applyOvernightPowerState(power)
    if phase == "background" || phase == "inactive" {
      overnightGuardStatus = "Recording overnight guard | app \(phase)"
      let snapshot = overnightRawSpool.synchronizeActive(reason: "scene_phase_\(phase)")
      overnightGuardRawNotificationCount = snapshot.notificationCount
      overnightGuardRangeTelemetryCount = snapshot.historicalRangePollCount
      overnightGuardCommandWriteCount = snapshot.commandWriteCount
      overnightGuardEventLogCount = snapshot.eventLogCount
      overnightGuardSpoolSizeSummary = Self.overnightSpoolSizeSummary(snapshot)
      if let rawURL = snapshot.rawNotificationsURL {
        overnightGuardSpoolPath = rawURL.path
      }
      if snapshot.lastError != nil {
        applyOvernightRawSpoolWarning(
          from: snapshot,
          reason: "lifecycle_spool_\(phase)",
          warningStatus: "Recording overnight guard | app \(phase) | flush warning"
        )
      }
      ble.record(source: "overnight.guard", title: "lifecycle.flush", body: "phase=\(phase) raw=\(snapshot.notificationCount) range=\(snapshot.historicalRangePollCount) commands=\(snapshot.commandWriteCount) events=\(snapshot.eventLogCount)")
    } else if phase == "active" || phase == "foreground" {
      resumeOvernightGuardStreamsIfReady(reason: "scene_phase_\(phase)")
    }
    writeOvernightGuardStatus(reason: "scene_phase_\(phase)")
  }

  func completeOnboarding() {
    onboardingComplete = true
    ble.record(source: "ui", title: "onboarding.complete")
  }

  func recordUIAction(_ title: String, detail: String = "") {
    ble.record(source: "ui", title: title, body: detail)
  }

  @discardableResult
  func handleDebugCommandDeepLink(_ url: URL) -> Bool {
    guard ["bullswift", "bull"].contains(url.scheme?.lowercased() ?? ""),
          url.host == "debug-command" else {
      return false
    }

    let components = URLComponents(url: url, resolvingAgainstBaseURL: false)
    let queryItems = components?.queryItems ?? []
    let commandID = url.pathComponents.dropFirst().first
      ?? queryItems.first(where: { $0.name == "id" || $0.name == "command" })?.value
      ?? ""
    let payloadHex = queryItems.first(where: { $0.name == "payload" || $0.name == "hex" })?.value
    guard !commandID.isEmpty else {
      ble.record(level: .warn, source: "ble.debug_command", title: "deep_link.invalid", body: url.absoluteString)
      return true
    }

    let normalizedCommandID = commandID.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    guard let command = ble.debugResearchCommands.first(where: { $0.id == normalizedCommandID }) else {
      ble.setDebugCommandStatus("Unknown debug command: \(commandID)")
      ble.record(level: .warn, source: "ble.debug_command", title: "deep_link.unknown", body: commandID)
      return true
    }
    guard command.allowsRemoteInvocation else {
      ble.setDebugCommandStatus("\(command.title) blocked from external deep link")
      ble.record(
        level: .warn,
        source: "ble.debug_command",
        title: "deep_link.blocked",
        body: "\(command.id) risk=\(command.risk)"
      )
      return true
    }

    ble.record(source: "ui", title: "debug_command.deep_link", body: "\(commandID) payload=\(payloadHex ?? "nil")")
    _ = ble.sendDebugResearchCommand(id: command.id, payloadHex: payloadHex, source: "deep_link")
    return true
  }

  func refreshHeartRateHourlyRanges(for date: Date = Date()) {
    heartRateSamplePipeline.refreshHeartRateTimeline(for: date)
  }

  func applyHeartRateTimelineSnapshot(_ snapshot: HeartRateTimelineSnapshot) {
    // Equality guard: the pipeline fires every 1 s; avoid a spurious objectWillChange
    // (and full-view re-render of all BullAppModel observers) when the data is unchanged.
    if snapshot.ranges != heartRateHourlyRanges {
      heartRateHourlyRanges = snapshot.ranges
    }
    if snapshot.status != heartRateStorageStatus {
      heartRateStorageStatus = snapshot.status
    }
  }

  func handleBLEConnectionStateChange(_ state: String) {
    if overnightGuardActive {
      if state == "ready" {
        resumeOvernightGuardStreamsIfReady(reason: "ble_ready")
      } else {
        passiveActivityCaptureWorkItem?.cancel()
        overnightGuardStatus = "Recording overnight guard | connection \(state)"
        refreshOvernightReadiness(reason: "ble_\(state)", record: true)
        writeOvernightGuardStatus(reason: "ble_\(state)")
      }
      return
    }

    guard state == "ready" else {
      passiveActivityCaptureWorkItem?.cancel()
      refreshOvernightReadiness(reason: "ble_\(state)")
      return
    }
    refreshOvernightReadiness(reason: "ble_ready")
    schedulePassiveActivityCapture(reason: "ble_ready")
    scheduleAutoStartRespiratoryPacketWatchIfNeeded()
  }

  func schedulePassiveActivityCapture(reason: String) {
    guard !autoStartHealthPacketCaptureOnReady,
          !autoStartTemperaturePacketCaptureOnReady,
          !autoStartPhysiologyPacketCaptureOnReady,
          !autoStartRespiratoryPacketWatchOnReady,
          activeHealthPacketCapture == nil else {
      return
    }
    passiveActivityCaptureWorkItem?.cancel()
    let workItem = DispatchWorkItem { [weak self] in
      Task { @MainActor in
        self?.attemptStartPassiveActivityCapture(reason: reason)
      }
    }
    passiveActivityCaptureWorkItem = workItem
    DispatchQueue.main.asyncAfter(deadline: .now() + 2, execute: workItem)
  }

  func attemptStartPassiveActivityCapture(reason: String) {
    passiveActivityCaptureWorkItem?.cancel()
    passiveActivityCaptureWorkItem = nil
    guard ble.connectionState == "ready",
          activeHealthPacketCapture == nil,
          !autoStartPhysiologyPacketCaptureOnReady,
          !activitySession.isActive else {
      return
    }
    ble.record(source: "activity.detect", title: "passive_capture.auto_start", body: reason)
    startHealthPacketCapture(duration: Self.passiveActivityCaptureDuration, source: "auto.passive_activity_detection")
  }

  func startMovementPacketValidationTest(timeout: TimeInterval = 45) {
    ble.record(source: "ui.debug", title: "movement_packet_test.start")
    guard ble.connectionState == "ready" else {
      movementPacketValidationStatus = "Connect WHOOP first. Current state: \(ble.connectionState)"
      movementPacketValidationIsRunning = false
      ble.record(level: .warn, source: "activity.detect", title: "movement_packet_test.blocked", body: ble.connectionState)
      return
    }

    movementPacketValidationTimeoutWorkItem?.cancel()
    movementPacketValidation = MovementPacketValidation(startedAt: Date(), timeout: timeout)
    movementPacketValidationIsRunning = true
    movementPacketValidationStatus = "Listening for real WHOOP movement packets"
    ble.record(source: "activity.detect", title: "movement_packet_test.listening", body: "timeout=\(Int(timeout.rounded()))s")

    let workItem = DispatchWorkItem { [weak self] in
      Task { @MainActor in
        self?.finishMovementPacketValidationTimedOut()
      }
    }
    movementPacketValidationTimeoutWorkItem = workItem
    DispatchQueue.main.asyncAfter(deadline: .now() + timeout, execute: workItem)
  }

  func startPhysiologySignalCapture() {
    ble.startPhysiologySignalCapture()
  }

  func stopPhysiologySignalCapture() {
    ble.stopPhysiologySignalCapture()
  }

  func beginOvernightGuardCriticalBackgroundTask(reason: String) {
    guard overnightGuardCriticalBackgroundTaskID == .invalid else {
      ble.record(
        source: "overnight.guard",
        title: "background_task.already_active",
        body: "active_reason=\(overnightGuardCriticalBackgroundTaskReason ?? "unknown") requested_reason=\(reason)"
      )
      return
    }

    let taskName = "Bull Overnight \(reason)"
    let taskID = UIApplication.shared.beginBackgroundTask(withName: taskName) { [weak self] in
      Task { @MainActor [weak self] in
        self?.expireOvernightGuardCriticalBackgroundTask()
      }
    }
    if taskID == .invalid {
      overnightGuardCriticalBackgroundTaskReason = nil
      ble.record(level: .warn, source: "overnight.guard", title: "background_task.denied", body: "reason=\(reason)")
      writeOvernightGuardStatus(reason: "background_task_denied")
      return
    }

    overnightGuardCriticalBackgroundTaskID = taskID
    overnightGuardCriticalBackgroundTaskReason = reason
    ble.record(source: "overnight.guard", title: "background_task.started", body: "reason=\(reason)")
    writeOvernightGuardStatus(reason: "background_task_started")
  }

  func expireOvernightGuardCriticalBackgroundTask() {
    let reason = overnightGuardCriticalBackgroundTaskReason ?? "unknown"
    ble.record(level: .warn, source: "overnight.guard", title: "background_task.expired", body: "reason=\(reason)")
    overnightGuardStatus = "Background time expired during \(reason); keep Bull foregrounded if possible"
    endOvernightGuardCriticalBackgroundTask(reason: "expired_\(reason)")
    writeOvernightGuardStatus(reason: "background_task_expired")
  }

  func endOvernightGuardCriticalBackgroundTask(reason: String) {
    let taskID = overnightGuardCriticalBackgroundTaskID
    guard taskID != .invalid else {
      return
    }
    let activeReason = overnightGuardCriticalBackgroundTaskReason ?? "unknown"
    overnightGuardCriticalBackgroundTaskID = .invalid
    overnightGuardCriticalBackgroundTaskReason = nil
    UIApplication.shared.endBackgroundTask(taskID)
    ble.record(source: "overnight.guard", title: "background_task.ended", body: "active_reason=\(activeReason) reason=\(reason)")
  }

  func startMovementHeartRateCapture() {
    ble.startMovementHeartRateCapture()
  }

  func stopMovementHeartRateCapture() {
    ble.stopMovementHeartRateCapture()
  }

  func enterHighFrequencyHistorySync() {
    ble.enterHighFrequencyHistorySync()
  }

  func exitHighFrequencyHistorySync() {
    ble.exitHighFrequencyHistorySync()
  }

}
