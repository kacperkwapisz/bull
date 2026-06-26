import Foundation

struct BullSyncStatus: Decodable {
  struct Upload: Decodable {
    let id: String
    let deviceId: String?
    let status: String
    let createdAt: Date?
    let parsedAt: Date?
    let timeframeStart: Date?
    let timeframeEnd: Date?
    let parseError: String?
  }

  struct Run: Decodable, Identifiable {
    let id: String
    let deviceId: String?
    let source: String
    let triggerTimestamp: Date?
    let resultTimestamp: Date?
    let totalPacketUpload: Int
    let uploadRetryCount: Int
    let status: String
  }

  let lastSuccessfulUploadAt: Date?
  let serverCurrentThrough: Date?
  let highWatermark: Date?
  let latestUpload: Upload?
  let recentSyncRuns: [Run]

  enum CodingKeys: String, CodingKey {
    case lastSuccessfulUploadAt = "last_successful_upload_at"
    case serverCurrentThrough = "server_current_through"
    case highWatermark = "high_watermark"
    case latestUpload = "latest_upload"
    case recentSyncRuns = "recent_sync_runs"
  }
}

enum BullSyncStatusClient {
  static func fetch() async throws -> BullSyncStatus {
    guard let token = CoachAuthKeychain.load() else { throw CoachAuthError.invalidResponse }
    var request = URLRequest(url: CoachAPIConfiguration.dataSyncStatusURL)
    request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    let (data, response) = try await URLSession.shared.data(for: request)
    guard let http = response as? HTTPURLResponse else { throw CoachAuthError.invalidResponse }
    guard (200..<300).contains(http.statusCode) else {
      throw CoachAuthError.http(http.statusCode, String(data: data, encoding: .utf8) ?? "")
    }
    let decoder = JSONDecoder()
    decoder.dateDecodingStrategy = .iso8601
    return try decoder.decode(BullSyncStatus.self, from: data)
  }
}
