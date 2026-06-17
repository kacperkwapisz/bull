import Foundation

struct CoachAIToolCall: Equatable {
  let id: String
  let callID: String
  let name: String
  var arguments: String
}

struct CoachAPIStreamEvent {
  let type: String
  let payload: [String: Any]
}

enum CoachAPIError: Error, LocalizedError {
  case missingSession
  case invalidURL
  case invalidBody
  case invalidResponse
  case httpStatus(Int, String)
  case api(String)

  var errorDescription: String? {
    switch self {
    case .missingSession:
      return "Set up Coach first."
    case .invalidURL:
      return "Coach API URL is invalid."
    case .invalidBody:
      return "Coach request could not be encoded."
    case .invalidResponse:
      return "Coach returned an invalid streaming response."
    case .httpStatus(let status, let body):
      return body.isEmpty ? "Coach request failed with HTTP \(status)." : "Coach request failed (HTTP \(status)): \(body)"
    case .api(let message):
      return message
    }
  }
}

enum CoachAPIRequestBuilder {
  static func makeBody(
    messages: [[String: Any]],
    modelTier: CoachModelPreset,
    toolMode: ToolMode
  ) -> [String: Any] {
    var body: [String: Any] = [
      "model_tier": modelTier.apiTier,
      "messages": messages,
    ]
    switch toolMode {
    case .required:
      body["tool_mode"] = "required"
    case .auto:
      body["tool_mode"] = "auto"
    case .none:
      body["tool_mode"] = "none"
    }
    return body
  }

  enum ToolMode {
    case required
    case auto
    case none
  }

  static func message(role: String, text: String) -> [String: Any] {
    ["role": role, "content": text]
  }

  /// Assistant turn that requested tool calls, in OpenAI multi-turn shape.
  static func assistantToolCallMessage(_ calls: [CoachAIToolCall]) -> [String: Any] {
    [
      "role": "assistant",
      "content": "",
      "tool_calls": calls.map { call -> [String: Any] in
        [
          "id": call.callID,
          "type": "function",
          "function": [
            "name": call.name,
            "arguments": call.arguments.isEmpty ? "{}" : call.arguments,
          ],
        ]
      },
    ]
  }

  /// Tool result message correlated back to the originating call.
  static func toolResultMessage(callID: String, output: String) -> [String: Any] {
    ["role": "tool", "tool_call_id": callID, "content": output]
  }
}

struct BullAppleSession {
  let accessToken: String
  let userID: String
  let isNewUser: Bool
}

struct CoachAPIClient {
  /// Exchange a device-issued Apple identity token for a BullAPI session JWT.
  func exchangeAppleIdentityToken(_ identityToken: String, deviceID: String?) async throws -> BullAppleSession {
    var request = URLRequest(url: CoachAPIConfiguration.appleAuthURL)
    request.httpMethod = "POST"
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    var payload: [String: Any] = ["identity_token": identityToken]
    if let deviceID, !deviceID.isEmpty {
      payload["device_id"] = deviceID
    }
    request.httpBody = try JSONSerialization.data(withJSONObject: payload)
    let (data, response) = try await URLSession.shared.data(for: request)
    guard let http = response as? HTTPURLResponse else {
      throw CoachAuthError.invalidResponse
    }
    guard (200..<300).contains(http.statusCode) else {
      throw CoachAuthError.http(http.statusCode, String(data: data, encoding: .utf8) ?? "")
    }
    guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
          let token = json["access_token"] as? String,
          let userID = json["user_id"] as? String else {
      throw CoachAuthError.invalidResponse
    }
    return BullAppleSession(
      accessToken: token,
      userID: userID,
      isNewUser: json["is_new_user"] as? Bool ?? false
    )
  }

  /// Push curated daily metrics computed on-device to the long-term store.
  /// `body` must match BullAPI's `POST /v1/data/metrics` schema (family arrays
  /// keyed by day). Returns the server's `ingested` counts. Every value
  /// originates from the connected device's own sensors.
  @discardableResult
  /// Upload the connected user's profile + device timezone so server-side
  /// compute can derive energy and bucket daily rollups on the local day.
  func pushProfile(body: [String: Any], token: String) async throws {
    guard JSONSerialization.isValidJSONObject(body) else {
      throw CoachAPIError.invalidBody
    }
    var request = URLRequest(url: CoachAPIConfiguration.dataProfileURL)
    request.httpMethod = "POST"
    request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    request.httpBody = try JSONSerialization.data(withJSONObject: body)
    request.timeoutInterval = 30
    let (data, response) = try await URLSession.shared.data(for: request)
    guard let http = response as? HTTPURLResponse else {
      throw CoachAPIError.invalidResponse
    }
    guard (200..<300).contains(http.statusCode) else {
      throw CoachAPIError.httpStatus(http.statusCode, String(data: data, encoding: .utf8) ?? "")
    }
  }

