import SwiftUI

struct CoachView: View {
  @EnvironmentObject private var model: BullAppModel
  @EnvironmentObject private var router: AppRouter
  @EnvironmentObject private var calibration: CalibrationManager
  @ObservedObject var healthStore: HealthDataStore
  @StateObject private var chat = CoachChatModel()
  @State private var promptDraft = ""
  @State private var appliedCoachPromptRequestID = 0
  @State private var showingChat = false
  // Cache the derived overview so the four snapshot(for:) builds + summary scans
  // only run when an input actually changes — not on every BullAppModel tick
  // (which previously rebuilt them ~10x/s even while Coach was off-screen).
  @State private var cachedCoachSnapshot: CoachOverviewSnapshot?

  var body: some View {
    let _ = Self.bullPrintChangesIfEnabled()
    CoachOverviewScreen(
      snapshot: cachedCoachSnapshot ?? .placeholder,
      chatIsSignedIn: chat.isSignedIn,
      chatStatus: chatStatus,
      openChat: { openChat(prompt: nil) },
      openHealth: router.openHealth,
      openMore: router.openMore,
      openHomeTab: { router.selectedTab = .home },
      openChatPrompt: openChat(prompt:)
    )
    .bullScreenBackground()
    .navigationTitle("Coach")
    .navigationBarTitleDisplayMode(.inline)
    .toolbarBackground(.hidden, for: .navigationBar)
    .toolbar {
      if chat.isSignedIn {
        ToolbarItem(placement: .topBarTrailing) {
          CoachProfileMenu(chat: chat)
        }
      }
    }
    .sheet(isPresented: $showingChat) {
      NavigationStack {
        chatSheetContent
          .bullScreenBackground()
          .navigationTitle(chatSheetShowsChat ? "Coach Chat" : "Set up Coach")
          .navigationBarTitleDisplayMode(.inline)
          .toolbarBackground(.hidden, for: .navigationBar)
          .toolbar {
            ToolbarItem(placement: .topBarLeading) {
              Button("Done") {
                showingChat = false
              }
            }
            if chat.isSignedIn {
              ToolbarItem(placement: .topBarTrailing) {
                CoachProfileMenu(chat: chat)
              }
            }
          }
      }
    }
    .onAppear {
      model.recordUIAction("page.opened", detail: "Coach")
      healthStore.loadBridgeCatalogsIfNeeded()
      healthStore.refreshPacketInputsIfNeeded()
      chat.refreshAuth()
      applyRequestedCoachPromptIfNeeded()
      refreshCoachSnapshot()
    }
    .onChange(of: healthStore.packetInputStatus) { _, _ in refreshCoachSnapshot() }
    .onChange(of: healthStore.packetScoreStatus) { _, _ in refreshCoachSnapshot() }
    .onChange(of: healthStore.catalogStatus) { _, _ in refreshCoachSnapshot() }
    .onChange(of: healthStore.bandSleepImportStatus) { _, _ in refreshCoachSnapshot() }
    .onChange(of: model.ble.liveHeartRateBPM) { _, _ in refreshCoachSnapshot() }
    .onChange(of: router.coachSetupRequestID) { _, requestID in
      guard requestID > 0, !chatSheetShowsChat else {
        return
      }
      showingChat = true
    }
    .onChange(of: router.coachPromptRequestID) { _, _ in
      applyRequestedCoachPromptIfNeeded()
    }
  }

  /// Chat is only usable once the user is signed in AND has accepted Coach
  /// data sharing. Sign-in can happen at the launch gate, so consent must be
  /// checked independently here — otherwise there is no way to accept it.
  private var chatSheetShowsChat: Bool {
    chat.isSignedIn && !chat.needsConsent
  }

  @ViewBuilder
  private var chatSheetContent: some View {
    if chatSheetShowsChat {
      CoachChatScreen(
        chat: chat,
        healthStore: healthStore,
        appModel: model,
        draft: $promptDraft,
        scrollToBottomRequestID: router.coachScrollToBottomRequestID
      )
    } else {
      CoachSignInScreen(
        loginStatus: chat.loginStatus,
        needsConsent: chat.needsConsent,
        errorMessage: chat.errorMessage,
        acceptConsent: chat.acceptConsent,
        setup: chat.setupCoach
      )
    }
  }

