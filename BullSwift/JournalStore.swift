import Foundation

/// Loads the journal catalog, today's entry, and behavior insights from the
/// user's Bull account, and saves the day's logged behaviors back. Behaviors are
/// the user's own self-reported habits — never physiological data.
@MainActor
final class JournalStore: ObservableObject {
  @Published var catalog: [JournalCatalogTag] = []
  @Published var customTags: [String] = []
  @Published var selected: Set<String> = []
  @Published var note: String = ""
  @Published var insights: BehaviorInsights?
  @Published var metric: InsightMetric = .recovery {
    didSet { Task { await loadInsights() } }
  }
  @Published var isLoading = false
  @Published var isSaving = false
  @Published var insightsUnavailable = false
  @Published var errorMessage: String?
  @Published var savedAt: Date?

  private let todayKey: String

  init() {
    let formatter = DateFormatter()
    formatter.calendar = Calendar.current
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.dateFormat = "yyyy-MM-dd"
    todayKey = formatter.string(from: Date())
  }

  /// All tags to show in the picker: catalog plus any custom tags from history.
  var allTagsByCategory: [(category: String, tags: [JournalCatalogTag])] {
    var groups: [String: [JournalCatalogTag]] = [:]
    for tag in catalog {
      groups[tag.category, default: []].append(tag)
    }
    let custom = customTags.map { JournalCatalogTag(tag: $0, label: $0, category: "custom") }
    if !custom.isEmpty {
      groups["custom", default: []].append(contentsOf: custom)
    }
    var order = JournalCatalogTag.categoryOrder
    if !custom.isEmpty { order.append("custom") }
    return order.compactMap { key in
      guard let tags = groups[key], !tags.isEmpty else { return nil }
      return (key, tags)
    }
  }

  func load() async {
    isLoading = true
    errorMessage = nil
    defer { isLoading = false }
    do {
      async let catalogTask = fetchCatalog()
      async let historyTask = fetchHistory()
      let (cat, history) = try await (catalogTask, historyTask)
      catalog = cat
      // Today's selection.
      if let today = history.first(where: { $0.day == todayKey }) {
        selected = Set(today.behaviors.map(\.tag))
        note = today.note ?? ""
      }
      // Custom tags = anything logged that isn't in the catalog.
      let known = Set(cat.map(\.tag))
      var custom = Set<String>()
      for entry in history {
        for behavior in entry.behaviors where !known.contains(behavior.tag) {
          custom.insert(behavior.tag)
        }
      }
      customTags = custom.sorted()
    } catch {
      errorMessage = "Couldn't load your journal."
    }
    await loadInsights()
  }

  func toggle(_ tag: String) {
    if selected.contains(tag) {
      selected.remove(tag)
    } else {
      selected.insert(tag)
    }
  }

  func addCustomTag(_ raw: String) {
    let normalized = raw
      .trimmingCharacters(in: .whitespacesAndNewlines)
      .lowercased()
      .replacingOccurrences(of: " ", with: "_")
    guard !normalized.isEmpty else { return }
    if !customTags.contains(normalized) && !catalog.contains(where: { $0.tag == normalized }) {
      customTags.append(normalized)
      customTags.sort()
    }
    selected.insert(normalized)
  }

  func save() async {
    guard let token = CoachAuthKeychain.load() else {
      errorMessage = "Sign in to save your journal."
      return
    }
    isSaving = true
    errorMessage = nil
    defer { isSaving = false }
    let behaviors = selected.map { ["tag": $0] }
    var body: [String: Any] = ["day": todayKey, "behaviors": behaviors]
    if !note.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
      body["note"] = note
    }
    do {
      var request = URLRequest(url: CoachAPIConfiguration.dataJournalURL)
      request.httpMethod = "POST"
      request.setValue("application/json", forHTTPHeaderField: "Content-Type")
      request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
      request.httpBody = try JSONSerialization.data(withJSONObject: body)
      let (_, response) = try await URLSession.shared.data(for: request)
      guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
        throw CoachAPIError.invalidResponse
      }
      savedAt = Date()
      await loadInsights()
    } catch {
      errorMessage = "Couldn't save. Try again."
    }
  }

  func loadInsights() async {
    insightsUnavailable = false
    guard let token = CoachAuthKeychain.load() else { return }
    var components = URLComponents(url: CoachAPIConfiguration.dataJournalInsightsURL, resolvingAgainstBaseURL: false)
    components?.queryItems = [URLQueryItem(name: "metric", value: metric.rawValue)]
    guard let url = components?.url else { return }
    var request = URLRequest(url: url)
    request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    do {
      let (data, response) = try await URLSession.shared.data(for: request)
      guard let http = response as? HTTPURLResponse else { return }
      if http.statusCode == 503 {
        insightsUnavailable = true
        insights = nil
        return
      }
      guard (200..<300).contains(http.statusCode) else { return }
      let decoded = try JSONDecoder().decode(InsightsEnvelope.self, from: data)
      insights = decoded.insights
    } catch {
      insights = nil
    }
  }

  // MARK: - Fetch helpers

  private func fetchCatalog() async throws -> [JournalCatalogTag] {
    guard let token = CoachAuthKeychain.load() else { return [] }
    var request = URLRequest(url: CoachAPIConfiguration.dataJournalCatalogURL)
    request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    let (data, response) = try await URLSession.shared.data(for: request)
    guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
      throw CoachAPIError.invalidResponse
    }
    return try JSONDecoder().decode(CatalogEnvelope.self, from: data).tags
  }

  private func fetchHistory() async throws -> [JournalEntryDTO] {
    guard let token = CoachAuthKeychain.load() else { return [] }
    var request = URLRequest(url: CoachAPIConfiguration.dataJournalURL)
    request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    let (data, response) = try await URLSession.shared.data(for: request)
    guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
      throw CoachAPIError.invalidResponse
    }
    return try JSONDecoder().decode(HistoryEnvelope.self, from: data).rows
  }

  private struct CatalogEnvelope: Codable { let tags: [JournalCatalogTag] }
  private struct HistoryEnvelope: Codable { let rows: [JournalEntryDTO] }
  private struct InsightsEnvelope: Codable { let insights: BehaviorInsights }
}
