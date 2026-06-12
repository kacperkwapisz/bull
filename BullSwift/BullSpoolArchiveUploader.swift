import Foundation

/// Phase 2 of Bull's storage strategy: the device derives metrics locally,
/// while finished raw overnight spools are zipped, uploaded to the user's
/// account (`/v1/data/uploads`, checksum-deduped server side), and deleted
/// locally only after the server confirms. Keeps the phone footprint small
/// without losing raw fidelity.
final class BullSpoolArchiveUploader: @unchecked Sendable {
  /// Matches the server's bundle cap; oversized archives are left for the
  /// retention janitor instead of failing the upload repeatedly.
  static let maxArchiveBytes = 128 * 1024 * 1024
  /// Sessions must be finished for this long before archiving so the SQLite
  /// mirror drain and any immediate local export finish reading the spool.
  static let minimumFinishedAge: TimeInterval = 60 * 60

  private let queue = DispatchQueue(label: "com.bull.swift.spool-archive", qos: .utility)
  private var isRunning = false
  private let log: @Sendable (String, String) -> Void

  init(log: @escaping @Sendable (String, String) -> Void = { _, _ in }) {
    self.log = log
  }

  /// Scan for finished overnight sessions and archive them. Safe to call on
  /// every launch/foreground; runs are serialized and skip while one is active.
  func archiveFinishedSessions() {
    queue.async { [weak self] in
      guard let self, !self.isRunning else {
        return
      }
      guard let token = CoachAuthKeychain.load() else {
        return
      }
      let candidates = Self.finishedSessionDirectories()
      guard !candidates.isEmpty else {
        return
      }
      self.isRunning = true
      Task { [weak self] in
        guard let self else {
          return
        }
        for session in candidates {
          await self.archive(session: session, token: token)
        }
        self.queue.async { self.isRunning = false }
      }
    }
  }

  // MARK: - Session discovery

  struct FinishedSession {
    let sessionID: String
    let directoryURL: URL
    let status: String
  }

  private static func finishedSessionDirectories() -> [FinishedSession] {
    let fileManager = FileManager.default
    guard let documents = fileManager.urls(for: .documentDirectory, in: .userDomainMask).first else {
      return []
    }
    let root = documents
      .appendingPathComponent("BullSwift", isDirectory: true)
      .appendingPathComponent("OvernightGuard", isDirectory: true)
    guard let entries = try? fileManager.contentsOfDirectory(
      at: root,
      includingPropertiesForKeys: [.isDirectoryKey, .contentModificationDateKey],
      options: [.skipsHiddenFiles]
    ) else {
      return []
    }
    let now = Date()
    return entries.compactMap { url -> FinishedSession? in
      guard (try? url.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true else {
        return nil
      }
      let manifestURL = url.appendingPathComponent("manifest.json")
      guard
        let data = try? Data(contentsOf: manifestURL),
        let manifest = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
        let status = manifest["status"] as? String,
        let sessionID = manifest["session_id"] as? String,
        status != "active"
      else {
        // Active or unreadable sessions stay local: they may be resumed by
        // the unclean-shutdown recovery path. The retention janitor remains
        // the backstop for sessions that never reach a terminal status.
        return nil
      }
      // Recently finished sessions stay local briefly: the SQLite mirror
      // drain and post-stop local exports still read these files.
      let modifiedAt = (try? url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate)
        ?? now
      guard now.timeIntervalSince(modifiedAt) >= minimumFinishedAge else {
        return nil
      }
      return FinishedSession(sessionID: sessionID, directoryURL: url, status: status)
    }
  }

  // MARK: - Archive + upload

  private func archive(session: FinishedSession, token: String) async {
    let zipURL = FileManager.default.temporaryDirectory
      .appendingPathComponent("bull-spool-archive-\(session.sessionID).zip")
    defer {
      try? FileManager.default.removeItem(at: zipURL)
    }
    do {
      try Self.zipDirectory(at: session.directoryURL, to: zipURL)
    } catch {
      log("archive.zip_failed", "session=\(session.sessionID) error=\(String(describing: error))")
      return
    }

    let byteCount = (try? zipURL.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0
    guard byteCount > 0 else {
      log("archive.zip_empty", "session=\(session.sessionID)")
      return
    }
    guard byteCount <= Self.maxArchiveBytes else {
      log("archive.too_large", "session=\(session.sessionID) bytes=\(byteCount)")
      return
    }

    do {
      let bundleID = try await Self.upload(
        zipURL: zipURL,
        fileName: "\(session.sessionID).zip",
        token: token
      )
      try FileManager.default.removeItem(at: session.directoryURL)
      log(
        "archive.uploaded",
        "session=\(session.sessionID) bytes=\(byteCount) bundle=\(bundleID) local_removed=true"
      )
    } catch {
      // Local data is kept; the next launch retries. The server dedupes
      // identical bytes by checksum, so retries never duplicate storage.
      log("archive.upload_failed", "session=\(session.sessionID) error=\(String(describing: error))")
    }
  }

  /// Zip a directory using file coordination's for-uploading copy, which
  /// produces a zip archive for directory URLs.
  private static func zipDirectory(at directoryURL: URL, to destinationURL: URL) throws {
    var coordinatorError: NSError?
    var moveError: Error?
    let coordinator = NSFileCoordinator()
    coordinator.coordinate(
      readingItemAt: directoryURL,
      options: [.forUploading],
      error: &coordinatorError
    ) { zippedURL in
      do {
        try? FileManager.default.removeItem(at: destinationURL)
        try FileManager.default.copyItem(at: zippedURL, to: destinationURL)
      } catch {
        moveError = error
      }
    }
    if let coordinatorError {
      throw coordinatorError
    }
    if let moveError {
      throw moveError
    }
  }

  private static func upload(zipURL: URL, fileName: String, token: String) async throws -> String {
    let boundary = "bull-archive-\(UUID().uuidString)"
    var body = Data()
    func appendField(_ name: String, _ value: String) {
      body.append(Data("--\(boundary)\r\nContent-Disposition: form-data; name=\"\(name)\"\r\n\r\n\(value)\r\n".utf8))
    }
    appendField("device_id", UIDeviceIdentifier.coachDeviceID)
    body.append(Data(
      "--\(boundary)\r\nContent-Disposition: form-data; name=\"bundle\"; filename=\"\(fileName)\"\r\nContent-Type: application/zip\r\n\r\n".utf8
    ))
    body.append(try Data(contentsOf: zipURL))
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
