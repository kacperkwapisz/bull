import Foundation

@MainActor
enum CoachToolRegistry {
  static func execute(
    call: CoachAIToolCall,
    healthStore: HealthDataStore,
    appModel: BullAppModel
  ) -> String {
    let payload = CoachLocalToolContext.build(healthStore: healthStore, appModel: appModel)
    let tools = payload["tools"] as? [String: Any] ?? [:]
    let output: Any

    switch call.name {
    case "load_stats", "get_activities", "get_capture_sessions", "get_raw_session_data":
      output = tools[call.name] ?? ["error": "tool_not_available", "tool": call.name]
    case "get_data_gaps":
      output = [
        "readiness": healthStore.metricInputReadinessSummary(),
        "input_next_action": healthStore.metricInputReadinessNextActionSummary(),
        "score_next_action": healthStore.packetDerivedScoreNextActionSummary(),
        "packet_inputs": healthStore.packetInputStatus,
        "packet_scores": healthStore.packetScoreStatus,
        "capture": tools["get_capture_sessions"] ?? [:],
      ]
    default:
      output = ["error": "unknown_tool", "tool": call.name]
    }

    return jsonString(output)
  }

  static func displayTitle(for name: String) -> String {
    switch name {
    case "load_stats":
      return "Checked your metrics"
    case "get_activities":
      return "Checked activities"
    case "get_capture_sessions":
      return "Checked capture"
    case "get_data_gaps":
      return "Checked data gaps"
    default:
      return "Checked \(name)"
    }
  }

  private static func jsonString(_ value: Any) -> String {
    guard JSONSerialization.isValidJSONObject(value),
          let data = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]),
          let string = String(data: data, encoding: .utf8) else {
      return "{\"error\":\"json_encoding_failed\"}"
    }
    return string
  }
}