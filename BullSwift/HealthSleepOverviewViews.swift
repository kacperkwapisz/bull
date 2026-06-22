import Darwin
import Foundation
import SwiftUI
import UIKit

struct SleepV2OverviewPage: View {
  @EnvironmentObject private var router: AppRouter
  @EnvironmentObject private var calibration: CalibrationManager
  @ObservedObject var store: HealthDataStore
  @ObservedObject var ble: BullBLEClient
  @Binding var selectedDate: Date
  @Environment(\.colorScheme) private var colorScheme
  @State private var showingInsightsSheet = false
  @State private var showingAlarmSheet = false
	  @State private var showingSleepNeededSheet = false
	  @State private var showingDatePicker = false
    @State private var selectedTrend: HealthMetricSnapshot?
	  @State private var selectedPrimarySleep: PrimarySleepDetail?
	  @State private var scrollOffsetY: CGFloat = 0
    @State private var autoBandSyncRequested = false
    // Cached per data change instead of recomputed on every body pass; this page
    // re-renders while the parallax header scrolls.
    @State private var cachedData: SleepV2PageData?
	  private let heroHeight: CGFloat = 320
	  private let heroBackgroundHeight: CGFloat = 560
    private var autoBandSleepSyncEnabled: Bool {
      let processInfo = ProcessInfo.processInfo
      return processInfo.arguments.contains("--bull-auto-band-sleep-sync")
        || processInfo.environment["BULL_AUTO_BAND_SLEEP_SYNC"] == "1"
    }

	  var body: some View {
	    let _ = Self.bullPrintChangesIfEnabled()
	    let data = cachedData ?? pageData()
	    let palette = SleepV2Palette(colorScheme: colorScheme)
		    ScrollViewReader { _ in
	      ZStack(alignment: .top) {
	        palette.background
	          .ignoresSafeArea()

	        SleepV2ScenicBackground(palette: palette)
	          .frame(height: heroBackgroundHeight)
	          .offset(y: min(scrollOffsetY, 0))
	          .ignoresSafeArea(edges: .top)
	          .allowsHitTesting(false)

	        ScrollView {
	          LazyVStack(alignment: .leading, spacing: 0) {
	            SleepV2ScrollOffsetProbe()

	            CalibrationHeroContainer(
	              snapshot: calibration.uiSnapshot,
	              route: .sleep,
	              palette: palette,
	              onCelebrateCompletion: { calibration.markCompletionCelebrated() }
	            ) {
	              SleepV2Hero(
	                palette: palette,
	                title: "Sleep",
	                dateLabel: dateLabel,
	                score: data.score,
	                onDateTap: { showingDatePicker = true }
	              )
	            }
	            .frame(height: heroHeight)

	            VStack(alignment: .leading, spacing: 14) {
	              HStack(spacing: 12) {
                SleepV2StatCard(
                  palette: palette,
                  systemImage: "bed.double.fill",
                  label: "Time in Bed",
                  value: data.primarySleep?.timeInBedText ?? "No data"
                )
                SleepV2StatCard(
                  palette: palette,
                  systemImage: "clock.fill",
                  label: "Time Asleep",
                  value: data.primarySleep?.durationText ?? "No data"
                )
              }
              .frame(height: 96)

              SleepV2CoachingCard(palette: palette, tip: data.coachTip) {
                router.openCoach(prompt: data.coachTip.prompt)
              }

	              SleepV2ActionRow(
	                palette: palette,
	                systemImage: "sparkles",
	                title: "View insights",
	                action: { showingInsightsSheet = true }
	              )

	              SleepV2SleepWindowCard(
                palette: palette,
                onWakeTap: { showingAlarmSheet = true },
                onSleepNeeded: { showingSleepNeededSheet = true }
              )

              SleepV2BandSyncStatusLine(ble: ble, palette: palette) {
                startBandSleepSync(automatic: false)
              }

              SleepV2SectionHeader(title: "Timeline", palette: palette)

              SleepV2TimelineRow(
                palette: palette,
                session: data.primarySleep,
                action: {
                  if let primarySleep = data.primarySleep {
                    selectedPrimarySleep = primarySleep
                  }
                }
              )

              SleepV2SectionHeader(title: "Trends", palette: palette)

              VStack(spacing: 14) {
                ForEach(data.trendRows) { snapshot in
                  SleepV2TrendRow(palette: palette, snapshot: snapshot) {
                    selectedTrend = snapshot
                  }
	                }
	              }
	            }
	            .padding(.horizontal, 18)
	            .padding(.bottom, 34)
	          }
	        }
	        .coordinateSpace(name: SleepV2ScrollOffsetProbe.coordinateSpaceName)
	        .onPreferenceChange(SleepV2ScrollOffsetPreferenceKey.self) { value in
	          let clamped = max(min(value, 0), -heroBackgroundHeight).rounded()
	          if clamped != scrollOffsetY {
	            scrollOffsetY = clamped
	          }
	        }
	      }
	    }
    .navigationTitle("Sleep")
    .navigationBarTitleDisplayMode(.inline)
    .toolbarBackground(.hidden, for: .navigationBar)
    .toolbar {
      ToolbarItem(placement: .principal) {
        Text("Sleep")
          .font(.headline.weight(.semibold))
          .foregroundStyle(palette.text)
      }
      ToolbarItem(placement: .topBarTrailing) {
        Button {
          showingAlarmSheet = true
        } label: {
          Image(systemName: "alarm")
        }
        .accessibilityLabel("Sleep alarm settings")
      }
    }
    .onAppear {
      store.loadBridgeCatalogsIfNeeded()
      startBandSleepSyncIfReady()
      refreshData()
    }
    .onChange(of: selectedDate) { _, _ in
      refreshData()
    }
    .onChange(of: store.catalogStatus) { _, _ in
      refreshData()
    }
    .onChange(of: ble.canSyncHistorical) { _, _ in
      startBandSleepSyncIfReady()
    }
    .onChange(of: ble.historicalSyncStatus) { _, newValue in
      if newValue == "synced" {
        store.refreshSleepAfterBandSync(packetCount: ble.historicalPacketCount)
        refreshData()
      } else if newValue == "failed" {
        store.markBandSleepSyncFailed(ble.historicalSyncStatus)
      }
    }
    .sheet(isPresented: $showingDatePicker) {
      ScoreDatePickerSheet(
        title: "Sleep",
        routes: [.sleep],
        snapshots: [store.snapshot(for: .sleep)],
        calendarDays: store.calendarDays,
        selectedDate: $selectedDate
      )
    }
	    .sheet(isPresented: $showingAlarmSheet) {
	      SleepV2AlarmSheet(ble: ble)
	    }
	    .sheet(isPresented: $showingSleepNeededSheet) {
	      SleepV2SleepNeededSheet(palette: palette)
	    }
	    .sheet(isPresented: $showingInsightsSheet) {
	      SleepV2InsightsSheet(palette: palette)
	    }
		    .sheet(item: $selectedTrend) { snapshot in
		      SleepV2BevelTrendSheet(snapshot: snapshot)
		    }
    .sheet(item: $selectedPrimarySleep) { sleep in
      PrimarySleepDetailSheet(sleep: sleep)
    }
  }

