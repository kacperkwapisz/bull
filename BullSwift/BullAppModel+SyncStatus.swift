import Foundation

extension BullAppModel {
  func refreshServerSyncStatus() {
    Task { [weak self] in
      guard let self else { return }
      do {
        let status = try await BullSyncStatusClient.fetch()
        await MainActor.run {
          self.syncStatus = status
          self.syncStatusSummary = Self.syncStatusSummary(status)
        }
      } catch {
        await MainActor.run {
          self.syncStatusSummary = "Sync status unavailable: \(error.localizedDescription)"
        }
      }
    }
  }

  private static func syncStatusSummary(_ status: BullSyncStatus) -> String {
    guard let last = status.lastSuccessfulUploadAt else {
      return "No server upload yet"
    }
    let age = Date().timeIntervalSince(last)
    let stale = age >= 3 * 60 * 60 ? "Data behind" : "Current"
    return "\(stale) | last upload \(relativeSyncAge(last))"
  }

  static func relativeSyncAge(_ date: Date) -> String {
    let formatter = RelativeDateTimeFormatter()
    formatter.unitsStyle = .abbreviated
    return formatter.localizedString(for: date, relativeTo: Date())
  }
}
