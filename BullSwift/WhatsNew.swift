import SwiftUI

// MARK: - Changelog model

/// One changelog release shown in the "What's New" dialog. Bump `id` for every
/// new entry — the highest dismissed `id` is persisted, so adding a higher id
/// makes the dialog reappear once on next launch.
struct ChangelogEntry: Identifiable {
  let id: Int
  let title: String
  let date: String
  let highlights: [ChangelogHighlight]
}

struct ChangelogHighlight: Identifiable {
  let id = UUID()
  let icon: String
  let tint: Color
  let title: String
  let detail: String
}

/// Author the changelog here. Newest entry first; give it the highest `id`.
enum Changelog {
  static let entries: [ChangelogEntry] = [
    ChangelogEntry(
      id: 2,
      title: "Engine, Refined",
      date: "June 2026",
      highlights: [
        ChangelogHighlight(
          icon: "waveform.path.ecg",
          tint: Color(red: 0.20, green: 0.68, blue: 0.27),
          title: "More accurate HRV",
          detail: "Heart-rate variability now segments around recording gaps and rejects ectopic beats before scoring, for a steadier, more trustworthy nightly reading."
        ),
        ChangelogHighlight(
          icon: "slider.horizontal.3",
          tint: Color(red: 0.55, green: 0.40, blue: 0.95),
          title: "Biometric Engine preview",
          detail: "Open More → Biometric Engine to watch your personal baseline, Recovery v1 and Readiness build live from your band's own sensor data."
        ),
      ]
    ),
    ChangelogEntry(
      id: 1,
      title: "Biometric Engine",
      date: "June 2026",
      highlights: [
        ChangelogHighlight(
          icon: "waveform.path.ecg.rectangle",
          tint: Color(red: 0.20, green: 0.68, blue: 0.27),
          title: "Personal recovery baseline",
          detail: "Recovery now learns your own 14-night HRV and resting-HR baseline and scores each day against it, with calibrating, provisional and trusted confidence levels."
        ),
        ChangelogHighlight(
          icon: "bolt.heart",
          tint: Color(red: 0.95, green: 0.55, blue: 0.10),
          title: "Readiness engine",
          detail: "A new readiness signal weighs your recent training load against your longer-term load to tell you when you're primed, balanced or run down."
        ),
        ChangelogHighlight(
          icon: "bed.double.fill",
          tint: Color(red: 0.40, green: 0.45, blue: 0.95),
          title: "Staged sleep",
          detail: "Sleep is now broken into Wake, Light, Deep and REM with time-in-stage, efficiency and onset metrics, shown honestly as uncalibrated until validated."
        ),
        ChangelogHighlight(
          icon: "figure.run",
          tint: Color(red: 0.90, green: 0.30, blue: 0.45),
          title: "Smarter strain & calories",
          detail: "Strain adds a heart-rate-reserve TRIMP model, and calories use your height, weight, age and sex for a more personal estimate."
        ),
        ChangelogHighlight(
          icon: "dumbbell.fill",
          tint: Color(red: 0.18, green: 0.48, blue: 0.95),
          title: "Automatic workout detection",
          detail: "Bursts of elevated heart rate and motion are detected as exercise sessions automatically, with per-session strain and calories."
        ),
        ChangelogHighlight(
          icon: "drop.fill",
          tint: Color(red: 0.30, green: 0.70, blue: 0.85),
          title: "Blood oxygen, skin temp & breathing",
          detail: "Bull now decodes SpO₂, skin temperature and respiration straight from your band's own sensor data, with implausible readings rejected."
        ),
        ChangelogHighlight(
          icon: "bubble.left.and.text.bubble.right.fill",
          tint: Color(red: 0.55, green: 0.40, blue: 0.95),
          title: "A sharper Coach",
          detail: "The Coach can now use your live metrics as tools while it answers, for grounded, data-aware guidance."
        ),
      ]
    )
  ]

  static var latestID: Int { entries.map(\.id).max() ?? 0 }
}

