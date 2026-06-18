import SwiftUI

/// Daily behavior log + "what's helping / hurting" insights, in one screen.
struct JournalView: View {
  @StateObject private var store = JournalStore()
  @State private var customDraft = ""

  var body: some View {
    Form {
      insightsSection
      logSection
      customSection
      noteSection
    }
    .navigationTitle("Journal")
    .navigationBarTitleDisplayMode(.inline)
    .toolbar {
      ToolbarItem(placement: .confirmationAction) {
        Button(store.isSaving ? "Saving…" : "Save") {
          Task { await store.save() }
        }
        .disabled(store.isSaving)
      }
    }
    .overlay {
      if store.isLoading && store.catalog.isEmpty {
        ProgressView()
      }
    }
    .task { await store.load() }
    .refreshable { await store.load() }
  }

  // MARK: - Insights

  private var insightsSection: some View {
    Section {
      Picker("Metric", selection: $store.metric) {
        ForEach(InsightMetric.allCases) { Text($0.title).tag($0) }
      }
      .pickerStyle(.segmented)

      if store.insightsUnavailable {
        Text("Insights are temporarily unavailable.")
          .foregroundStyle(.secondary)
          .font(.subheadline)
      } else if let insights = store.insights, !insights.impacts.isEmpty {
        if !insights.helpful.isEmpty {
          Text("Helps your \(insights.metric)")
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(.green)
          ForEach(insights.helpful) { impactRow($0) }
        }
        if !insights.harmful.isEmpty {
          Text("Hurts your \(insights.metric)")
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(.red)
          ForEach(insights.harmful) { impactRow($0) }
        }
        Text("Shows correlation, not causation — a habit you do on already-hard days can look harmful.")
          .font(.caption)
          .foregroundStyle(.secondary)
      } else {
        Text(emptyInsightsCopy)
          .foregroundStyle(.secondary)
          .font(.subheadline)
      }
    } header: {
      Text("What moves your \(store.metric.title.lowercased())")
    }
  }

  private var emptyInsightsCopy: String {
    let days = store.insights?.analyzedDays ?? 0
    if days == 0 {
      return "Log your behaviors for a couple of weeks to see what moves your \(store.metric.title.lowercased())."
    }
    return "Keep logging — a habit needs enough on and off days before its impact shows here."
  }

  private func impactRow(_ impact: BehaviorImpact) -> some View {
    let positive = impact.delta > 0
    let points = Int(impact.delta.rounded())
    return HStack {
      Text(label(for: impact.behavior))
      Spacer()
      Text(strengthLabel(impact.strength))
        .font(.caption2.weight(.semibold))
        .foregroundStyle(.secondary)
      Text("\(points > 0 ? "+" : "")\(points) pts")
        .font(.subheadline.weight(.semibold))
        .foregroundStyle(positive ? .green : .red)
        .monospacedDigit()
    }
  }

  private func strengthLabel(_ raw: String) -> String {
    switch raw {
    case "strong": return "STRONG"
    case "moderate": return "MODERATE"
    default: return "WEAK"
    }
  }

  // MARK: - Daily log

  private var logSection: some View {
    ForEach(store.allTagsByCategory, id: \.category) { group in
      Section(JournalCatalogTag.categoryTitle(group.category)) {
        ForEach(group.tags) { tag in
          Button {
            store.toggle(tag.tag)
          } label: {
            HStack {
              Text(tag.label)
                .foregroundStyle(.primary)
              Spacer()
              if store.selected.contains(tag.tag) {
                Image(systemName: "checkmark.circle.fill")
                  .foregroundStyle(.tint)
              } else {
                Image(systemName: "circle")
                  .foregroundStyle(.secondary)
              }
            }
          }
        }
      }
    }
  }

  private var customSection: some View {
    Section("Add your own") {
      HStack {
        TextField("Custom behavior", text: $customDraft)
          .textInputAutocapitalization(.never)
          .autocorrectionDisabled()
        Button("Add") {
          store.addCustomTag(customDraft)
          customDraft = ""
        }
        .disabled(customDraft.trimmingCharacters(in: .whitespaces).isEmpty)
      }
    }
  }

  private var noteSection: some View {
    Section("Note") {
      TextField("Anything notable about today", text: $store.note, axis: .vertical)
        .lineLimit(2...5)
      if let savedAt = store.savedAt {
        Text("Saved \(savedAt.formatted(date: .omitted, time: .shortened))")
          .font(.caption)
          .foregroundStyle(.secondary)
      }
      if let error = store.errorMessage {
        Text(error).font(.caption).foregroundStyle(.red)
      }
    }
  }

  private func label(for tag: String) -> String {
    if let match = store.catalog.first(where: { $0.tag == tag }) {
      return match.label
    }
    return tag
      .replacingOccurrences(of: "_", with: " ")
      .capitalized
  }
}
