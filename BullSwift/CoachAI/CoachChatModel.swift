import Foundation

@MainActor
final class CoachChatModel: ObservableObject {
  @Published private(set) var isSignedIn = false
  @Published private(set) var loginStatus = "Not set up"
  @Published private(set) var modelPreset: CoachModelPreset
  @Published private(set) var messages: [CoachChatMessage] = []
  @Published private(set) var streamState: CoachStreamState = .idle
  @Published private(set) var errorMessage: String?
  @Published private(set) var needsConsent = false

  private static let modelPresetDefaultsKey = "bull.coach.modelPreset"
  private static let seedPromptText = "What should we look at today?"
  private var accessToken: String?
  private var sendTask: Task<Void, Never>?
  private let client = CoachAPIClient()

  init() {
    let storedRawValue = UserDefaults.standard.string(forKey: Self.modelPresetDefaultsKey)
    modelPreset = storedRawValue.flatMap(CoachModelPreset.init(rawValue:)) ?? .defaultValue
    messages = Self.normalizedPersistedMessages(CoachConversationStore.load())
    if !messages.isEmpty {
      persistConversation()
    }
    needsConsent = !CoachConsentStore.hasAccepted
    if let token = CoachAuthKeychain.load() {
      accessToken = token
      isSignedIn = true
      loginStatus = "Ready"
    }
  }

  deinit {
    sendTask?.cancel()
  }

  func refreshAuth() {
    if let token = CoachAuthKeychain.load() {
      accessToken = token
      isSignedIn = true
      loginStatus = "Ready"
      seedAssistantPromptIfNeeded()
    } else {
      accessToken = nil
      isSignedIn = false
      loginStatus = "Not set up"
    }
    needsConsent = !CoachConsentStore.hasAccepted
  }

  func acceptConsent() {
    CoachConsentStore.accept()
    needsConsent = false
  }

  func selectModelPreset(_ preset: CoachModelPreset) {
    modelPreset = preset
    UserDefaults.standard.set(preset.rawValue, forKey: Self.modelPresetDefaultsKey)
  }

  func startNewConversation() {
    sendTask?.cancel()
    sendTask = nil
    streamState = .idle
    errorMessage = nil
    messages.removeAll()
    CoachConversationStore.clear()
    seedAssistantPromptIfNeeded()
  }

  func setupCoach() {
    errorMessage = nil
    loginStatus = "Connecting"
    Task { [weak self] in
      guard let self else {
        return
      }
      do {
        let deviceID = UIDeviceIdentifier.coachDeviceID
        let token = try await client.fetchDevToken(deviceID: deviceID)
        try CoachAuthKeychain.save(token: token)
        accessToken = token
        isSignedIn = true
        loginStatus = "Ready"
        seedAssistantPromptIfNeeded()
      } catch {
        loginStatus = "Setup failed"
        errorMessage = describe(error)
      }
    }
  }

  func signOut() {
    sendTask?.cancel()
    sendTask = nil
    CoachAuthKeychain.delete()
    accessToken = nil
    isSignedIn = false
    loginStatus = "Not set up"
    streamState = .idle
    messages.removeAll()
    CoachConversationStore.clear()
    CoachConsentStore.reset()
    needsConsent = true
  }

  func cancelStreaming() {
    sendTask?.cancel()
    sendTask = nil
    streamState = .idle
    cancelStreamingMessages()
  }

  func send(
    _ prompt: String,
    healthStore: HealthDataStore,
    appModel: BullAppModel
  ) {
    let trimmedPrompt = prompt.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedPrompt.isEmpty, !streamState.isStreaming else {
      return
    }
    guard CoachConsentStore.hasAccepted else {
      needsConsent = true
      errorMessage = "Accept Coach data sharing to continue."
      return
    }
    guard let accessToken else {
      isSignedIn = false
      errorMessage = CoachAPIError.missingSession.localizedDescription
      return
    }

    let assistantID = UUID()
    let contextualPrompt = contextualPrompt(for: trimmedPrompt)
    messages.append(CoachChatMessage(role: .user, text: trimmedPrompt))
    messages.append(CoachChatMessage(id: assistantID, role: .assistant, text: "", isStreaming: true))
    streamState = .streaming
    errorMessage = nil
    persistConversation()

    sendTask?.cancel()
    sendTask = Task { [weak self] in
      guard let self else {
        return
      }
      do {
        try await streamResponseLoop(
          prompt: trimmedPrompt,
          contextualPrompt: contextualPrompt,
          accessToken: accessToken,
          assistantID: assistantID,
          healthStore: healthStore,
          appModel: appModel
        )
        finishAssistantMessage(assistantID)
        streamState = .idle
      } catch is CancellationError {
        markAssistantMessageCancelled(assistantID)
        streamState = .idle
      } catch where isCancelledError(error) {
        markAssistantMessageCancelled(assistantID)
        streamState = .idle
      } catch {
        let message = describe(error)
        appendAssistantText("\n\(message)", to: assistantID)
        finishAssistantMessage(assistantID)
        errorMessage = message
        streamState = .failed(message)
      }
    }
  }

