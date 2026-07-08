import CryptoKit
import Foundation
import SQLite3
import Security

/// Errors surfaced by Bull's user-controlled, on-device encrypted backup flow.
enum BullLocalBackupError: LocalizedError {
  case emptyPassphrase
  case databaseNotFound(URL)
  case snapshotFailed(String)
  case archiveEncodingFailed(String)
  case invalidBackup(String)
  case authenticationFailed
  case fileSystemFailure(String)

  var errorDescription: String? {
    switch self {
    case .emptyPassphrase:
      return "Enter a backup passphrase before continuing."
    case .databaseNotFound(let url):
      return "No local Bull database was found at \(url.path)."
    case .snapshotFailed(let reason):
      return "Could not create a consistent local database snapshot: \(reason)"
    case .archiveEncodingFailed(let reason):
      return "Could not package the local backup: \(reason)"
    case .invalidBackup(let reason):
      return "This does not look like a valid Bull backup: \(reason)"
    case .authenticationFailed:
      return "The backup could not be opened. The passphrase may be wrong, or the file may have been changed."
    case .fileSystemFailure(let reason):
      return "Could not update local backup files: \(reason)"
    }
  }
}

/// Creates and restores encrypted local backups.
///
/// Bull keeps this backup device-only and user-controlled: local history is
/// encrypted before it leaves the app sandbox, and restoring never contacts the
/// network.
enum BullLocalBackup {
  private static let backupMagic = Data("BULLBACKUP1\n".utf8)
  private static let archiveMagic = Data("BULLARCHIVE1\n".utf8)
  private static let kdfInfo = Data("BullLocalBackup.v1".utf8)
  private static let tagByteCount = 16
  private static let sqliteTransient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)

  private static let reportCacheNames = ["scores", "inputs"]

  /// Exports the local SQLite store and report caches into an authenticated,
  /// AES-GCM encrypted `.bullbackup` file ready for a share sheet.
  static func exportEncryptedBackup(passphrase: String) throws -> URL {
    try requirePassphrase(passphrase)

    let fileManager = FileManager.default
    let workingDirectory = fileManager.temporaryDirectory
      .appendingPathComponent("BullLocalBackup-\(UUID().uuidString)", isDirectory: true)
    try fileManager.createDirectory(at: workingDirectory, withIntermediateDirectories: true)

    let databaseURL = URL(fileURLWithPath: HealthDataStore.defaultDatabasePath())
    let databaseSnapshotURL = workingDirectory.appendingPathComponent("bull.sqlite")
    try createConsistentDatabaseCopy(from: databaseURL, to: databaseSnapshotURL)

    var files: [ArchiveFile] = [
      ArchiveFile(
        path: "bull.sqlite",
        kind: "sqlite",
        data: try Data(contentsOf: databaseSnapshotURL)
      ),
    ]

    for name in reportCacheNames {
      guard
        let cacheURL = HealthDataStore.reportsCacheURL(name),
        fileManager.fileExists(atPath: cacheURL.path)
      else { continue }
      files.append(
        ArchiveFile(
          path: cacheURL.lastPathComponent,
          kind: "report-cache-\(name)",
          data: try Data(contentsOf: cacheURL)
        )
      )
    }

    let archive = try makeArchive(files: files)
    let sealedPayload = try encrypt(archive, passphrase: passphrase)

    let outputURL = workingDirectory
      .appendingPathComponent("Bull-Local-Backup-\(safeTimestamp()).bullbackup")
    try sealedPayload.write(to: outputURL, options: .atomic)
    return outputURL
  }

  /// Decrypts, verifies, and restores a `.bullbackup` file into the local store.
  /// The current database is copied to a `.pre-restore` file before replacement.
  static func restoreEncryptedBackup(from url: URL, passphrase: String) throws {
    try requirePassphrase(passphrase)

    let didAccess = url.startAccessingSecurityScopedResource()
    defer {
      if didAccess { url.stopAccessingSecurityScopedResource() }
    }

    let encryptedBackup = try Data(contentsOf: url)
    let archive = try decrypt(encryptedBackup, passphrase: passphrase)
    let unpackedFiles = try unpackArchive(archive)

    guard let databaseData = unpackedFiles["bull.sqlite"] else {
      throw BullLocalBackupError.invalidBackup("The archive is missing bull.sqlite.")
    }
    guard databaseData.starts(with: Data("SQLite format 3\0".utf8)) else {
      throw BullLocalBackupError.invalidBackup("The archived database is not a SQLite database.")
    }

    let fileManager = FileManager.default
    let databaseURL = URL(fileURLWithPath: HealthDataStore.defaultDatabasePath())
    let storeDirectory = databaseURL.deletingLastPathComponent()
    try fileManager.createDirectory(at: storeDirectory, withIntermediateDirectories: true)

    let stagingDirectory = storeDirectory
      .appendingPathComponent(".bull-restore-\(UUID().uuidString)", isDirectory: true)
    try fileManager.createDirectory(at: stagingDirectory, withIntermediateDirectories: true)
    defer { try? fileManager.removeItem(at: stagingDirectory) }

    let stagedDatabaseURL = stagingDirectory.appendingPathComponent("bull.sqlite")
    try databaseData.write(to: stagedDatabaseURL, options: .atomic)

    let timestamp = safeTimestamp()
    try backUpCurrentStoreIfPresent(databaseURL: databaseURL, timestamp: timestamp)
    try backUpCurrentReportCachesIfPresent(timestamp: timestamp)

    try replaceSQLiteStore(with: stagedDatabaseURL, at: databaseURL)
    try restoreReportCaches(from: unpackedFiles)
  }
}