  private var chatStatus: String {
    if chat.isSignedIn {
      return chat.streamState.isStreaming ? "Streaming" : "Ready"
    }
    return chat.loginStatus
  }

  private var coachSnapshot: CoachOverviewSnapshot {
    CoachOverviewSnapshot.make(
      healthStore: healthStore,
      appModel: model,
      calibrationSnapshot: calibration.uiSnapshot
    )
  }

  private func refreshCoachSnapshot() {
    calibration.ensureStarted(connectedAt: model.ble.connectedAt)
    calibration.refreshUISnapshot(store: healthStore, isBandConnected: model.ble.isConnectedForUserBaseline)
    cachedCoachSnapshot = coachSnapshot
  }

  private func openChat(prompt: String?) {
    if let prompt {
      let trimmedPrompt = prompt.trimmingCharacters(in: .whitespacesAndNewlines)
      if !trimmedPrompt.isEmpty {
        promptDraft = trimmedPrompt
      }
    }
    showingChat = true
  }

  private func applyRequestedCoachPromptIfNeeded() {
    guard router.coachPromptRequestID != appliedCoachPromptRequestID else {
      return
    }
    appliedCoachPromptRequestID = router.coachPromptRequestID
    let prompt = router.coachPromptDraft.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !prompt.isEmpty else {
      return
    }
    promptDraft = prompt
    showingChat = true
  }
}

private struct CoachOverviewSnapshot {
  let recommendation: CoachRecommendation
  let highlights: [CoachMetricHighlight]
  let gaps: [CoachDataGap]

  /// Cheap empty state shown before the first real build (e.g. while the tab is
  /// off-screen and has never been opened). Avoids paying for `make` on renders
  /// that will never be seen.
  static let placeholder = CoachOverviewSnapshot(
    recommendation: CoachRecommendation(title: "Review today", message: "", evidence: [], prompt: ""),
    highlights: [],
    gaps: []
  )

  @MainActor
  static func make(
    healthStore: HealthDataStore,
    appModel: BullAppModel,
    calibrationSnapshot: CalibrationUISnapshot
  ) -> CoachOverviewSnapshot {
    let _signpost = bullSignpostBegin(BullSignpost.ui, "CoachOverviewSnapshot.make")
    defer { bullSignpostEnd(_signpost) }
    let homeTip = CoachTipFactory.homeTip(healthStore: healthStore, appModel: appModel)
    let readiness = healthStore.metricInputReadinessSummary()
    let inputNextAction = healthStore.metricInputReadinessNextActionSummary()
    let featureNextAction = healthStore.packetDerivedFeatureNextActionSummary()
    let scoreNextAction = healthStore.packetDerivedScoreNextActionSummary()
    let liveHeartRate = healthStore.latestHeartRateSummary(
      bpm: appModel.ble.liveHeartRateBPM,
      source: appModel.ble.liveHeartRateSource,
      updatedAt: appModel.ble.liveHeartRateUpdatedAt
    )
    let snapshots = [
      healthStore.snapshot(for: .sleep),
      healthStore.snapshot(for: .recovery),
      healthStore.snapshot(for: .strain),
      healthStore.snapshot(for: .stress),
    ]

    let recommendation = CoachRecommendation(
      title: primaryFocusTitle(
        inputNextAction: inputNextAction,
        scoreNextAction: scoreNextAction,
        snapshots: snapshots,
        calibrationSnapshot: calibrationSnapshot
      ),
      message: consumerRecommendationMessage(
        calibrationSnapshot: calibrationSnapshot,
        inputNextAction: inputNextAction,
        scoreNextAction: scoreNextAction,
        snapshots: snapshots,
        isBandConnected: appModel.ble.isConnectedForUserBaseline
      ),
      evidence: [
        "Readiness: \(readiness)",
        "Features: \(featureNextAction)",
        "Scores: \(scoreNextAction)",
        "Latest HR: \(liveHeartRate)",
      ],
      prompt: homeTip.prompt
    )

    var highlights = snapshots.map { snapshot in
      CoachMetricHighlight(
        id: snapshot.route.rawValue,
        title: snapshot.title,
        value: snapshot.displayValue.isEmpty ? "--" : snapshot.displayValue,
        status: humanizedHomeStatus(snapshot.status),
        freshness: snapshot.freshness,
        provenance: "",
        systemImage: snapshot.systemImage,
        tint: snapshot.tint,
        route: snapshot.route
      )
    }
    highlights.append(
      CoachMetricHighlight(
        id: "hrv",
        title: "HRV",
        value: healthStore.hrvFeatureSummary(),
        status: "HRV",
        freshness: healthStore.packetInputStatus,
        provenance: "",
        systemImage: "waveform.path.ecg",
        tint: .blue,
        route: .healthMonitor
      )
    )
    highlights.append(
      CoachMetricHighlight(
        id: "live-hr",
        title: "Live HR",
        value: liveHeartRate,
        status: "Live",
        freshness: HealthDataStore.relativeText(for: appModel.ble.liveHeartRateUpdatedAt) ?? "Waiting",
        provenance: "",
        systemImage: "heart.fill",
        tint: .red,
        route: .healthMonitor
      )
    )

    return CoachOverviewSnapshot(
      recommendation: recommendation,
      highlights: highlights,
      gaps: dataGaps(
        healthStore: healthStore,
        snapshots: snapshots,
        inputNextAction: inputNextAction,
        featureNextAction: featureNextAction,
        scoreNextAction: scoreNextAction,
        calibrationSnapshot: calibrationSnapshot
      )
    )
  }

