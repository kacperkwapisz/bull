import Foundation

@MainActor
final class AppRouter: ObservableObject {
  @Published var selectedTab: BullAppTab = .home
  @Published var healthPath: [HealthRoute] = []
  @Published var morePath: [MoreRoute] = []
  @Published var codexAuthCallbackURL: URL?
  @Published var codexEmbeddedLoginRequestID = 0
  @Published var coachPromptDraft = ""
  @Published var coachPromptRequestID = 0
  @Published var coachScrollToBottomRequestID = 0

  func openHealth(_ route: HealthRoute?) {
    selectedTab = .health
    if let route {
      healthPath = [route]
    } else {
      healthPath = []
    }
  }

  func openCoach(prompt: String? = nil) {
    selectedTab = .coach
    guard let prompt else {
      return
    }
    let trimmedPrompt = prompt.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedPrompt.isEmpty else {
      return
    }
    coachPromptDraft = trimmedPrompt
    coachPromptRequestID += 1
  }

  func openMore(_ route: MoreRoute?) {
    selectedTab = .more
    if let route {
      morePath = [route]
    } else {
      morePath = []
    }
  }

  func reselect(_ tab: BullAppTab) {
    switch tab {
    case .coach:
      coachScrollToBottomRequestID += 1
    default:
      break
    }
  }

  @discardableResult
  func handleDeepLink(_ url: URL) -> Bool {
    if isCodexAuthCallback(url) {
      selectedTab = .coach
      codexAuthCallbackURL = url
      return true
    }

    if url.scheme == "bullswift", url.host == "coach" {
      selectedTab = .coach
      if url.pathComponents.dropFirst().first == "embedded-login" {
        codexEmbeddedLoginRequestID += 1
      }
      return true
    }

    if url.scheme == "bullswift", url.host == "more" {
      let routeName = url.pathComponents.dropFirst().first ?? ""
      if routeName.isEmpty {
        openMore(nil)
        return true
      }
      guard let route = MoreRoute(rawValue: routeName) else {
        return false
      }
      openMore(route)
      return true
    }

    guard url.scheme == "bullswift", url.host == "health" else {
      return false
    }
    let routeName = url.pathComponents.dropFirst().first ?? ""
    if routeName.isEmpty {
      openHealth(nil)
      return true
    }
    guard let route = HealthRoute(rawValue: routeName) else {
      return false
    }
    openHealth(route)
    return true
  }

  private func isCodexAuthCallback(_ url: URL) -> Bool {
    guard let scheme = url.scheme?.lowercased() else {
      return false
    }
    return ["bullswift", "bull"].contains(scheme) && url.host == "codex-auth"
  }
}
