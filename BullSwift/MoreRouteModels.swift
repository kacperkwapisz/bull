import SwiftUI

enum MoreRoute: String, CaseIterable, Identifiable, Hashable {
  case profile
  case device
  case connectionLab
  case capture
  case localStore
  case healthSync
  case rawExport
  case algorithms
  case debug
  case privacy
  case support
  case about
  case developer

  var id: String { rawValue }

  var title: String {
    switch self {
    case .profile: "Profile"
    case .device: "Device"
    case .connectionLab: "Connection Lab"
    case .capture: "Capture"
    case .localStore: "Local Store"
    case .healthSync: "Apple Health Profile"
    case .rawExport: "Raw Export"
    case .algorithms: "Algorithms"
    case .debug: "Debug"
    case .privacy: "Privacy"
    case .support: "Support"
    case .about: "About"
    case .developer: "Developer"
    }
  }

  var subtitle: String {
    switch self {
    case .profile: "Name, birthday, height, weight, and profile basics"
    case .device: "WHOOP band, connection, battery, and pairing"
    case .connectionLab: "Low-level Bluetooth, hello, and event diagnostics"
    case .capture: "Notification capture, imports, and command evidence"
    case .localStore: "SQLite path, schema, and storage health"
    case .healthSync: "Profile weight autofill only"
    case .rawExport: "Bundle windows, data scopes, validation, and lint"
    case .algorithms: "Operational algorithm preferences"
    case .debug: "Rust, parser, command groups, and gated controls"
    case .privacy: "Local data, export, lint, and deletion state"
    case .support: "Logs, support bundles, and troubleshooting"
    case .about: "App, Rust core, and licenses"
    case .developer: "Capture, exports, bridge diagnostics, and debug tools"
    }
  }

  var systemImage: String {
    switch self {
    case .profile: "person.crop.circle"
    case .device: "sensor.tag.radiowaves.forward"
    case .connectionLab: "antenna.radiowaves.left.and.right"
    case .capture: "record.circle"
    case .localStore: "externaldrive"
    case .healthSync: "heart.text.square"
    case .rawExport: "square.and.arrow.up"
    case .algorithms: "function"
    case .debug: "terminal"
    case .privacy: "hand.raised"
    case .support: "lifepreserver"
    case .about: "info.circle"
    case .developer: "hammer"
    }
  }

  var statusKeyPath: KeyPath<MoreRouteStatus, MoreStatusKind> {
    switch self {
    case .profile: \.profile
    case .device: \.device
    case .connectionLab: \.connectionLab
    case .capture: \.capture
    case .localStore: \.localStore
    case .healthSync: \.healthSync
    case .rawExport: \.rawExport
    case .algorithms: \.algorithms
    case .debug: \.debug
    case .privacy: \.privacy
    case .support: \.support
    case .about: \.about
    case .developer: \.developer
    }
  }

  static let deviceRoutes: [MoreRoute] = [.device]
  static let appRoutes: [MoreRoute] = [.healthSync]
  static let settingsRoutes: [MoreRoute] = [.privacy]
  static let supportRoutes: [MoreRoute] = [.support, .about]
  static let developerRoutes: [MoreRoute] = [.developer]
  static let developerToolRoutes: [MoreRoute] = [.connectionLab, .capture, .localStore, .rawExport, .algorithms, .debug]
}

struct MoreRouteStatus {
  var profile: MoreStatusKind
  var device: MoreStatusKind
  var connectionLab: MoreStatusKind
  var capture: MoreStatusKind
  var localStore: MoreStatusKind
  var healthSync: MoreStatusKind
  var rawExport: MoreStatusKind
  var algorithms: MoreStatusKind
  var debug: MoreStatusKind
  var privacy: MoreStatusKind
  var support: MoreStatusKind
  var about: MoreStatusKind
  var developer: MoreStatusKind
}

enum MoreStatusKind: String, CaseIterable {
  case ready
  case pending
  case blocked
  case unavailable
  case stale

  var title: String {
    rawValue.capitalized
  }

  var tint: Color {
    switch self {
    case .ready: .green
    case .pending: .blue
    case .blocked: .orange
    case .unavailable: .gray
    case .stale: .yellow
    }
  }

  var systemImage: String {
    switch self {
    case .ready: "checkmark.circle.fill"
    case .pending: "clock.fill"
    case .blocked: "exclamationmark.triangle.fill"
    case .unavailable: "minus.circle.fill"
    case .stale: "arrow.clockwise.circle.fill"
    }
  }
}