  private static func primaryFocusTitle(
    inputNextAction: String,
    scoreNextAction: String,
    snapshots: [HealthMetricSnapshot],
    calibrationSnapshot: CalibrationUISnapshot
  ) -> String {
    if calibrationSnapshot.showConnectBandPrompt {
      return "Connect your band"
    }
    if calibrationSnapshot.isInUserBaselinePhase {
      return "Getting to know you"
    }
    if snapshots.contains(where: { $0.source.kind == .unavailable }) {
      return "A few scores are still missing"
    }
    return "Review today"
  }

  @MainActor
  private static func dataGaps(
    healthStore: HealthDataStore,
    snapshots: [HealthMetricSnapshot],
    inputNextAction: String,
    featureNextAction: String,
    scoreNextAction: String,
    calibrationSnapshot: CalibrationUISnapshot
  ) -> [CoachDataGap] {
    var gaps: [CoachDataGap] = []

    if BullFeatureFlags.showCoachDevGaps {
      appendGap(
        &gaps,
        id: "readiness",
        title: "Input readiness",
        detail: inputNextAction,
        systemImage: "square.stack.3d.up",
        tint: .blue,
        actionTitle: "Review Inputs",
        action: .health(.packetInputs)
      )
      appendGap(
        &gaps,
        id: "features",
        title: "Packet features",
        detail: featureNextAction,
        systemImage: "dot.radiowaves.left.and.right",
        tint: .cyan,
        actionTitle: "Review Inputs",
        action: .health(.packetInputs)
      )
      appendGap(
        &gaps,
        id: "scores",
        title: "Score outputs",
        detail: scoreNextAction,
        systemImage: "function",
        tint: .purple,
        actionTitle: "Review Algorithms",
        action: .health(.algorithms)
      )
    }

    for snapshot in snapshots where snapshot.source.kind == .unavailable {
      let action: CoachOverviewAction = consumerMissingMetricAction(for: snapshot.route)
      appendGap(
        &gaps,
        id: "missing-\(snapshot.route.rawValue)",
        title: "\(snapshot.title) not ready yet",
        detail: consumerMissingMetricDetail(for: snapshot.route),
        systemImage: snapshot.systemImage,
        tint: snapshot.tint,
        actionTitle: "Open \(snapshot.title)",
        action: action
      )
    }

    if calibrationSnapshot.isInUserBaselinePhase {
      appendGap(
        &gaps,
        id: "user-calibration",
        title: "Building your baseline",
        detail: "Day \(calibrationSnapshot.dayIndex) of \(calibrationSnapshot.daysRequired). \(calibrationSnapshot.homeActionLine)",
        systemImage: "figure.wave",
        tint: .mint,
        actionTitle: "Open Home",
        action: .homeTab
      )
    }

    return Array(gaps.prefix(5))
  }

