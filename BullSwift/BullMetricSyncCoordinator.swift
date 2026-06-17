import Foundation

/// Track A: keep BullAPI as Bull's long-term clean-data store.
///
/// After the device computes its metrics locally, this coordinator exports the
/// curated daily rollup from the Rust core and pushes it to the server; on a
/// fresh install it restores that history back into the local SQLite store so a
/// reinstall (or app reset) never loses data. Every value originates from the
/// connected device's own sensors — this path never imports physiology from
/// third-party health stores. The curated push is independent of the raw-spool
/// archive uploader and does not require object storage.
///
/// Pushes and restores are serialized and safe to call on every launch /
/// foreground / background. The server upserts curated rows by `(user, day)`,
/// and the Rust import upserts by the local row's idempotency key, so repeated
/// runs converge instead of duplicating.
final class BullMetricSyncCoordinator: @unchecked Sendable {
  private let databasePath: String
  private let bridge = BullRustBridge()
  private let client = CoachAPIClient()
  private let queue = DispatchQueue(label: "com.bull.swift.metric-sync", qos: .utility)
  private var isPushing = false
  private var isRestoring = false
  private var isPushingProfile = false
  private let log: @Sendable (String, String) -> Void

  /// Families the server stores and the Rust core round-trips.
  private static let families = ["recovery", "sleep", "strain", "stress", "energy", "vitals"]

  init(databasePath: String, log: @escaping @Sendable (String, String) -> Void = { _, _ in }) {
    self.databasePath = databasePath
    self.log = log
  }

  /// Upload the connected user's profile + device timezone so server-side
  /// compute can derive energy/calorie estimates and bucket daily rollups on the
  /// user's local calendar day. Optional fields are omitted when unset so the
  /// server degrades honestly instead of guessing. Safe to call on launch /
  /// background; serialized and idempotent (server upserts one row per user).
  func pushProfile() {
    queue.async { [weak self] in
      guard let self, !self.isPushingProfile else {
        return
      }
      guard case .found(let token) = CoachAuthKeychain.loadResult() else {
        return
      }
      self.isPushingProfile = true
      Task { [weak self] in
        guard let self else {
          return
        }
        defer { self.queue.async { self.isPushingProfile = false } }
        let profile = OnboardingProfileSnapshot()
        var body: [String: Any] = ["timezone": TimeZone.current.identifier]
        if profile.weightGrams > 0 {
          body["weight_grams"] = profile.weightGrams
        }
        if !profile.dateOfBirthString.isEmpty {
          body["date_of_birth"] = profile.dateOfBirthString
        }
        if let sex = HealthDataStore.normalizedProfileSex(profile.genderRaw) {
          body["sex"] = sex
        }
        do {
          try await self.client.pushProfile(body: body, token: token)
          self.log("profile.push", "ok")
        } catch {
          self.log("profile.push_failed", String(describing: error))
        }
      }
    }
  }

  /// Export locally-computed curated metrics and push them to the server.
  /// Call after nightly compute and on background. `source` is stamped on every
  /// pushed row as provenance (e.g. "device_nightly_compute").
  func push(source: String) {
    queue.async { [weak self] in
      guard let self, !self.isPushing else {
        return
      }
      // Only proceed with a readable session. Never purge on `.notFound` /
      // `.unavailable`: a locked-device background relaunch can briefly fail to
      // read the keychain, and the next trigger retries.
      guard case .found(let token) = CoachAuthKeychain.loadResult() else {
        return
      }
      self.isPushing = true
      Task { [weak self] in
        guard let self else {
          return
        }
        defer { self.queue.async { self.isPushing = false } }
        await self.runPush(source: source, token: token)
      }
    }
  }

  /// Restore curated metric history from the server into the local store.
  /// Idempotent; typically called once on launch after sign-in so a fresh
  /// install rehydrates its history. `from`/`to` are `YYYY-MM-DD` (optional).
  func restore(from: String? = nil, to: String? = nil) {
    queue.async { [weak self] in
      guard let self, !self.isRestoring else {
        return
      }
      guard case .found(let token) = CoachAuthKeychain.loadResult() else {
        return
      }
      self.isRestoring = true
      Task { [weak self] in
        guard let self else {
          return
        }
        defer { self.queue.async { self.isRestoring = false } }
        await self.runRestore(from: from, to: to, token: token)
      }
    }
  }

  // MARK: - Push

  private func runPush(source: String, token: String) async {
    let body: [String: Any]
    do {
      let result = try bridge.request(
        method: "metrics.export_curated",
        args: ["database_path": databasePath, "source": source]
      )
      guard let exported = result["body"] as? [String: Any] else {
        log("metric_sync.push_no_body", "source=\(source)")
        return
      }
      body = exported
    } catch {
      log("metric_sync.export_failed", String(describing: error))
      return
    }

    if Self.isEmptyBody(body) {
      // Nothing computed yet — honest no-op rather than an empty push.
      return
    }

    do {
      let counts = try await client.pushDailyMetrics(body: body, token: token)
      let ingested = counts["ingested"] as? [String: Any] ?? [:]
      log("metric_sync.pushed", "source=\(source) ingested=\(ingested)")
    } catch {
      // Local data is the source of truth; the next trigger retries. The server
      // upserts by (user, day), so retries never duplicate.
      log("metric_sync.push_failed", String(describing: error))
    }
  }

  // MARK: - Restore

  private func runRestore(from: String?, to: String?, token: String) async {
    let history: [String: Any]
    do {
      history = try await client.fetchMetricHistory(from: from, to: to, token: token)
    } catch {
      log("metric_sync.fetch_failed", String(describing: error))
      return
    }

    var args: [String: Any] = ["database_path": databasePath]
    var total = 0
    for family in Self.families {
      if let rows = history[family] as? [[String: Any]] {
        args[family] = rows
        total += rows.count
      }
    }
    if total == 0 {
      // Honest empty state: server has no curated history for this user yet.
      return
    }

    do {
      let result = try bridge.request(method: "metrics.import_curated", args: args)
      let imported = result["imported"] as? [String: Any] ?? [:]
      log("metric_sync.restored", "rows=\(total) imported=\(imported)")
    } catch {
      log("metric_sync.import_failed", String(describing: error))
    }
  }

  private static func isEmptyBody(_ body: [String: Any]) -> Bool {
    for key in families + ["spo2"] {
      if let array = body[key] as? [Any], !array.isEmpty {
        return false
      }
    }
    return true
  }
}