private extension BullLocalBackup {
  struct ArchiveFile {
    let path: String
    let kind: String
    let data: Data
  }

  struct ArchiveManifest: Codable {
    let version: Int
    let createdAt: String
    let files: [ArchiveFileManifest]
  }

  struct ArchiveFileManifest: Codable {
    let path: String
    let kind: String
    let byteCount: Int
    let sha256: String
  }

  struct BackupHeader: Codable {
    let version: Int
    let cipher: String
    let kdf: String
    let salt: String
    let nonce: String
  }

  static func requirePassphrase(_ passphrase: String) throws {
    guard !passphrase.isEmpty else {
      throw BullLocalBackupError.emptyPassphrase
    }
  }

  static func createConsistentDatabaseCopy(from sourceURL: URL, to destinationURL: URL) throws {
    let fileManager = FileManager.default
    guard fileManager.fileExists(atPath: sourceURL.path) else {
      throw BullLocalBackupError.databaseNotFound(sourceURL)
    }

    try? fileManager.removeItem(at: destinationURL)

    do {
      try vacuumDatabase(from: sourceURL, into: destinationURL)
    } catch {
      try? fileManager.removeItem(at: destinationURL)
      do {
        try fileManager.copyItem(at: sourceURL, to: destinationURL)
      } catch {
        throw BullLocalBackupError.snapshotFailed(error.localizedDescription)
      }
    }
  }

  static func vacuumDatabase(from sourceURL: URL, into destinationURL: URL) throws {
    var database: OpaquePointer?
    let openResult = sqlite3_open_v2(
      sourceURL.path,
      &database,
      SQLITE_OPEN_READONLY | SQLITE_OPEN_FULLMUTEX,
      nil
    )
    guard openResult == SQLITE_OK, let database else {
      let message = sqliteMessage(database) ?? "SQLite open failed with code \(openResult)."
      if let database { sqlite3_close(database) }
      throw BullLocalBackupError.snapshotFailed(message)
    }
    defer { sqlite3_close(database) }

    sqlite3_busy_timeout(database, 5_000)

    var statement: OpaquePointer?
    let prepareResult = sqlite3_prepare_v2(database, "VACUUM INTO ?", -1, &statement, nil)
    guard prepareResult == SQLITE_OK, let statement else {
      throw BullLocalBackupError.snapshotFailed(sqliteMessage(database) ?? "Could not prepare VACUUM INTO.")
    }
    defer { sqlite3_finalize(statement) }

    let bindResult = destinationURL.path.withCString { pathPointer in
      sqlite3_bind_text(statement, 1, pathPointer, -1, sqliteTransient)
    }
    guard bindResult == SQLITE_OK else {
      throw BullLocalBackupError.snapshotFailed(sqliteMessage(database) ?? "Could not bind snapshot path.")
    }

    let stepResult = sqlite3_step(statement)
    guard stepResult == SQLITE_DONE else {
      throw BullLocalBackupError.snapshotFailed(sqliteMessage(database) ?? "VACUUM INTO failed with code \(stepResult).")
    }
  }