  private var dateLabel: String {
    let suffix = selectedDate.formatted(.dateTime.day().month(.abbreviated))
    let prefix = ScoreDateTimeline.dateLabel(for: selectedDate)
    return "\(prefix), \(suffix)"
  }

  private func refreshData() {
    calibration.refreshUISnapshot(store: store, isBandConnected: ble.isConnectedForUserBaseline)
    cachedData = pageData()
  }

  private func pageData() -> SleepV2PageData {
    let selectedSnapshot = ScoreDateTimeline.datedSnapshot(
      from: store.snapshot(for: .sleep),
      date: selectedDate
    )
    let primarySleep = store.primarySleep()
    let score = SleepV2Numbers.firstInt(in: selectedSnapshot.value)
      ?? SleepV2Numbers.firstInt(in: primarySleep?.scoreText ?? "")
    return SleepV2PageData(
      score: score,
      primarySleep: primarySleep,
      trendRows: store.trendRows(for: .sleep),
      coachTip: CoachTipFactory.sleepTip(healthStore: store, ble: ble, calibrationSnapshot: calibration.uiSnapshot)
    )
  }

  private func startBandSleepSyncIfReady() {
    guard autoBandSleepSyncEnabled else {
      if !autoBandSyncRequested {
        autoBandSyncRequested = true
        ble.record(
          source: "health.sleep",
          title: "band_sleep_sync.auto_skipped",
          body: "autoBandSleepSync=false"
        )
      }
      return
    }
    guard !autoBandSyncRequested, ble.canSyncHistorical else {
      return
    }
    autoBandSyncRequested = true
    startBandSleepSync(automatic: true)
  }

  private func startBandSleepSync(automatic: Bool) {
    store.markBandSleepSyncRequested(
      automatic: automatic,
      canSync: ble.canSyncHistorical,
      detail: ble.historicalSyncStatus
    )
    guard ble.canSyncHistorical else {
      return
    }
    ble.syncHistoricalPackets(rangeFirst: true)
  }

}

private struct SleepV2PageData {
  let score: Int?
  let primarySleep: PrimarySleepDetail?
  let trendRows: [HealthMetricSnapshot]
  let coachTip: CoachInlineTip
}

