import Foundation

enum CoachStreamState: Equatable {
  case idle
  case streaming
  case failed(String)

  var isStreaming: Bool {
    if case .streaming = self {
      return true
    }
    return false
  }
}

struct CoachToolEvent: Identifiable, Equatable, Codable {
  let id: String
  var name: String
  var status: String
  var arguments: String
  var resultSummary: String?
}

struct CoachChatMessage: Identifiable, Equatable, Codable {
  enum Role: Equatable, Codable {
    case user
    case assistant
  }

  let id: UUID
  let role: Role
  var text: String
  var toolEvents: [CoachToolEvent]
  var isStreaming: Bool
  var isCancelled: Bool
  let createdAt: Date

  init(
    id: UUID = UUID(),
    role: Role,
    text: String,
    toolEvents: [CoachToolEvent] = [],
    isStreaming: Bool = false,
    isCancelled: Bool = false,
    createdAt: Date = Date()
  ) {
    self.id = id
    self.role = role
    self.text = text
    self.toolEvents = toolEvents
    self.isStreaming = isStreaming
    self.isCancelled = isCancelled
    self.createdAt = createdAt
  }
}

enum CoachModelPreset: String, CaseIterable, Identifiable {
  case coach
  case deeperInsight

  var id: String { rawValue }

  static let defaultValue: CoachModelPreset = .coach

  var title: String {
    switch self {
    case .coach:
      return "Coach"
    case .deeperInsight:
      return "Deeper insight"
    }
  }

  var apiTier: String {
    switch self {
    case .coach:
      return "default"
    case .deeperInsight:
      return "deep"
    }
  }
}

enum CoachConversationStore {
  private static let defaultsKey = "bull.coach.conversation.v1"
  private static let maxPersistedMessages = 80

  static func load() -> [CoachChatMessage] {
    guard let data = UserDefaults.standard.data(forKey: defaultsKey) else {
      return []
    }
    let decoder = JSONDecoder()
    decoder.dateDecodingStrategy = .iso8601
    return (try? decoder.decode([CoachChatMessage].self, from: data)) ?? []
  }

  static func save(_ messages: [CoachChatMessage]) {
    let encoder = JSONEncoder()
    encoder.dateEncodingStrategy = .iso8601
    let persisted = Array(messages.suffix(maxPersistedMessages))
    guard let data = try? encoder.encode(persisted) else {
      return
    }
    UserDefaults.standard.set(data, forKey: defaultsKey)
  }

  static func clear() {
    UserDefaults.standard.removeObject(forKey: defaultsKey)
  }
}
