import Foundation

/// Uploads the connected user's profile + device timezone to BullAPI so
/// server-side compute can derive energy/calorie estimates and bucket daily
/// rollups on the user's local calendar day.
///
/// Curated metrics (recovery/sleep/strain/stress/energy/vitals) are computed
/// server-side from the device's uploaded sensor frames and read back via the
/// data API, so the device no longer pushes locally-computed rows or restores
/// history into a local store. Every value still originates from the connected
/// device's own sensors; nothing here imports physiology from third-party
/// health stores.
///
/// Uploads are serialized and safe to call on every launch / foreground /
/// background. The server upserts one profile row per user, so repeats converge.
final class BullMetricSyncCoordinator: @unchecked Sendable {
  private let client = CoachAPIClient()
  private let queue = DispatchQueue(label: "com.bull.swift.metric-sync", qos: .utility)
  private var isPushingProfile = false
  private let log: @Sendable (String, String) -> Void

  init(databasePath _: String, log: @escaping @Sendable (String, String) -> Void = { _, _ in }) {
    // `databasePath` is retained in the signature for call-site compatibility;
    // the coordinator no longer reads or writes the local store.
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
}
