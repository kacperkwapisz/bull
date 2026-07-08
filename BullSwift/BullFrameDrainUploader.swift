import Foundation
import UIKit

/// Drains the local raw-frame buffer to the user's account so the device never
/// becomes a permanent frame store. Frames are pulled in byte-bounded bundles,
/// uploaded to `/v1/data/uploads` (the server holds the durable copy, deduped by
/// checksum), and only then deleted locally (`mark_frames_synced`). Synced
/// frames past the on-device retention window are pruned — the `decoded_frames`
/// cascade reclaims their decoded rows. A failed upload keeps the frames for the
/// next pass (no data loss before a confirmed 2xx).
///
/// This is the single raw-upload path; it supersedes the overnight spool
/// archive once enabled.
final class BullFrameDrainUploader: @unchecked Sendable {
  /// Decoded-binary payload budget per bundle. The compressed upload is smaller;
  /// kept well under the server's bundle cap.
  static let bundlePayloadByteBudget = 2 * 1024 * 1024
  /// Hard cap on retained *synced* raw payload. The store is kept this small
  /// regardless of how much has been synced — only a recent slice stays on
  /// device (for the live/recent view); deep history comes from the server.
  // Thin-client retention: every frame is uploaded to the server, which durably
  // stores the bundle bytes and computes all metrics; the device reads display
  // data back from the server. So once frames are confirmed synced there is no
  // reason to keep them locally — drop all synced frames and retain only the
  // not-yet-uploaded buffer. A far-future cutoff means "all synced".
  static let drainAllSyncedBefore = "9999-12-31T23:59:59Z"
  /// Local-first raw-frame retention: the phone only needs raw captured frames
  /// long enough to score the current night and calibrate a strain target from a
  /// few recent days. Personal baselines and long-run trends are read from
  /// persisted nightly summaries (`daily_recovery_metrics`, `daily_sleep_metrics`,
  /// …), which pruning never touches — so raw frames can be released quickly
  /// while a full history of cheap summaries is kept. Holding less raw sensor data
  /// on-device is also better for privacy. Server mode drops all synced frames.
  static let localFirstRetentionDays = 4

  /// True when compute runs on-device (local-first). Nonisolated so the drain
  /// queue can read it without hopping to the main actor.
  static func localComputeMode() -> Bool {
    let args = ProcessInfo.processInfo.arguments
    if args.contains("--bull-compute-server") { return false }
    if args.contains("--bull-compute-local") { return true }
    if let raw = UserDefaults.standard.string(forKey: "bull.compute.mode") { return raw != "server" }
    return true
  }

  /// Whether raw frames are uploaded to the cloud as an OPTIONAL backup. Local-
  /// first computes everything on-device, so uploading is opt-in and defaults
  /// OFF: physiological data stays on the device unless the user enables cloud
  /// backup. In server compute mode uploading stays on (the server needs the
  /// frames to compute).
  static func cloudBackupEnabled() -> Bool {
    let args = ProcessInfo.processInfo.arguments
    if args.contains("--bull-cloud-backup") { return true }
    if args.contains("--bull-no-cloud-backup") { return false }
    if !localComputeMode() { return true }
    return UserDefaults.standard.bool(forKey: "bull.cloud.backup.enabled")
  }