  static func makeArchive(files: [ArchiveFile]) throws -> Data {
    let manifestFiles = files.map { file in
      ArchiveFileManifest(
        path: file.path,
        kind: file.kind,
        byteCount: file.data.count,
        sha256: Data(SHA256.hash(data: file.data)).base64EncodedString()
      )
    }
    let manifest = ArchiveManifest(
      version: 1,
      createdAt: ISO8601DateFormatter().string(from: Date()),
      files: manifestFiles
    )

    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    let manifestData: Data
    do {
      manifestData = try encoder.encode(manifest)
    } catch {
      throw BullLocalBackupError.archiveEncodingFailed(error.localizedDescription)
    }

    var archive = Data()
    archive.append(archiveMagic)
    archive.appendUInt64(UInt64(manifestData.count))
    archive.append(manifestData)
    for file in files {
      archive.append(file.data)
    }
    return archive
  }

  static func unpackArchive(_ archive: Data) throws -> [String: Data] {
    var cursor = 0
    guard archive.consume(prefix: archiveMagic, cursor: &cursor) else {
      throw BullLocalBackupError.invalidBackup("The archive header is missing.")
    }
    let manifestLength = try archive.readUInt64(cursor: &cursor)
    guard manifestLength <= UInt64(Int.max) else {
      throw BullLocalBackupError.invalidBackup("The archive manifest is too large.")
    }
    let manifestByteCount = Int(manifestLength)
    guard cursor + manifestByteCount <= archive.count else {
      throw BullLocalBackupError.invalidBackup("The archive manifest is incomplete.")
    }

    let manifestData = Data(archive[cursor..<(cursor + manifestByteCount)])
    cursor += manifestByteCount

    let manifest: ArchiveManifest
    do {
      manifest = try JSONDecoder().decode(ArchiveManifest.self, from: manifestData)
    } catch {
      throw BullLocalBackupError.invalidBackup("The archive manifest is malformed.")
    }
    guard manifest.version == 1 else {
      throw BullLocalBackupError.invalidBackup("Unsupported archive version \(manifest.version).")
    }

    var unpacked: [String: Data] = [:]
    for file in manifest.files {
      guard isExpectedArchivePath(file.path) else {
        throw BullLocalBackupError.invalidBackup("Unexpected archived file \(file.path).")
      }
      guard unpacked[file.path] == nil else {
        throw BullLocalBackupError.invalidBackup("Duplicate archived file \(file.path).")
      }
      guard file.byteCount >= 0, cursor + file.byteCount <= archive.count else {
        throw BullLocalBackupError.invalidBackup("Archived file \(file.path) is truncated.")
      }
      let data = Data(archive[cursor..<(cursor + file.byteCount)])
      cursor += file.byteCount
      let digest = Data(SHA256.hash(data: data)).base64EncodedString()
      guard digest == file.sha256 else {
        throw BullLocalBackupError.invalidBackup("Archived file \(file.path) failed its checksum.")
      }
      unpacked[file.path] = data
    }

    guard cursor == archive.count else {
      throw BullLocalBackupError.invalidBackup("The archive contains trailing data.")
    }
    return unpacked
  }

  static func encrypt(_ archive: Data, passphrase: String) throws -> Data {
    let salt = try randomData(byteCount: 16)
    let nonceData = try randomData(byteCount: 12)
    let nonce = try AES.GCM.Nonce(data: nonceData)
    let key = deriveKey(passphrase: passphrase, salt: salt)

    let header = BackupHeader(
      version: 1,
      cipher: "AES-256-GCM",
      kdf: "HKDF-SHA256",
      salt: salt.base64EncodedString(),
      nonce: nonceData.base64EncodedString()
    )
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    let headerData: Data
    do {
      headerData = try encoder.encode(header)
    } catch {
      throw BullLocalBackupError.archiveEncodingFailed(error.localizedDescription)
    }
    let sealed = try AES.GCM.seal(archive, using: key, nonce: nonce, authenticating: headerData)

    var output = Data()
    output.append(backupMagic)
    output.appendUInt32(UInt32(headerData.count))
    output.append(headerData)
    output.append(sealed.ciphertext)
    output.append(sealed.tag)
    return output
  }