  private func streamResponseLoop(
    prompt: String,
    contextualPrompt: String,
    accessToken: String,
    assistantID: UUID,
    healthStore: HealthDataStore,
    appModel: BullAppModel
  ) async throws {
    var conversationMessages = [CoachAPIRequestBuilder.message(role: "user", text: contextualPrompt)]
    var toolMode: CoachAPIRequestBuilder.ToolMode = .required

    for round in 0..<2 {
      var completedToolCalls: [CoachAIToolCall] = []
      var inFlightToolCalls: [String: CoachAIToolCall] = [:]

      let requestBody = CoachAPIRequestBuilder.makeBody(
        messages: conversationMessages,
        modelTier: modelPreset,
        toolMode: toolMode
      )

      try await client.stream(accessToken: accessToken, body: requestBody) { [weak self] event in
        guard let self else {
          return
        }
        try handle(
          event,
          assistantID: assistantID,
          inFlightToolCalls: &inFlightToolCalls,
          completedToolCalls: &completedToolCalls
        )
      }

      guard !completedToolCalls.isEmpty, round == 0 else {
        return
      }

      let toolSummary = completedToolCalls.map { call -> String in
        let output = CoachToolRegistry.execute(call: call, healthStore: healthStore, appModel: appModel)
        updateToolEvent(id: call.id, in: assistantID) { event in
          event.status = "Returned"
          event.resultSummary = summarizeToolOutput(output)
          event.name = CoachToolRegistry.displayTitle(for: call.name)
        }
        return "Tool \(call.name) output:\n\(output)"
      }.joined(separator: "\n\n")

      conversationMessages.append(
        CoachAPIRequestBuilder.message(
          role: "user",
          text: """
          \(toolSummary)

          Use the tool outputs above to answer this original Coach question now. Do not request more tools.

          Original question:
          \(prompt)
          """
        )
      )
      toolMode = .none
    }

    if isAssistantTextEmpty(assistantID) {
      throw CoachAPIError.api("Coach returned tool calls but no final reply.")
    }
  }

  private func handle(
    _ event: CoachAPIStreamEvent,
    assistantID: UUID,
    inFlightToolCalls: inout [String: CoachAIToolCall],
    completedToolCalls: inout [CoachAIToolCall]
  ) throws {
    switch event.type {
    case "response.created", "response.in_progress":
      break
    case "response.output_text.delta":
      if let delta = event.payload["delta"] as? String {
        appendAssistantText(delta, to: assistantID)
      }
    case "response.output_text.done":
      guard let text = event.payload["text"] as? String, isAssistantTextEmpty(assistantID) else {
        return
      }
      appendAssistantText(text, to: assistantID)
    case "response.output_item.added":
      guard let item = event.payload["item"] as? [String: Any],
            let call = toolCall(from: item, fallbackID: fallbackToolID(from: event.payload)) else {
        return
      }
      inFlightToolCalls[call.id] = call
      upsertToolEvent(
        CoachToolEvent(
          id: call.id,
          name: CoachToolRegistry.displayTitle(for: call.name),
          status: "Calling",
          arguments: "",
          resultSummary: nil
        ),
        in: assistantID
      )
    case "response.function_call_arguments.delta":
      let id = fallbackToolID(from: event.payload)
      guard let id, let delta = event.payload["delta"] as? String else {
        return
      }
      var call = inFlightToolCalls[id] ?? CoachAIToolCall(id: id, callID: id, name: "function", arguments: "")
      call.arguments += delta
      inFlightToolCalls[id] = call
    case "response.function_call_arguments.done", "response.output_item.done":
      completeToolCall(
        from: event.payload,
        assistantID: assistantID,
        inFlightToolCalls: &inFlightToolCalls,
        completedToolCalls: &completedToolCalls
      )
    case "response.completed":
      break
    case "response.failed", "error":
      throw CoachAPIError.api(errorMessage(from: event.payload))
    default:
      break
    }
  }

