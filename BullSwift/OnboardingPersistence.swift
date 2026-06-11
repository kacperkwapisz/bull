import Foundation
import Security

enum OnboardingStorage {
  static let onboardingComplete = "bull.swift.onboardingComplete"
  static let onboardingRedoRequested = "bull.swift.onboardingRedoRequested"
  static let firstName = "bull.swift.profile.firstName"
  static let dateOfBirth = "bull.swift.profile.dateOfBirth"
  static let unitSystem = "bull.swift.profile.unitSystem"
  static let heightInput = "bull.swift.profile.heightInput"
  static let heightFeetInput = "bull.swift.profile.heightFeetInput"
  static let heightInchesInput = "bull.swift.profile.heightInchesInput"
  static let weightInput = "bull.swift.profile.weightInput"
  static let gender = "bull.swift.profile.gender"
  static let heightMm = "bull.swift.profile.heightMm"
  static let weightGrams = "bull.swift.profile.weightGrams"
  static let createdAtUnixMs = "bull.swift.profile.createdAtUnixMs"
  static let timezoneID = "bull.swift.profile.timezoneID"
  static let healthKitPermissionHandled = "bull.swift.permissions.healthKitHandled"
  static let locationPermissionHandled = "bull.swift.permissions.locationHandled"
  static let notificationPermissionHandled = "bull.swift.permissions.notificationHandled"
  static let persistedState = "bull.swift.onboarding.persistedState"
}

struct OnboardingProfileSnapshot: Codable {
  var firstName: String
  var dateOfBirthString: String
  var unitSystemRaw: String
  var heightInput: String
  var heightFeetInput: String
  var heightInchesInput: String
  var weightInput: String
  var genderRaw: String
  var heightMm: Int
  var weightGrams: Int
  var createdAtUnixMs: Int
  var timezoneID: String

  init(
    firstName: String,
    dateOfBirthString: String,
    unitSystemRaw: String,
    heightInput: String,
    heightFeetInput: String,
    heightInchesInput: String,
    weightInput: String,
    genderRaw: String,
    heightMm: Int,
    weightGrams: Int,
    createdAtUnixMs: Int,
    timezoneID: String
  ) {
    self.firstName = firstName
    self.dateOfBirthString = dateOfBirthString
    self.unitSystemRaw = unitSystemRaw
    self.heightInput = heightInput
    self.heightFeetInput = heightFeetInput
    self.heightInchesInput = heightInchesInput
    self.weightInput = weightInput
    self.genderRaw = genderRaw
    self.heightMm = heightMm
    self.weightGrams = weightGrams
    self.createdAtUnixMs = createdAtUnixMs
    self.timezoneID = timezoneID
  }

  init(defaults: UserDefaults = .standard) {
    firstName = defaults.string(forKey: OnboardingStorage.firstName) ?? ""
    dateOfBirthString = defaults.string(forKey: OnboardingStorage.dateOfBirth) ?? ""
    unitSystemRaw = defaults.string(forKey: OnboardingStorage.unitSystem) ?? "imperial"
    heightInput = defaults.string(forKey: OnboardingStorage.heightInput) ?? ""
    heightFeetInput = defaults.string(forKey: OnboardingStorage.heightFeetInput) ?? ""
    heightInchesInput = defaults.string(forKey: OnboardingStorage.heightInchesInput) ?? ""
    weightInput = defaults.string(forKey: OnboardingStorage.weightInput) ?? ""
    genderRaw = defaults.string(forKey: OnboardingStorage.gender) ?? ""
    heightMm = defaults.integer(forKey: OnboardingStorage.heightMm)
    weightGrams = defaults.integer(forKey: OnboardingStorage.weightGrams)
    createdAtUnixMs = defaults.integer(forKey: OnboardingStorage.createdAtUnixMs)
    timezoneID = defaults.string(forKey: OnboardingStorage.timezoneID) ?? ""
  }

  var hasRequiredDetails: Bool {
    !firstName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
      && !dateOfBirthString.isEmpty
      && !genderRaw.isEmpty
      && heightMm > 0
      && weightGrams > 0
  }

  func write(to defaults: UserDefaults = .standard) {
    defaults.set(firstName, forKey: OnboardingStorage.firstName)
    defaults.set(dateOfBirthString, forKey: OnboardingStorage.dateOfBirth)
    defaults.set(unitSystemRaw, forKey: OnboardingStorage.unitSystem)
    defaults.set(heightInput, forKey: OnboardingStorage.heightInput)
    defaults.set(heightFeetInput, forKey: OnboardingStorage.heightFeetInput)
    defaults.set(heightInchesInput, forKey: OnboardingStorage.heightInchesInput)
    defaults.set(weightInput, forKey: OnboardingStorage.weightInput)
    defaults.set(genderRaw, forKey: OnboardingStorage.gender)
    defaults.set(heightMm, forKey: OnboardingStorage.heightMm)
    defaults.set(weightGrams, forKey: OnboardingStorage.weightGrams)
    defaults.set(createdAtUnixMs, forKey: OnboardingStorage.createdAtUnixMs)
    defaults.set(timezoneID, forKey: OnboardingStorage.timezoneID)
  }
}