  func pushDailyMetrics(body: [String: Any], token: String) async throws -> [String: Any] {
    guard JSONSerialization.isValidJSONObject(body) else {
      throw CoachAPIError.invalidBody
    }
    var request = URLRequest(url: CoachAPIConfiguration.dataMetricsURL)
    request.httpMethod = "POST"
    request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    request.httpBody = try JSONSerialization.data(withJSONObject: body)
    request.timeoutInterval = 60
    let (data, response) = try await URLSession.shared.data(for: request)
    guard let http = response as? HTTPURLResponse else {
      throw CoachAPIError.invalidResponse
    }
    guard (200..<300).contains(http.statusCode) else {
      throw CoachAPIError.httpStatus(http.statusCode, String(data: data, encoding: .utf8) ?? "")
    }
    return (try? JSONSerialization.jsonObject(with: data) as? [String: Any]) ?? [:]
  }

  /// Fetch curated metric history for restore-on-reinstall. Returns the parsed
  /// family arrays (recovery/sleep/strain/stress/energy/vitals) the local Rust
  /// core re-imports to hydrate its `daily_*` tables.
  func fetchMetricHistory(from: String?, to: String?, token: String) async throws -> [String: Any] {
    var components = URLComponents(
      url: CoachAPIConfiguration.dataMetricsURL,
      resolvingAgainstBaseURL: false
    )
    var items: [URLQueryItem] = []
    if let from, !from.isEmpty {
      items.append(URLQueryItem(name: "from", value: from))
    }
    if let to, !to.isEmpty {
      items.append(URLQueryItem(name: "to", value: to))
    }
    if !items.isEmpty {
      components?.queryItems = items
    }
    guard let url = components?.url else {
      throw CoachAPIError.invalidURL
    }
    var request = URLRequest(url: url)
    request.httpMethod = "GET"
    request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    request.timeoutInterval = 60
    let (data, response) = try await URLSession.shared.data(for: request)
    guard let http = response as? HTTPURLResponse else {
      throw CoachAPIError.invalidResponse
    }
    guard (200..<300).contains(http.statusCode) else {
      throw CoachAPIError.httpStatus(http.statusCode, String(data: data, encoding: .utf8) ?? "")
    }
    return (try? JSONSerialization.jsonObject(with: data) as? [String: Any]) ?? [:]
  }

  func stream(
    accessToken: String,
    body: [String: Any],
    onEvent: @MainActor @escaping (CoachAPIStreamEvent) throws -> Void
  ) async throws {
    let endpoint = CoachAPIConfiguration.responsesURL
    guard JSONSerialization.isValidJSONObject(body) else {
      throw CoachAPIError.invalidBody
    }
    let bodyData = try JSONSerialization.data(withJSONObject: body)

    var request = URLRequest(url: endpoint)
    request.httpMethod = "POST"
    request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    request.setValue("text/event-stream", forHTTPHeaderField: "Accept")
    request.httpBody = bodyData
    request.timeoutInterval = 180

    let (bytes, response) = try await URLSession.shared.bytes(for: request)
    guard let httpResponse = response as? HTTPURLResponse else {
      throw CoachAPIError.invalidResponse
    }
    guard (200..<300).contains(httpResponse.statusCode) else {
      let bodyText = try await readErrorBody(from: bytes)
      throw CoachAPIError.httpStatus(httpResponse.statusCode, bodyText)
    }

    var dataLines: [String] = []
    for try await line in bytes.lines {
      try Task.checkCancellation()
      let trimmedLine = line.trimmingCharacters(in: .whitespacesAndNewlines)
      if trimmedLine.isEmpty {
        try await process(dataLines: dataLines, onEvent: onEvent)
        dataLines.removeAll()
      } else if trimmedLine.hasPrefix("data:") {
        let value = String(trimmedLine.dropFirst(5)).trimmingCharacters(in: .whitespacesAndNewlines)
        dataLines.append(value)
      }
    }
    try await process(dataLines: dataLines, onEvent: onEvent)
  }

  private func process(
    dataLines: [String],
    onEvent: @MainActor @escaping (CoachAPIStreamEvent) throws -> Void
  ) async throws {
    guard !dataLines.isEmpty else {
      return
    }
    for dataText in dataLines.flatMap(Self.jsonPayloads(from:)) {
      guard dataText != "[DONE]",
            let data = dataText.data(using: .utf8),
            let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let type = object["type"] as? String else {
        continue
      }
      try await onEvent(CoachAPIStreamEvent(type: type, payload: object))
    }
  }

  private func readErrorBody(from bytes: URLSession.AsyncBytes) async throws -> String {
    var lines: [String] = []
    for try await line in bytes.lines {
      lines.append(line)
      if lines.joined().count > 4000 {
        break
      }
    }
    return lines.joined(separator: "\n")
  }

  private static func jsonPayloads(from dataLine: String) -> [String] {
    dataLine
      .split(whereSeparator: \.isNewline)
      .map { String($0).trimmingCharacters(in: .whitespacesAndNewlines) }
      .filter { !$0.isEmpty }
  }
}