  private func completeToolCall(
    from payload: [String: Any],
    assistantID: UUID,
    inFlightToolCalls: inout [String: CoachAIToolCall],
    completedToolCalls: inout [CoachAIToolCall]
  ) {
    let fallbackID = fallbackToolID(from: payload)
    let finishedCall: CoachAIToolCall?
    if let item = payload["item"] as? [String: Any],
       let itemCall = toolCall(from: item, fallbackID: fallbackID) {
      finishedCall = itemCall
    } else if let fallbackID, var call = inFlightToolCalls[fallbackID] {
      if let arguments = payload["arguments"] as? String {
        call.arguments = arguments
      }
      finishedCall = call
    } else {
      finishedCall = nil
    }

    guard let finishedCall else {
      return
    }
    guard !completedToolCalls.contains(where: { $0.id == finishedCall.id || $0.callID == finishedCall.callID }) else {
      return
    }

    completedToolCalls.append(finishedCall)
    inFlightToolCalls[finishedCall.id] = finishedCall
    upsertToolEvent(
      CoachToolEvent(
        id: finishedCall.id,
        name: CoachToolRegistry.displayTitle(for: finishedCall.name),
        status: "Running",
        arguments: finishedCall.arguments,
        resultSummary: nil
      ),
      in: assistantID
    )
  }

  private func appendAssistantText(_ delta: String, to id: UUID) {
    guard let index = messages.firstIndex(where: { $0.id == id }) else {
      return
    }
    messages[index].text += delta
  }

  private func contextualPrompt(for prompt: String) -> String {
    let transcript = recentTranscriptContext(excludingCurrentPrompt: prompt)
    guard !transcript.isEmpty else {
      return prompt
    }
    return """
    Recent Coach conversation context:
    \(transcript)

    Current user message:
    \(prompt)
    """
  }

  private func recentTranscriptContext(excludingCurrentPrompt prompt: String) -> String {
    let turns = messages.compactMap { message -> String? in
      guard !message.isStreaming, !message.isCancelled else {
        return nil
      }
      let text = message.text.trimmingCharacters(in: .whitespacesAndNewlines)
      guard !text.isEmpty, text != Self.seedPromptText else {
        return nil
      }
      if message.role == .user, text == prompt {
        return nil
      }
      switch message.role {
      case .user:
        return "User: \(text)"
      case .assistant:
        return "Coach: \(text)"
      }
    }
    return boundedContext(from: turns.suffix(12), maxCharacters: 6_000)
  }

  private func boundedContext<S: Sequence>(from turns: S, maxCharacters: Int) -> String where S.Element == String {
    var selected: [String] = []
    var count = 0
    for turn in Array(turns).reversed() {
      let nextCount = count + turn.count + 2
      guard nextCount <= maxCharacters || selected.isEmpty else {
        break
      }
      selected.append(turn)
      count = nextCount
    }
    return selected.reversed().joined(separator: "\n\n")
  }

  private func isAssistantTextEmpty(_ id: UUID) -> Bool {
    guard let message = messages.first(where: { $0.id == id }) else {
      return true
    }
    return message.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
  }

  private func upsertToolEvent(_ event: CoachToolEvent, in messageID: UUID) {
    guard let messageIndex = messages.firstIndex(where: { $0.id == messageID }) else {
      return
    }
    if let eventIndex = messages[messageIndex].toolEvents.firstIndex(where: { $0.id == event.id }) {
      messages[messageIndex].toolEvents[eventIndex] = event
    } else {
      messages[messageIndex].toolEvents.append(event)
    }
  }

  private func updateToolEvent(
    id: String,
    in messageID: UUID,
    update: (inout CoachToolEvent) -> Void
  ) {
    guard let messageIndex = messages.firstIndex(where: { $0.id == messageID }) else {
      return
    }
    guard let eventIndex = messages[messageIndex].toolEvents.firstIndex(where: { $0.id == id }) else {
      return
    }
    update(&messages[messageIndex].toolEvents[eventIndex])
  }