  static func decrypt(_ encryptedBackup: Data, passphrase: String) throws -> Data {
    var cursor = 0
    guard encryptedBackup.consume(prefix: backupMagic, cursor: &cursor) else {
      throw BullLocalBackupError.invalidBackup("The file header is missing.")
    }

    let headerLength = try encryptedBackup.readUInt32(cursor: &cursor)
    let headerByteCount = Int(headerLength)
    guard cursor + headerByteCount <= encryptedBackup.count else {
      throw BullLocalBackupError.invalidBackup("The file header is incomplete.")
    }

    let headerData = Data(encryptedBackup[cursor..<(cursor + headerByteCount)])
    cursor += headerByteCount

    let header: BackupHeader
    do {
      header = try JSONDecoder().decode(BackupHeader.self, from: headerData)
    } catch {
      throw BullLocalBackupError.invalidBackup("The backup header is malformed.")
    }
    guard header.version == 1, header.cipher == "AES-256-GCM", header.kdf == "HKDF-SHA256" else {
      throw BullLocalBackupError.invalidBackup("Unsupported backup format.")
    }
    guard let salt = Data(base64Encoded: header.salt), let nonceData = Data(base64Encoded: header.nonce) else {
      throw BullLocalBackupError.invalidBackup("The backup header is malformed.")
    }
    guard encryptedBackup.count - cursor > tagByteCount else {
      throw BullLocalBackupError.invalidBackup("The encrypted payload is missing.")
    }

    let encryptedPayload = encryptedBackup[cursor..<encryptedBackup.count]
    let ciphertext = encryptedPayload.dropLast(tagByteCount)
    let tag = encryptedPayload.suffix(tagByteCount)
    let key = deriveKey(passphrase: passphrase, salt: salt)

    do {
      let sealedBox = try AES.GCM.SealedBox(
        nonce: AES.GCM.Nonce(data: nonceData),
        ciphertext: ciphertext,
        tag: tag
      )
      return try AES.GCM.open(sealedBox, using: key, authenticating: headerData)
    } catch {
      throw BullLocalBackupError.authenticationFailed
    }
  }

  static func deriveKey(passphrase: String, salt: Data) -> SymmetricKey {
    HKDF<SHA256>.deriveKey(
      inputKeyMaterial: SymmetricKey(data: Data(passphrase.utf8)),
      salt: salt,
      info: kdfInfo,
      outputByteCount: 32
    )
  }

  static func backUpCurrentStoreIfPresent(databaseURL: URL, timestamp: String) throws {
    guard FileManager.default.fileExists(atPath: databaseURL.path) else { return }
    let backupURL = databaseURL.deletingLastPathComponent()
      .appendingPathComponent("\(databaseURL.lastPathComponent).\(timestamp).pre-restore")
    try createConsistentDatabaseCopy(from: databaseURL, to: backupURL)
  }

  static func backUpCurrentReportCachesIfPresent(timestamp: String) throws {
    for name in reportCacheNames {
      guard
        let cacheURL = HealthDataStore.reportsCacheURL(name),
        FileManager.default.fileExists(atPath: cacheURL.path)
      else { continue }
      let backupURL = cacheURL.deletingLastPathComponent()
        .appendingPathComponent("\(cacheURL.lastPathComponent).\(timestamp).pre-restore")
      do {
        try FileManager.default.copyItem(at: cacheURL, to: backupURL)
      } catch {
        throw BullLocalBackupError.fileSystemFailure(error.localizedDescription)
      }
    }
  }