// MARK: - Seen-state store

/// Tracks which changelog entries the user has already dismissed. Persists the
/// highest dismissed id so newly authored entries surface exactly once.
final class ChangelogStore: ObservableObject {
  private static let storageKey = "bull.changelog.lastSeenID.v1"

  @Published private(set) var unseen: [ChangelogEntry]

  init(defaults: UserDefaults = .standard) {
    let lastSeen = defaults.integer(forKey: Self.storageKey)
    unseen = Changelog.entries
      .filter { $0.id > lastSeen }
      .sorted { $0.id > $1.id }
  }

  var hasUnseen: Bool { !unseen.isEmpty }

  func markAllSeen(defaults: UserDefaults = .standard) {
    defaults.set(Changelog.latestID, forKey: Self.storageKey)
    unseen = []
  }
}

// MARK: - What's New dialog

struct WhatsNewView: View {
  let entries: [ChangelogEntry]
  var onClose: () -> Void

  @Environment(\.dismiss) private var dismiss

  private var highlights: [ChangelogHighlight] {
    entries.flatMap(\.highlights)
  }

  private var headerSubtitle: String {
    if let first = entries.first {
      return entries.count == 1 ? first.date : "\(entries.count) updates"
    }
    return ""
  }

  var body: some View {
    VStack(spacing: 0) {
      Capsule()
        .fill(Color.secondary.opacity(0.4))
        .frame(width: 38, height: 5)
        .padding(.top, 10)
        .padding(.bottom, 6)

      ScrollView {
        VStack(alignment: .leading, spacing: 22) {
          VStack(alignment: .leading, spacing: 6) {
            Text("What's New")
              .font(.system(size: 30, weight: .heavy, design: .rounded))
              .foregroundStyle(.primary)
            if !headerSubtitle.isEmpty {
              Text(headerSubtitle)
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(.secondary)
            }
          }
          .padding(.top, 8)

          VStack(spacing: 12) {
            ForEach(highlights) { highlight in
              ChangelogHighlightRow(highlight: highlight)
            }
          }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 20)
        .padding(.bottom, 24)
      }

      VStack(spacing: 0) {
        Button(action: close) {
          Text("Got it")
            .font(.system(size: 17, weight: .bold))
            .foregroundStyle(.white)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 15)
            .background(
              Color.accentColor,
              in: RoundedRectangle(cornerRadius: 16, style: .continuous)
            )
        }
        .buttonStyle(.plain)
        .padding(.horizontal, 20)
        .padding(.top, 10)
        .padding(.bottom, 16)
      }
      .background(.ultraThinMaterial)
    }
    .bullScreenBackground()
    .presentationDragIndicator(.hidden)
    .interactiveDismissDisabled(false)
  }

  private func close() {
    onClose()
    dismiss()
  }
}

private struct ChangelogHighlightRow: View {
  let highlight: ChangelogHighlight

  var body: some View {
    HStack(alignment: .top, spacing: 14) {
      Image(systemName: highlight.icon)
        .font(.system(size: 18, weight: .semibold))
        .foregroundStyle(highlight.tint)
        .frame(width: 44, height: 44)
        .background(
          highlight.tint.opacity(0.16),
          in: RoundedRectangle(cornerRadius: 12, style: .continuous)
        )

      VStack(alignment: .leading, spacing: 3) {
        Text(highlight.title)
          .font(.system(size: 16, weight: .bold))
          .foregroundStyle(.primary)
        Text(highlight.detail)
          .font(.system(size: 14, weight: .regular))
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
      }

      Spacer(minLength: 0)
    }
    .padding(14)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(
      BullTheme.plainBackground,
      in: RoundedRectangle(cornerRadius: 16, style: .continuous)
    )
    .overlay(
      RoundedRectangle(cornerRadius: 16, style: .continuous)
        .strokeBorder(Color.primary.opacity(0.06), lineWidth: 1)
    )
  }
}
