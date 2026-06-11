import Foundation
import SwiftUI
import os

// MARK: - Performance instrumentation scaffolding
//
// Measurement-only. None of this changes app behavior.
//
// Two layers:
//   1. `OSSignposter` intervals for the suspected main-thread hot paths
//      (snapshot derivation, display-safe scan, live packet publish).
//      Signposts are effectively free unless Instruments is recording, so
//      they stay compiled in for Debug *and* Release. View them in
//      Instruments on the "os_signpost" track (subsystem com.bull.swift).
//
//   2. `Self.bullPrintChangesIfEnabled()` wraps SwiftUI's `_printChanges()`
//      so we can see *which* @Published property re-ran a view body. This is
//      DEBUG-only and gated behind a launch flag so normal runs stay quiet.
//
// Enable view-change logging with either:
//   - Scheme launch argument:  --bull-print-view-changes
//   - Environment variable:     BULL_PRINT_VIEW_CHANGES=1

// MARK: Signposters

enum BullSignpost {
  static let subsystem = "com.bull.swift"

  /// Main-thread UI derivation (snapshot building, display-safe scan, view-model assembly).
  static let ui = OSSignposter(subsystem: subsystem, category: "ui")

  /// Live BLE -> parse -> publish pipeline work that lands on the main actor.
  static let pipeline = OSSignposter(subsystem: subsystem, category: "pipeline")

  /// Rust FFI / packet-input extraction boundaries.
  static let bridge = OSSignposter(subsystem: subsystem, category: "bridge")
}

// MARK: Interval helpers (defer-friendly)

struct BullSignpostToken {
  let signposter: OSSignposter
  let name: StaticString
  let state: OSSignpostIntervalState
}

/// Begin a signpost interval. Pair with `bullSignpostEnd`, typically via `defer`:
///
///     let token = bullSignpostBegin(BullSignpost.ui, "landingSnapshots")
///     defer { bullSignpostEnd(token) }
@inline(__always)
func bullSignpostBegin(_ signposter: OSSignposter, _ name: StaticString) -> BullSignpostToken {
  BullSignpostToken(signposter: signposter, name: name, state: signposter.beginInterval(name))
}

@inline(__always)
func bullSignpostEnd(_ token: BullSignpostToken) {
  token.signposter.endInterval(token.name, token.state)
}

/// Measure a synchronous closure as a signpost interval and return its result.
@inline(__always)
func bullSignpostMeasure<T>(
  _ signposter: OSSignposter,
  _ name: StaticString,
  _ work: () -> T
) -> T {
  let state = signposter.beginInterval(name)
  defer { signposter.endInterval(name, state) }
  return work()
}

// MARK: View-change logging flags

enum BullPerfFlags {
  /// True when `--bull-print-view-changes` is passed or `BULL_PRINT_VIEW_CHANGES=1` is set.
  static let printViewChanges: Bool = {
    let processInfo = ProcessInfo.processInfo
    return processInfo.arguments.contains("--bull-print-view-changes")
      || processInfo.environment["BULL_PRINT_VIEW_CHANGES"] == "1"
  }()
}

extension View {
  /// Logs *why* this view's `body` re-evaluated (which dependency changed),
  /// but only in DEBUG builds with the launch flag enabled. No-op otherwise.
  ///
  /// Call as a leading statement in `body`:
  ///
  ///     var body: some View {
  ///       let _ = Self.bullPrintChangesIfEnabled()
  ///       ...
  ///     }
  @inline(__always)
  static func bullPrintChangesIfEnabled() {
    #if DEBUG
    if BullPerfFlags.printViewChanges {
      Self._printChanges()
    }
    #endif
  }
}
