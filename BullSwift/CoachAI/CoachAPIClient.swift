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
    messages: [[String: String]],
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

  static func message(role: String, text: String) -> [String: String] {
    ["role": role, "content": text]
  }
}

struct CoachAPIClient {
  func fetchDevToken(deviceID: String?) async throws -> String {
    var request = URLRequest(url: CoachAPIConfiguration.devTokenURL)
    request.httpMethod = "POST"
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    var payload: [String: Any] = [:]
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
          let token = json["access_token"] as? String else {
      throw CoachAuthError.invalidResponse
    }
    return token
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