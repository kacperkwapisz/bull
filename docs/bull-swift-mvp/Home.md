# Bull Swift MVP: Home

Source map: Flutter `TodayView`, Flutter `BullShell` Overview tab, Swift `AppShellView` Home tab, Swift `DeviceView`.

MVP rule: Home is the daily command center. It should show today's live state first, use real connected-device and local metric data when present, and fall back to empty, stale, or unavailable states where no Swift data bridge exists yet.

## Parent View Contract

- [ ] Create a dedicated `HomeView.swift` or split `HomeDashboardView` out of `AppShellView.swift`.
- [ ] Keep this tab behind the Swift `Home` tab item.
- [ ] Use `NavigationStack` routes for device, score detail, health monitor, timeline item, journal, activity, capture, and settings.
- [ ] Define a `HomeSnapshot` value type that can be populated from `BullAppModel`, `BullBLEClient`, and Rust/local store calls.
- [ ] Add loading, empty, stale, and unavailable states per section; do not use sample data at runtime.
- [ ] Add previews for connected, disconnected, no-data, and populated real-data days.
- [ ] Add accessibility labels for every tappable card and date/device control.

## Top Chrome And Date

- [ ] Show selected date with previous/next day controls.
- [ ] Add date picker route/sheet equivalent to Flutter `TodayView._pickDate`.
- [ ] Show busy/sync indicator when device or metric refresh is running.
- [ ] Add device toolbar button with connected/disconnected color state.
- [ ] Tapping the device button opens `DeviceView`.
- [ ] Define one shared relative-time formatter for `lastSyncAt`, battery, HR, and metric refreshes.

## Device Status Card

- [ ] Show active device name from `ble.activeDeviceName`.
- [ ] Show connection state from `ble.connectionState`.
- [ ] Show reconnect state from `ble.reconnectState`.
- [ ] Show battery percent from `ble.batteryLevelPercent`.
- [ ] Show live HR from `ble.liveHeartRateBPM`.
- [ ] Show last sync from `ble.lastSyncAt`.
- [ ] Include quick action to scan/reconnect when disconnected.
- [ ] Keep copy live: no static "Connected" text unless BLE state says connected/ready.

## Today Score Stack

- [ ] Add Sleep score card/gauge from Flutter `todaySleepScoreSummary()`.
- [ ] Add Recovery score card/gauge from Flutter `todayRecoveryScoreSummary()`.
- [ ] Add Strain score card/gauge from Flutter `todayStrainScoreSummary()`.
- [ ] Preserve strain denominator semantics: Flutter normalizes strain from a 21-point scale to percent for some visuals.
- [ ] Show HRV summary from Flutter `todayHrvScoreSummary()` where it improves the daily snapshot.
- [ ] Parse score values into numeric + status + provenance fields instead of displaying only raw summary strings.
- [ ] Tapping Sleep opens Health > Sleep detail.
- [ ] Tapping Recovery opens Health > Recovery detail.
- [ ] Tapping Strain opens Health > Strain detail.
- [ ] Include provenance badges when `todayScoreProvenanceSummary(family)` is available.

## Daily Outlook / Coach Teaser

- [ ] Show readiness summary from `metricInputReadinessSummary()`.
- [ ] Show input next action from `metricInputReadinessNextActionSummary()`.
- [ ] Show score next action from `packetDerivedScoreNextActionSummary()`.
- [ ] Provide a clear route into Coach for the day's recommendation.
- [ ] Provide a clear route into Capture when the next action needs fresh data.
- [ ] Provide missing-data copy when readiness is missing or pending.

## Stress And Energy

- [ ] Show Stress summary from `todayStressScoreSummary()`.
- [ ] Link Stress card to Health > Stress detail.
- [ ] Add Energy Bank card based on Flutter `V2EnergyBankPage`.
- [ ] Track Energy Bank data points: energy level, stress value, total charged, total drained, primary sleep contribution, usage window.
- [ ] Add unavailable chart state until Swift has the energy time-series bridge.
- [ ] Show coaching copy only from computed/local data or explicit missing-data state.

## Health Monitor Preview

- [ ] Show Latest HR from `latestHeartRateSummary()` or BLE live HR.
- [ ] Show HRV from `todayHrvScoreSummary()` / `hrvFeatureSummary()`.
- [ ] Show Recovery from `todayRecoveryScoreSummary()`.
- [ ] Show Stress from `todayStressScoreSummary()`.
- [ ] Show Sleep from `todaySleepScoreSummary()`.
- [ ] Link card to Health > Health Monitor.
- [ ] Include preview/stale state if any child metric is missing.

## Daily Timeline

- [ ] Add primary sleep row: start/end, duration, score/status.
- [ ] Add activity/strain row: activity summary, strain, calories/energy where available.
- [ ] Add recovery row: score, HRV, resting HR where available.
- [ ] Preserve Flutter routes: sleep tap, activity tap, recovery tap.
- [ ] Make timeline rows data-driven so later captures can insert workouts, naps, journal entries, and calibration events.
- [ ] Add empty timeline state for first-run devices.

## Tools Grid

- [ ] Add Sleep Coach shortcut to Coach/Sleep planning.
- [ ] Add Activity shortcut to Capture or activity entry flow.
- [ ] Add Journal shortcut to Coach/Journal prompt.
- [ ] Add Calibration shortcut to More/Algorithms or Health/Calibration.
- [ ] Surface each tool's readiness state, not just a static label.

## Evidence Footer

- [ ] Show Rust core version from `model.rustStatus`.
- [ ] Show local database/store path or "pending".
- [ ] Show mode: local data, live device, imported capture, or unavailable.
- [ ] Link to More > Debug when evidence/provenance is tapped.
- [ ] Include latest HR, sleep, recovery, and strain provenance when present.

## Parallel Agent Tasks

- [ ] Agent Home-A: Extract `HomeDashboardView` into `HomeView.swift` and keep behavior unchanged.
- [ ] Agent Home-B: Define `HomeSnapshot` and parse summary strings into typed display fields.
- [ ] Agent Home-C: Build the daily score stack and navigation to Health child pages.
- [ ] Agent Home-D: Build Health Monitor preview and Daily Timeline.
- [ ] Agent Home-E: Build Tools grid and Evidence footer.
- [ ] Agent Home-F: Add previews and simulator screenshot checks for connected/disconnected/no-data states.

## Acceptance Checks

- [ ] Home builds without touching Health/Coach/More internals.
- [ ] Home can render with no device connected.
- [ ] Home updates live HR/battery/connection without relaunch.
- [ ] Every card either links somewhere useful or is explicitly disabled with an empty-state reason.
- [ ] Simulator screenshots cover populated, disconnected, and no-data states.
