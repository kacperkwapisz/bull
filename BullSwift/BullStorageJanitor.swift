import Foundation

/// Launch-time storage hygiene. Bull keeps the phone footprint small: stale
/// export artifacts are deleted, finished overnight spools age out after a
/// short debugging window, and the Rust store compacts raw payload copies it
/// no longer needs (curated metrics are never touched).
enum BullStorageJanitor {
  /// Orphaned tmp export bundles older than this are leftovers from an
  /// interrupted export and are safe to delete.
  static let staleTmpExportAge: TimeInterval = 60 * 60
  /// Completed exports are transient share artifacts; age them out.
  static let exportRetentionAge: TimeInterval = 7 * 24 * 60 * 60
  /// Finished overnight spool sessions are kept briefly for debugging. The
  /// newest session always survives so unclean shutdowns can be resumed.
  static let overnightSessionRetentionAge: TimeInterval = 3 * 24 * 60 * 60

  struct CleanupSummary {
    var removedFileCount = 0
    var removedByteCount: Int64 = 0
    var failures: [String] = []

    var bodyText: String {
      let megabytes = Double(removedByteCount) / 1_000_000
      var text = "removed=\(removedFileCount) freed=\(String(format: "%.1f", megabytes))MB"
      if !failures.isEmpty {
        text += " failures=\(failures.joined(separator: "; "))"
      }
      return text
    }
  }

  static func cleanUpLaunchArtifacts(now: Date = Date()) -> CleanupSummary {
    var summary = CleanupSummary()
    cleanStaleTmpExportBundles(now: now, summary: &summary)
    cleanAgedExports(now: now, summary: &summary)
    pruneFinishedOvernightSessions(now: now, summary: &summary)
    return summary
  }

  // MARK: - Export artifacts

  private static func cleanStaleTmpExportBundles(now: Date, summary: inout CleanupSummary) {
    let tmp = FileManager.default.temporaryDirectory
    removeFiles(
      in: tmp,
      olderThan: staleTmpExportAge,
      now: now,
      summary: &summary
    ) { name in
      (name.hasPrefix("bull-local-data-") && name.hasSuffix(".bullbundle.json"))
        || (name.hasPrefix("bull-spool-archive-") && name.hasSuffix(".zip"))
    }
  }

  private static func cleanAgedExports(now: Date, summary: inout CleanupSummary) {
    guard let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first else {
      return
    }
    let exports = documents
      .appendingPathComponent("BullSwift", isDirectory: true)
      .appendingPathComponent("Exports", isDirectory: true)
    removeFiles(
      in: exports,
      olderThan: exportRetentionAge,
      now: now,
      summary: &summary
    ) { name in
      name.hasSuffix(".bullbundle.json")
    }
  }

  // MARK: - Overnight spools

  private static func pruneFinishedOvernightSessions(now: Date, summary: inout CleanupSummary) {
    let fileManager = FileManager.default
    guard let documents = fileManager.urls(for: .documentDirectory, in: .userDomainMask).first else {
      return
    }
    let root = documents
      .appendingPathComponent("BullSwift", isDirectory: true)
      .appendingPathComponent("OvernightGuard", isDirectory: true)
    guard let entries = try? fileManager.contentsOfDirectory(
      at: root,
      includingPropertiesForKeys: [.isDirectoryKey, .contentModificationDateKey],
      options: [.skipsHiddenFiles]
    ) else {
      return
    }
    let sessionDirectories = entries
      .filter { (try? $0.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true }
      .map { url -> (url: URL, modifiedAt: Date) in
        let modifiedAt = (try? url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate)
          ?? .distantPast
        return (url, modifiedAt)
      }
      .sorted { $0.modifiedAt > $1.modifiedAt }

    // The newest session is always kept: it may be unclean and resumable.
    for session in sessionDirectories.dropFirst() {
      guard now.timeIntervalSince(session.modifiedAt) > overnightSessionRetentionAge else {
        continue
      }
      removeItem(at: session.url, summary: &summary)
    }
  }

  // MARK: - Helpers

  private static func removeFiles(
    in directory: URL,
    olderThan age: TimeInterval,
    now: Date,
    summary: inout CleanupSummary,
    matching: (String) -> Bool
  ) {
    let fileManager = FileManager.default
    guard let entries = try? fileManager.contentsOfDirectory(
      at: directory,
      includingPropertiesForKeys: [.contentModificationDateKey, .fileSizeKey],
      options: [.skipsHiddenFiles]
    ) else {
      return
    }
    for url in entries where matching(url.lastPathComponent) {
      let modifiedAt = (try? url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate)
        ?? .distantPast
      guard now.timeIntervalSince(modifiedAt) > age else {
        continue
      }
      removeItem(at: url, summary: &summary)
    }
  }

  private static func removeItem(at url: URL, summary: inout CleanupSummary) {
    let byteCount = totalByteCount(at: url)
    do {
      try FileManager.default.removeItem(at: url)
      summary.removedFileCount += 1
      summary.removedByteCount += byteCount
    } catch {
      summary.failures.append("\(url.lastPathComponent): \(String(describing: error))")
    }
  }

  private static func totalByteCount(at url: URL) -> Int64 {
    let fileManager = FileManager.default
    var isDirectory: ObjCBool = false
    guard fileManager.fileExists(atPath: url.path, isDirectory: &isDirectory) else {
      return 0
    }
    if !isDirectory.boolValue {
      let size = (try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0
      return Int64(size)
    }
    guard let enumerator = fileManager.enumerator(at: url, includingPropertiesForKeys: [.fileSizeKey]) else {
      return 0
    }
    var total: Int64 = 0
    for case let child as URL in enumerator {
      total += Int64((try? child.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0)
    }
    return total
  }
}