  /// The `captured_before` retention cutoff: keep a recent window of frames on
  /// device so the phone can score them locally; prune older frames to bound
  /// storage. Nonisolated for the background drain queue.
  static func retentionCutoff(now: Date = Date()) -> String {
    let cutoff = now.addingTimeInterval(-Double(localFirstRetentionDays) * 86_400)
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime]
    return formatter.string(from: cutoff)
  }
  /// Safety bound on bundles drained per pass (each pass is re-entrant-safe).
  private static let maxBundlesPerPass = 512
  /// Minimum decoded payload to bother uploading on a non-forced (continuous)
  /// drain. Small slivers are held locally until this much accumulates so a
  /// live capture produces a handful of sizable bundles instead of hundreds of
  /// tiny ones (each of which would cost a full server-side parse). Forced
  /// drains (launch/background) flush everything regardless.
  static let minBatchPayloadBytes = 256 * 1024

  private let queue = DispatchQueue(label: "com.bull.swift.frame-drain", qos: .utility)
  private var isRunning = false
  private var uploadRetryCount = 0
  private let log: @Sendable (String, String) -> Void

  init(log: @escaping @Sendable (String, String) -> Void = { _, _ in }) {
    self.log = log
  }

  /// Drain the buffer for `databasePath`. Safe to call on launch/foreground and
  /// after a sync; runs are serialized and skip while one is active.
  func drain(databasePath: String, force: Bool = false) {
    queue.async { [weak self] in
      guard let self, !self.isRunning else { return }
      let cloudBackup = Self.cloudBackupEnabled()
      // Cloud backup needs a signed-in token; local-first retention maintenance
      // does not. When backup is off we still run to bound local storage.
      let token = cloudBackup ? CoachAuthKeychain.load() : nil
      if cloudBackup && token == nil { return }
      self.isRunning = true
      Task { [weak self] in
        guard let self else { return }
        await self.runDrain(databasePath: databasePath, token: token, force: force)
        self.queue.async { self.isRunning = false }
      }
    }
  }

  private func runDrain(databasePath: String, token: String?, force: Bool = false) async {
    let bridge = BullRustBridge()
    var uploadedBundles = 0
    var uploadedFrames = 0
    let cloudBackup = token != nil

    // Local-first without cloud backup: no upload. Just bound local storage to
    // the on-device scoring window and reclaim space, then return.
    if !cloudBackup {
      if let pruneResult = try? bridge.request(
        method: "store.prune_raw_evidence_before",
        args: ["database_path": databasePath, "captured_before": Self.retentionCutoff()]
      ) {
        let removed = (pruneResult["removed"] as? Int) ?? 0
        if removed > 0 { log("drain.local_retention_pruned", "removed=\(removed)") }
      }
      _ = try? bridge.request(method: "store.maintain", args: ["database_path": databasePath])
      return
    }
    guard let token else { return }

    // Re-streamed data the server already has (timestamp ≤ upload watermark) is
    // marked synced without re-uploading, so a band re-pull never resends it.
    if let result = try? bridge.request(
      method: "store.mark_already_uploaded_synced",
      args: ["database_path": databasePath]
    ), let marked = result["marked"] as? Int, marked > 0 {
      log("drain.dedup_already_uploaded", "marked=\(marked)")
    }

    for pass in 0..<Self.maxBundlesPerPass {
      let frames: [[String: Any]]
      do {
        let response = try bridge.request(
          method: "store.drain_frame_bundle",
          args: [
            "database_path": databasePath,
            "max_payload_bytes": Self.bundlePayloadByteBudget,
          ]
        )
        frames = response["frames"] as? [[String: Any]] ?? []
      } catch {
        log("drain.bundle_query_failed", String(describing: error))
        break
      }
      if frames.isEmpty { break }

      // Batch gate: on a non-forced (continuous) drain, hold the buffer until at
      // least minBatchPayloadBytes has accumulated so we don't fragment into
      // tiny bundles. The first bundle holds up to 2MB of the oldest unsynced
      // data, so its size reflects what's available; once we commit to draining,
      // later passes upload the rest unconditionally.
      if pass == 0 && !force {
        let pendingBytes = frames.reduce(0) {
          $0 + (($1["payload_hex"] as? String)?.count ?? 0) / 2
        }
        if pendingBytes < Self.minBatchPayloadBytes {
          log("drain.batch_hold", "pending=\(pendingBytes) min=\(Self.minBatchPayloadBytes)")
          break
        }
      }

      let ids = frames.compactMap { $0["evidence_id"] as? String }
      guard let body = Self.encodeBundle(frames) else {
        log("drain.encode_failed", "frames=\(frames.count)")
        break
      }

      do {
        let bundleID = try await Self.upload(body: body, fileName: "frames-\(UUID().uuidString).jsonl.z", token: token, packetCount: ids.count, retryCount: uploadRetryCount)
        // Only after a confirmed upload do we drop the local copies.
        _ = try? bridge.request(
          method: "store.mark_frames_synced",
          args: ["database_path": databasePath, "evidence_ids": ids]
        )
        uploadedBundles += 1
        uploadedFrames += ids.count
        uploadRetryCount = 0
        log("drain.bundle_uploaded", "frames=\(ids.count) bundle=\(bundleID)")
      } catch {
        // Keep frames; retry next pass. Server dedupes identical bytes.
        uploadRetryCount += 1
        log("drain.upload_failed", "frames=\(ids.count) retry=\(uploadRetryCount) error=\(String(describing: error))")
        break
      }
    }

    // Advance the persistent upload watermark to the newest uploaded timestamp
    // BEFORE pruning (pruning deletes the synced rows it reads). The historical
    // sync skips re-streamed data at/under this mark instead of re-uploading it.
    if uploadedFrames > 0 {
      _ = try? bridge.request(
        method: "store.advance_sync_watermark",
        args: ["database_path": databasePath]
      )
    }

    // Collapse the local store to the unsynced buffer: delete every synced frame
    // (cascade reclaims their decoded rows). Deep history + all display data live
    // server-side and are read back over the API.
    if let pruneResult = try? bridge.request(
      method: "store.prune_synced_frames",
      args: ["database_path": databasePath, "captured_before": Self.retentionCutoff()]
    ) {
      let removed = (pruneResult["removed"] as? Int) ?? 0
      if removed > 0 { log("drain.pruned", "removed=\(removed)") }
    }

    // Fold the freed space back and truncate the WAL.
    _ = try? bridge.request(method: "store.maintain", args: ["database_path": databasePath])

    if uploadedBundles > 0 {
      log("drain.pass_complete", "bundles=\(uploadedBundles) frames=\(uploadedFrames)")
    }
  }

  // MARK: - Bundle encoding (JSONL of raw frames, zlib-compressed)

  /// One JSON object per line: `{evidence_id, captured_at, sha256, payload_hex}`.
  /// Compressed with zlib; the server stores the bytes verbatim and a later
  /// server-side parser re-reads them.
  static func encodeBundle(_ frames: [[String: Any]]) -> Data? {
    var jsonl = Data()
    for frame in frames {
      guard
        let evidenceID = frame["evidence_id"] as? String,
        let capturedAt = frame["captured_at"] as? String,
        let payloadHex = frame["payload_hex"] as? String
      else { continue }
      let sha = frame["sha256"] as? String ?? ""
      let line: [String: Any] = [
        "evidence_id": evidenceID,
        "captured_at": capturedAt,
        "sha256": sha,
        "payload_hex": payloadHex,
      ]
      guard let data = try? JSONSerialization.data(withJSONObject: line) else { continue }
      jsonl.append(data)
      jsonl.append(0x0A) // newline
    }
    guard !jsonl.isEmpty else { return nil }
    return (try? (jsonl as NSData).compressed(using: .zlib)) as Data?
  }

  // MARK: - Upload (multipart, same surface as the spool archive uploader)

  private static func upload(body bundleBytes: Data, fileName: String, token: String, packetCount: Int, retryCount: Int) async throws -> String {
    let boundary = "bull-frames-\(UUID().uuidString)"
    var body = Data()
    body.append(Data("--\(boundary)\r\nContent-Disposition: form-data; name=\"device_id\"\r\n\r\n\(UIDeviceIdentifier.coachDeviceID)\r\n".utf8))
    body.append(Data("--\(boundary)\r\nContent-Disposition: form-data; name=\"source\"\r\n\r\nframe_drain\r\n".utf8))
    body.append(Data("--\(boundary)\r\nContent-Disposition: form-data; name=\"packet_count\"\r\n\r\n\(packetCount)\r\n".utf8))
    body.append(Data("--\(boundary)\r\nContent-Disposition: form-data; name=\"retry_count\"\r\n\r\n\(retryCount)\r\n".utf8))
    body.append(Data(
      "--\(boundary)\r\nContent-Disposition: form-data; name=\"bundle\"; filename=\"\(fileName)\"\r\nContent-Type: application/zlib\r\n\r\n".utf8
    ))
    body.append(bundleBytes)
    body.append(Data("\r\n--\(boundary)--\r\n".utf8))

    var request = URLRequest(url: CoachAPIConfiguration.dataUploadsURL)
    request.httpMethod = "POST"
    request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    request.setValue("multipart/form-data; boundary=\(boundary)", forHTTPHeaderField: "Content-Type")
    request.timeoutInterval = 300

    let (data, response) = try await URLSession.shared.upload(for: request, from: body)
    guard let http = response as? HTTPURLResponse else {
      throw CoachAuthError.invalidResponse
    }
    guard (200..<300).contains(http.statusCode) else {
      throw CoachAuthError.http(http.statusCode, String(data: data, encoding: .utf8) ?? "")
    }
    let json = (try? JSONSerialization.jsonObject(with: data) as? [String: Any]) ?? [:]
    return (json["bundleId"] as? String) ?? "unknown"
  }
}