  private static func appendGap(
    _ gaps: inout [CoachDataGap],
    id: String,
    title: String,
    detail: String,
    systemImage: String,
    tint: Color,
    actionTitle: String,
    action: CoachOverviewAction
  ) {
    let trimmed = detail.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty, trimmed.localizedCaseInsensitiveContains("review calibrated") == false else {
      return
    }
    guard gaps.contains(where: { $0.id == id }) == false else {
      return
    }
    gaps.append(
      CoachDataGap(
        id: id,
        title: title,
        detail: trimmed,
        systemImage: systemImage,
        tint: tint,
        actionTitle: actionTitle,
        action: action
      )
    )
  }

  private static func consumerRecommendationMessage(
    calibrationSnapshot: CalibrationUISnapshot,
    inputNextAction: String,
    scoreNextAction: String,
    snapshots: [HealthMetricSnapshot],
    isBandConnected: Bool
  ) -> String {
    if calibrationSnapshot.showConnectBandPrompt {
      return "Pair your band in Device to start building your baseline."
    }
    if calibrationSnapshot.isInUserBaselinePhase, !calibrationSnapshot.homeActionLine.isEmpty {
      return calibrationSnapshot.homeActionLine
    }
    if !isBandConnected {
      return "Connect your band to see today's scores."
    }
    if snapshots.contains(where: { $0.source.kind == .unavailable }) {
      return "Wear your band and check back — a few scores need more time on your wrist."
    }
    return firstUseful(
      inputNextAction,
      scoreNextAction,
      "Ask Coach how to spend today based on your latest scores."
    )
  }

  private static func consumerMissingMetricAction(for route: HealthRoute) -> CoachOverviewAction {
    switch route {
    case .sleep: .health(.sleep)
    case .recovery: .health(.recovery)
    case .strain: .health(.strain)
    case .stress: .health(.stress)
    default: .homeTab
    }
  }

  private static func consumerMissingMetricDetail(for route: HealthRoute) -> String {
    switch route {
    case .sleep:
      return "Wear your band overnight, then open Sleep."
    case .recovery:
      return "Keep the band on through the morning for Recovery."
    case .strain:
      return "Move with the band on — Strain fills in from your day."
    case .stress:
      return "Heart-rate history builds Stress over a few days."
    default:
      return "Check Home after your band has more time on your wrist."
    }
  }

  private static func firstUseful(_ values: String...) -> String {
    values
      .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
      .first { !$0.isEmpty } ?? "Ask Coach how to spend today based on your latest scores."
  }
}

private struct CoachRecommendation {
  let title: String
  let message: String
  let evidence: [String]
  let prompt: String
}

private struct CoachMetricHighlight: Identifiable {
  let id: String
  let title: String
  let value: String
  let status: String
  let freshness: String
  let provenance: String
  let systemImage: String
  let tint: Color
  let route: HealthRoute
}

private struct CoachDataGap: Identifiable {
  let id: String
  let title: String
  let detail: String
  let systemImage: String
  let tint: Color
  let actionTitle: String
  let action: CoachOverviewAction
}

private enum CoachOverviewAction: Hashable {
  case health(HealthRoute)
  case more(MoreRoute)
  case chat(String)
  case homeTab
}

private struct CoachOverviewScreen: View {
  let snapshot: CoachOverviewSnapshot
  let chatIsSignedIn: Bool
  let chatStatus: String
  let openChat: () -> Void
  let openHealth: (HealthRoute?) -> Void
  let openMore: (MoreRoute?) -> Void
  let openHomeTab: () -> Void
  let openChatPrompt: (String) -> Void