  static func replaceSQLiteStore(with stagedDatabaseURL: URL, at databaseURL: URL) throws {
    let fileManager = FileManager.default
    do {
      try removeSQLiteSidecars(for: databaseURL)
      if fileManager.fileExists(atPath: databaseURL.path) {
        _ = try fileManager.replaceItemAt(
          databaseURL,
          withItemAt: stagedDatabaseURL,
          backupItemName: nil,
          options: []
        )
      } else {
        try fileManager.moveItem(at: stagedDatabaseURL, to: databaseURL)
      }
      try removeSQLiteSidecars(for: databaseURL)
    } catch {
      throw BullLocalBackupError.fileSystemFailure(error.localizedDescription)
    }
  }

  static func restoreReportCaches(from unpackedFiles: [String: Data]) throws {
    for name in reportCacheNames {
      guard let cacheURL = HealthDataStore.reportsCacheURL(name) else { continue }
      let fileName = "bull-reports-\(name).json"
      do {
        if let data = unpackedFiles[fileName] {
          try FileManager.default.createDirectory(
            at: cacheURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
          )
          try data.write(to: cacheURL, options: .atomic)
        } else if FileManager.default.fileExists(atPath: cacheURL.path) {
          try FileManager.default.removeItem(at: cacheURL)
        }
      } catch {
        throw BullLocalBackupError.fileSystemFailure(error.localizedDescription)
      }
    }
  }

  static func removeSQLiteSidecars(for databaseURL: URL) throws {
    for suffix in ["-wal", "-shm"] {
      let sidecarURL = URL(fileURLWithPath: databaseURL.path + suffix)
      if FileManager.default.fileExists(atPath: sidecarURL.path) {
        try FileManager.default.removeItem(at: sidecarURL)
      }
    }
  }

  static func randomData(byteCount: Int) throws -> Data {
    var data = Data(count: byteCount)
    let result = data.withUnsafeMutableBytes { buffer in
      SecRandomCopyBytes(kSecRandomDefault, byteCount, buffer.baseAddress!)
    }
    guard result == errSecSuccess else {
      throw BullLocalBackupError.archiveEncodingFailed("Secure random generation failed.")
    }
    return data
  }

  static func safeTimestamp() -> String {
    ISO8601DateFormatter().string(from: Date())
      .replacingOccurrences(of: ":", with: "")
      .replacingOccurrences(of: ".", with: "-")
  }

  static func isExpectedArchivePath(_ path: String) -> Bool {
    path == "bull.sqlite" || reportCacheNames.contains { path == "bull-reports-\($0).json" }
  }

  static func sqliteMessage(_ database: OpaquePointer?) -> String? {
    guard let database, let cString = sqlite3_errmsg(database) else { return nil }
    return String(cString: cString)
  }
}

private extension Data {
  mutating func appendUInt32(_ value: UInt32) {
    appendBigEndianInteger(value)
  }

  mutating func appendUInt64(_ value: UInt64) {
    appendBigEndianInteger(value)
  }

  mutating func appendBigEndianInteger<T: FixedWidthInteger>(_ value: T) {
    var bigEndian = value.bigEndian
    Swift.withUnsafeBytes(of: &bigEndian) { buffer in
      append(contentsOf: buffer)
    }
  }

  func consume(prefix: Data, cursor: inout Int) -> Bool {
    guard count >= cursor + prefix.count else { return false }
    guard self[cursor..<(cursor + prefix.count)] == prefix[...] else { return false }
    cursor += prefix.count
    return true
  }

  func readUInt32(cursor: inout Int) throws -> UInt32 {
    let value = try readBigEndianInteger(byteCount: 4, cursor: &cursor)
    guard value <= UInt64(UInt32.max) else {
      throw BullLocalBackupError.invalidBackup("A length field is too large.")
    }
    return UInt32(value)
  }

  func readUInt64(cursor: inout Int) throws -> UInt64 {
    try readBigEndianInteger(byteCount: 8, cursor: &cursor)
  }

  func readBigEndianInteger(byteCount: Int, cursor: inout Int) throws -> UInt64 {
    guard cursor + byteCount <= count else {
      throw BullLocalBackupError.invalidBackup("A length field is incomplete.")
    }
    var value: UInt64 = 0
    for byte in self[cursor..<(cursor + byteCount)] {
      value = (value << 8) | UInt64(byte)
    }
    cursor += byteCount
    return value
  }
}
