import SwiftUI
import UniformTypeIdentifiers
import UIKit

/// Native SwiftUI surface for exporting and restoring Bull's encrypted,
/// user-controlled local backups.
struct BullLocalBackupView: View {
  @State private var exportPassphrase = ""
  @State private var restorePassphrase = ""
  @State private var selectedRestoreURL: URL?
  @State private var shareItem: BullBackupShareItem?
  @State private var status: BackupStatus?
  @State private var activeOperation: BackupOperation?
  @State private var isShowingDocumentPicker = false
  @State private var isShowingRestorePrompt = false

  private var isWorking: Bool { activeOperation != nil }

  var body: some View {
    Form {
      Section {
        SecureField("Backup passphrase", text: $exportPassphrase)
          .textContentType(.newPassword)
          .disabled(isWorking)

        Text("Your backup is encrypted before sharing. Bull does not store or recover this passphrase.")
          .font(.footnote)
          .foregroundStyle(.secondary)
      } header: {
        Text("Encryption")
      }

      Section {
        Button {
          exportBackup()
        } label: {
          operationRow(
            title: "Export encrypted backup",
            systemImage: "square.and.arrow.up",
            operation: .export
          )
        }
        .disabled(isWorking || exportPassphrase.isEmpty)
      } footer: {
        Text("Exports the on-device SQLite history plus cached reports into one .bullbackup file for Files, iCloud Drive, or another place you control.")
      }

      Section {
        Button {
          isShowingDocumentPicker = true
        } label: {
          operationRow(
            title: "Restore from backup…",
            systemImage: "arrow.down.doc",
            operation: .restore
          )
        }
        .disabled(isWorking)
      } footer: {
        Text("Restore verifies the encrypted file first, then backs up the current local store before replacing it.")
      }

      if let status {
        Section {
          Label(status.message, systemImage: status.systemImage)
            .foregroundStyle(status.tint)
        }
      }
    }
    .navigationTitle("Encrypted Backup")
    .fileImporter(
      isPresented: $isShowingDocumentPicker,
      allowedContentTypes: [.bullBackup],
      allowsMultipleSelection: false
    ) { result in
      handleImportedBackup(result)
    }
    .alert("Restore Backup", isPresented: $isShowingRestorePrompt) {
      SecureField("Backup passphrase", text: $restorePassphrase)
        .textContentType(.password)
      Button("Cancel", role: .cancel) {
        restorePassphrase = ""
        selectedRestoreURL = nil
      }
      Button("Restore", role: .destructive) {
        restoreSelectedBackup()
      }
      .disabled(restorePassphrase.isEmpty)
    } message: {
      Text("Enter the passphrase used when this .bullbackup file was exported.")
    }
    .sheet(item: $shareItem) { item in
      BullBackupShareSheet(activityItems: [item.url])
    }
  }

  @ViewBuilder
  private func operationRow(title: String, systemImage: String, operation: BackupOperation) -> some View {
    HStack {
      Label(title, systemImage: systemImage)
      Spacer()
      if activeOperation == operation {
        ProgressView()
      }
    }
  }

  private func exportBackup() {
    let passphrase = exportPassphrase
    activeOperation = .export
    status = .working("Preparing encrypted backup…")

    Task {
      do {
        let url = try await Task.detached(priority: .userInitiated) {
          try BullLocalBackup.exportEncryptedBackup(passphrase: passphrase)
        }.value
        await MainActor.run {
          activeOperation = nil
          shareItem = BullBackupShareItem(url: url)
          status = .success("Encrypted backup is ready to share.")
        }
      } catch {
        await MainActor.run {
          activeOperation = nil
          status = .failure(error.localizedDescription)
        }
      }
    }
  }

  private func handleImportedBackup(_ result: Result<[URL], Error>) {
    switch result {
    case .success(let urls):
      guard let url = urls.first else {
        status = .failure("No backup file was selected.")
        return
      }
      selectedRestoreURL = url
      restorePassphrase = ""
      isShowingRestorePrompt = true
    case .failure(let error):
      status = .failure(error.localizedDescription)
    }
  }

  private func restoreSelectedBackup() {
    guard let url = selectedRestoreURL else {
      status = .failure("No backup file was selected.")
      return
    }
    let passphrase = restorePassphrase
    restorePassphrase = ""
    selectedRestoreURL = nil
    activeOperation = .restore
    status = .working("Verifying and restoring backup…")

    Task {
      do {
        try await Task.detached(priority: .userInitiated) {
          try BullLocalBackup.restoreEncryptedBackup(from: url, passphrase: passphrase)
        }.value
        await MainActor.run {
          activeOperation = nil
          status = .success("Backup restored. Restart Bull if any old values remain visible.")
        }
      } catch {
        await MainActor.run {
          activeOperation = nil
          status = .failure(error.localizedDescription)
        }
      }
    }
  }
}

private enum BackupOperation {
  case export
  case restore
}

private struct BackupStatus {
  let message: String
  let systemImage: String
  let tint: Color

  static func working(_ message: String) -> BackupStatus {
    BackupStatus(message: message, systemImage: "clock", tint: .secondary)
  }

  static func success(_ message: String) -> BackupStatus {
    BackupStatus(message: message, systemImage: "checkmark.circle.fill", tint: .green)
  }

  static func failure(_ message: String) -> BackupStatus {
    BackupStatus(message: message, systemImage: "exclamationmark.triangle.fill", tint: .red)
  }
}

private struct BullBackupShareItem: Identifiable {
  let id = UUID()
  let url: URL
}

private struct BullBackupShareSheet: UIViewControllerRepresentable {
  let activityItems: [Any]

  func makeUIViewController(context _: Context) -> UIActivityViewController {
    UIActivityViewController(activityItems: activityItems, applicationActivities: nil)
  }

  func updateUIViewController(_: UIActivityViewController, context _: Context) {}
}

private extension UTType {
  static var bullBackup: UTType {
    UTType(filenameExtension: "bullbackup") ?? .data
  }
}

#Preview {
  NavigationStack {
    BullLocalBackupView()
  }
}