  var body: some View {
    ScrollView {
      LazyVStack(alignment: .leading, spacing: 16) {
        CoachRecommendationCard(recommendation: snapshot.recommendation) {
          openChatPrompt(snapshot.recommendation.prompt)
        }

        CoachOverviewChatCard(
          signedIn: chatIsSignedIn,
          status: chatStatus,
          action: openChat
        )

        CoachOverviewSectionTitle("Metric Highlights")
        LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 10) {
          ForEach(snapshot.highlights) { highlight in
            Button {
              openHealth(highlight.route)
            } label: {
              CoachMetricHighlightCard(highlight: highlight)
            }
            .buttonStyle(.plain)
          }
        }

        if !snapshot.gaps.isEmpty {
          CoachOverviewSectionTitle("Data Gaps")
          VStack(spacing: 10) {
            ForEach(snapshot.gaps) { gap in
              CoachDataGapCard(gap: gap) {
                handle(gap.action)
              }
            }
          }
        }
      }
      .padding(.horizontal, 16)
      .padding(.vertical, 18)
    }
    .scrollClipDisabled()
  }

  private func handle(_ action: CoachOverviewAction) {
    switch action {
    case .homeTab:
      openHomeTab()
    case .health(let route):
      openHealth(route)
    case .more(let route):
      openMore(route)
    case .chat(let prompt):
      openChatPrompt(prompt)
    }
  }
}

private struct CoachRecommendationCard: View {
  let recommendation: CoachRecommendation
  let ask: () -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 13) {
      HStack(alignment: .top, spacing: 12) {
        Image(systemName: "sparkles")
          .font(.system(size: 18, weight: .semibold))
          .foregroundStyle(.purple)
          .frame(width: 38, height: 38)
          .background(.purple.opacity(0.12), in: RoundedRectangle(cornerRadius: 8, style: .continuous))

        VStack(alignment: .leading, spacing: 5) {
          Text(recommendation.title)
            .font(.title3.weight(.semibold))
          Text(recommendation.message)
            .font(.subheadline)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
        }
      }

      VStack(alignment: .leading, spacing: 7) {
        ForEach(recommendation.evidence, id: \.self) { evidence in
          Label(evidence, systemImage: "checkmark.seal")
            .font(.caption)
            .foregroundStyle(.secondary)
            .lineLimit(2)
            .fixedSize(horizontal: false, vertical: true)
        }
      }

      Button(action: ask) {
        Label("Ask About This", systemImage: "bubble.left.and.bubble.right")
          .font(.subheadline.weight(.semibold))
          .frame(maxWidth: .infinity)
      }
      .buttonStyle(.borderedProminent)
    }
    .padding(16)
    .coachCardSurface(tint: .purple, prominent: true)
  }
}

private struct CoachOverviewChatCard: View {
  let signedIn: Bool
  let status: String
  let action: () -> Void

  var body: some View {
    HStack(spacing: 12) {
      Image(systemName: signedIn ? "bubble.left.and.bubble.right.fill" : "person.crop.circle.badge.checkmark")
        .font(.system(size: 17, weight: .semibold))
        .foregroundStyle(signedIn ? .blue : .secondary)
        .frame(width: 36, height: 36)
        .background((signedIn ? Color.blue : Color.secondary).opacity(0.12), in: RoundedRectangle(cornerRadius: 8, style: .continuous))

      VStack(alignment: .leading, spacing: 3) {
        Text(signedIn ? "Ask Coach" : "Set up Coach")
          .font(.headline)
        Text(status.isEmpty ? "Local Coach works without chat" : status)
          .font(.caption)
          .foregroundStyle(.secondary)
          .lineLimit(1)
      }

      Spacer(minLength: 8)

      Button(signedIn ? "Open" : "Set up", action: action)
        .font(.caption.weight(.semibold))
        .buttonStyle(.bordered)
        .controlSize(.small)
    }
    .padding(14)
    .coachCardSurface(tint: .blue)
  }
}

private struct CoachMetricHighlightCard: View {
  let highlight: CoachMetricHighlight

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      HStack(spacing: 8) {
        Image(systemName: highlight.systemImage)
          .font(.caption.weight(.bold))
          .foregroundStyle(highlight.tint)
        Text(highlight.title)
          .font(.caption.weight(.bold))
          .foregroundStyle(.secondary)
          .lineLimit(1)
        Spacer(minLength: 0)
      }