  private func finishAssistantMessage(_ id: UUID) {
    guard let index = messages.firstIndex(where: { $0.id == id }) else {
      return
    }
    messages[index].isStreaming = false
    if messages[index].text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
       messages[index].toolEvents.isEmpty,
       !messages[index].isCancelled {
      messages.remove(at: index)
    }
    persistConversation()
  }

  private func markAssistantMessageCancelled(_ id: UUID) {
    guard let index = messages.firstIndex(where: { $0.id == id }) else {
      return
    }
    messages[index].isStreaming = false
    messages[index].isCancelled = true
    markUnfinishedToolEventsStopped(in: index)
    persistConversation()
  }

  private func cancelStreamingMessages() {
    for index in messages.indices {
      guard messages[index].isStreaming else {
        continue
      }
      messages[index].isStreaming = false
      if messages[index].role == .assistant {
        messages[index].isCancelled = true
        markUnfinishedToolEventsStopped(in: index)
      }
    }
    persistConversation()
  }

  private func markUnfinishedToolEventsStopped(in messageIndex: Int) {
    for eventIndex in messages[messageIndex].toolEvents.indices {
      if messages[messageIndex].toolEvents[eventIndex].status != "Returned" {
        messages[messageIndex].toolEvents[eventIndex].status = "Stopped"
      }
    }
  }

  private func seedAssistantPromptIfNeeded() {
    guard messages.isEmpty else {
      return
    }
    messages.append(
      CoachChatMessage(
        role: .assistant,
        text: Self.seedPromptText
      )
    )
    persistConversation()
  }

  private func toolCall(from item: [String: Any], fallbackID: String?) -> CoachAIToolCall? {
    let itemID = item["id"] as? String ?? fallbackID
    let callID = item["call_id"] as? String ?? itemID
    let name = item["name"] as? String ?? (item["function"] as? [String: Any])?["name"] as? String
    let arguments = item["arguments"] as? String ?? (item["function"] as? [String: Any])?["arguments"] as? String ?? ""
    guard let itemID, let callID, let name else {
      return nil
    }
    return CoachAIToolCall(id: itemID, callID: callID, name: name, arguments: arguments)
  }

  private func fallbackToolID(from payload: [String: Any]) -> String? {
    payload["item_id"] as? String ??
      payload["call_id"] as? String ??
      (payload["output_index"] as? Int).map { "tool-\($0)" }
  }

  private func errorMessage(from payload: [String: Any]) -> String {
    if let error = payload["error"] as? [String: Any] {
      return error["message"] as? String ?? "\(error)"
    }
    return payload["message"] as? String ?? "Coach stream failed."
  }

  private func summarizeToolOutput(_ output: String) -> String {
    let compact = output
      .replacingOccurrences(of: "\n", with: " ")
      .replacingOccurrences(of: "  ", with: " ")
    return String(compact.prefix(180))
  }

  private func persistConversation() {
    CoachConversationStore.save(messages)
  }

  private func describe(_ error: Error) -> String {
    if isCancelledError(error) {
      return "Generation stopped."
    }
    if let localizedError = error as? LocalizedError, let description = localizedError.errorDescription {
      return description
    }
    return String(describing: error)
  }

  private func isCancelledError(_ error: Error) -> Bool {
    if let urlError = error as? URLError {
      return urlError.code == .cancelled
    }
    let nsError = error as NSError
    return nsError.domain == NSURLErrorDomain && nsError.code == NSURLErrorCancelled
  }

  private static func normalizedPersistedMessages(_ storedMessages: [CoachChatMessage]) -> [CoachChatMessage] {
    storedMessages.map { message in
      var normalized = message
      if normalized.isStreaming {
        normalized.isStreaming = false
        normalized.isCancelled = true
      }
      if normalized.isCancelled {
        for index in normalized.toolEvents.indices where normalized.toolEvents[index].status != "Returned" {
          normalized.toolEvents[index].status = "Stopped"
        }
      }
      return normalized
    }
  }
}

enum UIDeviceIdentifier {
  static var coachDeviceID: String {
    let key = "bull.coach.device_id"
    if let existing = UserDefaults.standard.string(forKey: key), existing.count >= 8 {
      return existing
    }
    let id = UUID().uuidString
    UserDefaults.standard.set(id, forKey: key)
    return id
  }
}