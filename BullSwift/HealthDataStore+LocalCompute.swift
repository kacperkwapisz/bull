import Foundation

// MARK: - Local-first compute wiring
//
// Bull scores the connected device's own captured frames ON-DEVICE using the
// linked Rust core, honouring the local-first / privacy principle (physiological
// data is processed on the device, not shipped to a server to be computed). The
// cloud remains available as an optional fallback and, later, opt-in backup.
//
// `runHomeRefresh` (see HealthDataStore+Snapshots) calls `fetchHomePayload()`,
// which prefers the on-device compute and falls back to the server BFF only when
// local compute yields nothing (e.g. before the core is ready or a fresh install
// with no local store yet).

extension HealthDataStore {
  enum ComputeMode: String {
    case local
    case server
  }

  /// Resolved compute mode. Defaults to on-device (`.local`). Override with a
  /// launch argument (`--bull-compute-server` / `--bull-compute-local`) or the
  /// `bull.compute.mode` user default.
  static var computeMode: ComputeMode {
    let args = ProcessInfo.processInfo.arguments
    if args.contains("--bull-compute-server") { return .server }
    if args.contains("--bull-compute-local") { return .local }
    if let raw = UserDefaults.standard.string(forKey: "bull.compute.mode"),
       let mode = ComputeMode(rawValue: raw) {
      return mode
    }
    return .local
  }

  /// Build the on-device compute profile from the stored onboarding profile.
  /// Missing fields are simply omitted so energy/age-derived outputs degrade
  /// honestly rather than guessing.
  static func localComputeProfile() -> LocalHomeProfile {
    let snapshot = OnboardingProfileSnapshot(defaults: .standard)
    let weightKg: Double? = snapshot.weightGrams > 0 ? Double(snapshot.weightGrams) / 1_000.0 : nil
    let heightCm: Double? = snapshot.heightMm > 0 ? Double(snapshot.heightMm) / 10.0 : nil
    let ageYears = ageInYears(fromDateOfBirthString: snapshot.dateOfBirthString)
    let sex = HealthDataStore.normalizedProfileSex(snapshot.genderRaw)
    let timezone = snapshot.timezoneID.isEmpty ? TimeZone.current.identifier : snapshot.timezoneID
    return LocalHomeProfile(
      weightKg: weightKg,
      heightCm: heightCm,
      ageYears: ageYears,
      sex: sex,
      timezone: timezone
    )
  }

  /// Home payload for `runHomeRefresh`. Prefers on-device compute; falls back to
  /// the server BFF only when local compute produced no scores.
  nonisolated static func fetchHomePayload(dateKey: String? = nil) async -> (home: [String: Any], source: String) {
    let (mode, dbPath, profile): (ComputeMode, String, LocalHomeProfile) = await MainActor.run {
      (HealthDataStore.computeMode, HealthDataStore.defaultDatabasePath(), HealthDataStore.localComputeProfile())
    }
    if mode == .local {
      let local = await LocalHomeService.computeHome(databasePath: dbPath, profile: profile)
      if homePayloadHasScores(local) {
        return (local, "device")
      }
      // Nothing computed locally yet — try the server as a fallback so a fresh
      // install / not-yet-ready core still shows any previously computed scores.
      let server = await fetchHome(dateKey: dateKey)
      if homePayloadHasScores(server) {
        return (server, "server-fallback")
      }
      return (local, "device")
    }
    let server = await fetchHome(dateKey: dateKey)
    return (server, "server")
  }

  nonisolated static func homePayloadHasScores(_ home: [String: Any]) -> Bool {
    for family in ["recovery", "sleep", "strain", "stress"] {
      if let report = home[family] as? [String: Any], !report.isEmpty {
        return true
      }
    }
    if let inputs = home["inputs"] as? [String: [String: Any]], !inputs.isEmpty {
      return true
    }
    return false
  }

  private static func ageInYears(fromDateOfBirthString value: String) -> Int? {
    guard !value.isEmpty else { return nil }
    let formatter = DateFormatter()
    formatter.calendar = Calendar(identifier: .gregorian)
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.timeZone = TimeZone(secondsFromGMT: 0)
    formatter.dateFormat = "yyyy-MM-dd"
    guard let birth = formatter.date(from: String(value.prefix(10))) else { return nil }
    let years = Calendar(identifier: .gregorian).dateComponents([.year], from: birth, to: Date()).year
    guard let years, years >= 0, years <= 120 else { return nil }
    return years
  }
}
