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
  static let syncedRetentionByteCap = 32 * 1024 * 1024
  /// Safety bound on bundles drained per pass (each pass is re-entrant-safe).
  private static let maxBundlesPerPass = 512

  private let queue = DispatchQueue(label: "com.bull.swift.frame-drain", qos: .utility)
  private var isRunning = false
  private let log: @Sendable (String, String) -> Void

  init(log: @escaping @Sendable (String, String) -> Void = { _, _ in }) {
    self.log = log
  }

  /// Drain the buffer for `databasePath`. Safe to call on launch/foreground and
  /// after a sync; runs are serialized and skip while one is active.
  func drain(databasePath: String) {
    queue.async { [weak self] in
      guard let self, !self.isRunning else { return }
      guard let token = CoachAuthKeychain.load() else { return }
      self.isRunning = true
      Task { [weak self] in
        guard let self else { return }
        await self.runDrain(databasePath: databasePath, token: token)
        self.queue.async { self.isRunning = false }
      }
    }
  }

  private func runDrain(databasePath: String, token: String) async {
    let bridge = BullRustBridge()
    var uploadedBundles = 0
    var uploadedFrames = 0

    // Re-streamed data the server already has (timestamp ≤ upload watermark) is
    // marked synced without re-uploading, so a band re-pull never resends it.
    if let result = try? bridge.request(
      method: "store.mark_already_uploaded_synced",
      args: ["database_path": databasePath]
    ), let marked = result["marked"] as? Int, marked > 0 {
      log("drain.dedup_already_uploaded", "marked=\(marked)")
    }

    for _ in 0..<Self.maxBundlesPerPass {
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

      let ids = frames.compactMap { $0["evidence_id"] as? String }
      guard let body = Self.encodeBundle(frames) else {
        log("drain.encode_failed", "frames=\(frames.count)")
        break
      }

      do {
        let bundleID = try await Self.upload(body: body, fileName: "frames-\(UUID().uuidString).jsonl.z", token: token)
        // Only after a confirmed upload do we drop the local copies.
        _ = try? bridge.request(
          method: "store.mark_frames_synced",
          args: ["database_path": databasePath, "evidence_ids": ids]
        )
        uploadedBundles += 1
        uploadedFrames += ids.count
        log("drain.bundle_uploaded", "frames=\(ids.count) bundle=\(bundleID)")
      } catch {
        // Keep frames; retry next pass. Server dedupes identical bytes.
        log("drain.upload_failed", "frames=\(ids.count) error=\(String(describing: error))")
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

    // Hard-bound the local store: keep only a small recent slice of synced
    // frames; cascade reclaims their decoded rows. Deep history lives server-side.
    if let pruneResult = try? bridge.request(
      method: "store.prune_synced_to_cap",
      args: ["database_path": databasePath, "max_payload_bytes": Self.syncedRetentionByteCap]
    ) {
      let removed = (pruneResult["removed"] as? Int) ?? 0
      if removed > 0 { log("drain.pruned", "removed=\(removed) cap=\(Self.syncedRetentionByteCap)") }
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

  private static func upload(body bundleBytes: Data, fileName: String, token: String) async throws -> String {
    let boundary = "bull-frames-\(UUID().uuidString)"
    var body = Data()
    body.append(Data("--\(boundary)\r\nContent-Disposition: form-data; name=\"device_id\"\r\n\r\n\(UIDeviceIdentifier.coachDeviceID)\r\n".utf8))
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