      Text(highlight.value)
        .font(.title3.weight(.semibold))
        .fontDesign(.rounded)
        .lineLimit(2)
        .minimumScaleFactor(0.70)

      VStack(alignment: .leading, spacing: 3) {
        Text(highlight.status)
          .font(.caption.weight(.semibold))
          .foregroundStyle(.primary)
          .lineLimit(1)
        Text(highlight.freshness)
          .font(.caption2)
          .foregroundStyle(.secondary)
          .lineLimit(1)
      }

      Spacer(minLength: 0)
    }
    .frame(maxWidth: .infinity, minHeight: 154, alignment: .topLeading)
    .padding(13)
    .coachCardSurface(tint: highlight.tint)
  }
}

private struct CoachDataGapCard: View {
  let gap: CoachDataGap
  let action: () -> Void

  var body: some View {
    HStack(alignment: .top, spacing: 12) {
      Image(systemName: gap.systemImage)
        .font(.system(size: 16, weight: .semibold))
        .foregroundStyle(gap.tint)
        .frame(width: 34, height: 34)
        .background(gap.tint.opacity(0.12), in: RoundedRectangle(cornerRadius: 8, style: .continuous))

      VStack(alignment: .leading, spacing: 5) {
        Text(gap.title)
          .font(.subheadline.weight(.semibold))
        Text(gap.detail)
          .font(.caption)
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
      }

      Spacer(minLength: 8)

      Button(gap.actionTitle, action: action)
        .font(.caption.weight(.semibold))
        .buttonStyle(.bordered)
        .controlSize(.small)
    }
    .padding(13)
    .coachCardSurface(tint: gap.tint)
  }
}

private struct CoachOverviewSectionTitle: View {
  let title: String

  init(_ title: String) {
    self.title = title
  }

  var body: some View {
    Text(title)
      .font(.headline.weight(.semibold))
      .frame(maxWidth: .infinity, alignment: .leading)
      .padding(.top, 2)
  }
}

private extension View {
  func coachCardSurface(tint: Color, prominent: Bool = false) -> some View {
    background(
      RoundedRectangle(cornerRadius: 8, style: .continuous)
        .fill(Color(.secondarySystemGroupedBackground))
        .shadow(color: tint.opacity(prominent ? 0.16 : 0.08), radius: prominent ? 14 : 8, x: 0, y: prominent ? 7 : 3)
    )
    .overlay {
      RoundedRectangle(cornerRadius: 8, style: .continuous)
        .stroke(tint.opacity(prominent ? 0.18 : 0.10), lineWidth: 1)
    }
  }
}

private struct CoachProfileMenu: View {
  @ObservedObject var chat: CoachChatModel

  var body: some View {
    Menu {
      Section("Model") {
        ForEach(CoachModelPreset.allCases) { preset in
          Button {
            chat.selectModelPreset(preset)
          } label: {
            VStack(alignment: .leading, spacing: 1) {
              HStack {
                Text(preset.title)
                if chat.modelPreset == preset {
                  Image(systemName: "checkmark")
                    .font(.caption.weight(.semibold))
                }
              }
              Text(preset.subtitle)
                .font(.caption2)
                .foregroundStyle(.secondary)
            }
          }
        }
      }

      Toggle(isOn: Binding(
        get: { chat.showToolActivity },
        set: { chat.setShowToolActivity($0) }
      )) {
        Label("Show Tool Activity", systemImage: "wrench.and.screwdriver")
      }

      Button(role: .destructive) {
        chat.startNewConversation()
      } label: {
        Label("New Conversation", systemImage: "plus.message")
      }
      .disabled(chat.streamState.isStreaming)

      Button(role: .destructive) {
        chat.signOut()
      } label: {
        Label("Sign Out", systemImage: "rectangle.portrait.and.arrow.right")
      }
    } label: {
      Image(systemName: "person.crop.circle")
    }
    .accessibilityLabel("Coach account")
  }
}

#Preview("Signed out") {
  NavigationStack {
    CoachView(healthStore: HealthDataStore())
      .environmentObject(BullAppModel(startBLE: false))
      .environmentObject(AppRouter())
  }
}
