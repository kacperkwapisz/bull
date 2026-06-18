import Foundation

/// A behavior the app offers in the journal picker. Catalog JSON is camelCase
/// (served straight from the TS catalog), so the default decoder maps it.
struct JournalCatalogTag: Codable, Identifiable, Hashable {
  let tag: String
  let label: String
  let category: String
  var hasAmount: Bool?
  var unit: String?
  var autoSource: String?

  var id: String { tag }

  static let categoryOrder = [
    "substances", "nutrition", "sleep", "mind", "activity", "lifestyle",
  ]

  static func categoryTitle(_ key: String) -> String {
    switch key {
    case "substances": return "Substances"
    case "nutrition": return "Nutrition"
    case "sleep": return "Sleep"
    case "mind": return "Mind"
    case "activity": return "Activity"
    case "lifestyle": return "Lifestyle"
    default: return key.capitalized
    }
  }
}

/// One logged behavior in a day's entry.
struct JournalBehavior: Codable, Hashable {
  let tag: String
  var amount: Double?
}

/// A stored day entry as returned by the list endpoint (camelCase columns).
struct JournalEntryDTO: Codable {
  let day: String
  let behaviors: [JournalBehavior]
  let note: String?
}

/// A single behavior's measured association with the metric. Insights JSON is
/// snake_case (serialized by the Rust engine), mapped via CodingKeys.
struct BehaviorImpact: Codable, Identifiable, Hashable {
  let behavior: String
  let daysWith: Int
  let daysWithout: Int
  let meanWith: Double
  let meanWithout: Double
  let delta: Double
  let strength: String

  var id: String { behavior }

  enum CodingKeys: String, CodingKey {
    case behavior
    case daysWith = "days_with"
    case daysWithout = "days_without"
    case meanWith = "mean_with"
    case meanWithout = "mean_without"
    case delta
    case strength
  }
}

/// The full insight summary from the engine.
struct BehaviorInsights: Codable {
  let metric: String
  let analyzedDays: Int
  let impacts: [BehaviorImpact]
  let insufficient: [String]
  let correlationOnly: Bool

  enum CodingKeys: String, CodingKey {
    case metric
    case analyzedDays = "analyzed_days"
    case impacts
    case insufficient
    case correlationOnly = "correlation_only"
  }

  var helpful: [BehaviorImpact] { impacts.filter { $0.delta > 0 } }
  var harmful: [BehaviorImpact] { impacts.filter { $0.delta < 0 }.reversed() }
}

enum InsightMetric: String, CaseIterable, Identifiable {
  case recovery
  case sleep
  var id: String { rawValue }
  var title: String { rawValue.capitalized }
}