struct OnboardingPersistedState: Codable {
  var version: Int
  var onboardingComplete: Bool
  var profile: OnboardingProfileSnapshot
}

enum OnboardingProfilePersistence {
  private static let keychainService = "com.bull.swift.onboarding"
  private static let keychainAccount = "profile"

  static func loadState() -> OnboardingPersistedState? {
    if let data = UserDefaults.standard.data(forKey: OnboardingStorage.persistedState),
       let state = try? JSONDecoder().decode(OnboardingPersistedState.self, from: data) {
      return state
    }
    guard
      let data = readKeychainData(),
      let state = try? JSONDecoder().decode(OnboardingPersistedState.self, from: data)
    else {
      return legacyStateFromDefaults()
    }
    UserDefaults.standard.set(data, forKey: OnboardingStorage.persistedState)
    return state
  }

  @discardableResult
  static func restoreIntoDefaultsIfAvailable(restoreCompletion: Bool) -> OnboardingPersistedState? {
    guard let state = loadState(), state.profile.hasRequiredDetails else {
      return nil
    }
    state.profile.write()
    if restoreCompletion, state.onboardingComplete {
      UserDefaults.standard.set(true, forKey: OnboardingStorage.onboardingComplete)
      UserDefaults.standard.set(false, forKey: OnboardingStorage.onboardingRedoRequested)
    }
    return state
  }

  static func saveProfile(_ profile: OnboardingProfileSnapshot, onboardingComplete: Bool) {
    profile.write()
    save(
      OnboardingPersistedState(
        version: 1,
        onboardingComplete: onboardingComplete,
        profile: profile
      )
    )
  }

  static func saveProfileFromDefaults(onboardingComplete: Bool) {
    let profile = OnboardingProfileSnapshot()
    guard profile.hasRequiredDetails else {
      return
    }
    saveProfile(profile, onboardingComplete: onboardingComplete)
  }

  static func markCompleteFromDefaults() {
    UserDefaults.standard.set(true, forKey: OnboardingStorage.onboardingComplete)
    UserDefaults.standard.set(false, forKey: OnboardingStorage.onboardingRedoRequested)
    saveProfileFromDefaults(onboardingComplete: true)
  }

  static func requestRedoFromDefaults() {
    UserDefaults.standard.set(false, forKey: OnboardingStorage.onboardingComplete)
    UserDefaults.standard.set(true, forKey: OnboardingStorage.onboardingRedoRequested)
    let profile = OnboardingProfileSnapshot()
    if profile.hasRequiredDetails {
      saveProfile(profile, onboardingComplete: false)
    }
  }

  private static func save(_ state: OnboardingPersistedState) {
    guard let data = try? JSONEncoder().encode(state) else {
      return
    }
    UserDefaults.standard.set(data, forKey: OnboardingStorage.persistedState)
    writeKeychainData(data)
  }

  private static func legacyStateFromDefaults() -> OnboardingPersistedState? {
    let profile = OnboardingProfileSnapshot()
    guard profile.hasRequiredDetails else {
      return nil
    }
    let state = OnboardingPersistedState(
      version: 1,
      onboardingComplete: UserDefaults.standard.bool(forKey: OnboardingStorage.onboardingComplete),
      profile: profile
    )
    save(state)
    return state
  }

  private static func keychainQuery() -> [String: Any] {
    [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: keychainService,
      kSecAttrAccount as String: keychainAccount,
    ]
  }

  private static func readKeychainData() -> Data? {
    var query = keychainQuery()
    query[kSecReturnData as String] = true
    query[kSecMatchLimit as String] = kSecMatchLimitOne

    var result: CFTypeRef?
    let status = SecItemCopyMatching(query as CFDictionary, &result)
    guard status == errSecSuccess else {
      return nil
    }
    return result as? Data
  }

  private static func writeKeychainData(_ data: Data) {
    let query = keychainQuery()
    let attributes: [String: Any] = [kSecValueData as String: data]
    let status = SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
    if status == errSecItemNotFound {
      var addQuery = query
      addQuery[kSecValueData as String] = data
      addQuery[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
      SecItemAdd(addQuery as CFDictionary, nil)
    }
  }
}
