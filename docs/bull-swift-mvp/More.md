# Bull Swift MVP: More

Source map: Flutter `SettingsView`, `DeviceView`, `CaptureView`, `DebugView`, Swift `MorePlaceholderView`, Swift `DeviceView`, Swift `ConnectionView`.

MVP rule: More owns operational surfaces: device, connection lab, capture/import, Health sync, raw export, algorithm settings, storage, debug, privacy, and support. It should be dense, inspectable, and honest about readiness.

## Parent View Contract

- [ ] Create a dedicated `MoreView.swift` or split `MorePlaceholderView` out of `AppShellView.swift`.
- [ ] Keep this tab behind the Swift `More` tab item.
- [ ] Define child routes: Device, Connection Lab, Capture, Debug, Local Store, Health Sync, Raw Export, Algorithms, Privacy, Support/About.
- [ ] Keep operational rows compact and list-based.
- [ ] Add status badges for ready, pending, blocked, unavailable, stale.
- [ ] Add previews for default, connected, and debug-heavy states.

## Device

- [ ] Keep current Swift `DeviceView` as the primary Device route.
- [ ] Show status and advanced panels.
- [ ] Keep WHOOP image asset in Swift asset catalog.
- [ ] Show live device name, connection, battery, firmware, model, last sync.
- [ ] Show live HR, Rust status, last parsed frame summary.
- [ ] Show actions: Bluetooth, scan, connect, reconnect, send hello, forget.
- [ ] Show discovered devices list.
- [ ] Show recent event log.
- [ ] Ensure all copy is backed by `BullBLEClient` or marked unavailable.

## Connection Lab

- [ ] Keep existing `ConnectionView` as a lab/debug route, not the primary user device view.
- [ ] Show Bluetooth state.
- [ ] Show connection state.
- [ ] Show reconnect state.
- [ ] Show remembered device.
- [ ] Show live HR source/update.
- [ ] Show Rust and client hello summaries.
- [ ] Show discovered devices and event log.
- [ ] Keep command actions available for debugging.

## Capture

- [ ] Port capture/import surface from Flutter `CaptureView`.
- [ ] Show capture session summary from `captureSessionSummary()`.
- [ ] Show live notification capture summary from `liveNotificationCaptureSummary()`.
- [ ] Show selected discovered device.
- [ ] Show recent notifications/events.
- [ ] Add actions for starting/stopping capture where Swift bridge supports it.
- [ ] Add import capture file action.
- [ ] Add import command evidence file action.
- [ ] Add import emulator log action.
- [ ] Add local frame match action.
- [ ] Add validated sample/read command action.

## Local Store

- [ ] Show SQLite/local store path.
- [ ] Show storage check status from `storageCheckStatusSummary()`.
- [ ] Show schema version.
- [ ] Show storage next action from `storageCheckNextActionSummary()`.
- [ ] Add Check action once Swift bridge supports storage check.
- [ ] Add empty state for no database yet.

## Health Sync

- [ ] Show backfill window from `healthSyncBackfillWindowSummary()`.
- [ ] Show backfill validation issue from `healthSyncBackfillWindowIssueSummary()`.
- [ ] Add editable backfill start/end fields.
- [ ] Show selected metric families from `healthSyncMetricFamilySummary()`.
- [ ] Add family toggles: heart_rate, resting_heart_rate, hrv, steps, activity.
- [ ] Show metric source rows via `healthSyncMetricSourceSummary(family)`.
- [ ] Show unavailable families: respiratory_rate, oxygen_saturation, skin_temperature, sleep, active_energy.
- [ ] Show Health adapter availability.
- [ ] Show Health authorization state.
- [ ] Show existing Bull records.
- [ ] Show platform sleep imports only as reference/quarantined evidence.
- [ ] Add Apple Health dry run action only for outbound/profile-boundary audits.
- [ ] Add Health Connect dry run action only if Android/shared build ever needs it.
- [ ] Add refresh Health adapter action.
- [ ] Show platform reports from `healthSyncReports`.

## Raw Export

- [ ] Show export window from `rawExportWindowSummary()`.
- [ ] Show export window issues from `rawExportWindowIssueSummary()`.
- [ ] Show export scope from `rawExportScopeSummary()`.
- [ ] Add editable fields: start, end, capture sessions, packet types, sensor signals, metric families, algorithm ids, algorithm versions.
- [ ] Add raw bytes toggle.
- [ ] Add data family chips: raw_evidence, decoded_frames, packet_timeline, metric_inputs, algorithm_runs, calibration_labels, calibration_runs, sqlite.
- [ ] Show recent capture sessions as shortcut rows for the export window.
- [ ] Add Export action.
- [ ] Show bundle path, zip path, row counts, export status.
- [ ] Show bundle validation, zip validation, privacy lint, and sanitized privacy statuses.

## Algorithms

- [ ] Show algorithm preference picker per family.
- [ ] Add "Defaults" action from `applyRecommendedAlgorithmDefaults()`.
- [ ] Show reference benchmark details per family.
- [ ] Link to Health > Algorithms for deeper metric context.
- [ ] Keep operational setting here and metric explanation in Health.

## Debug

- [ ] Port `DebugView` as an explicit route.
- [ ] Show Rust bridge/core version.
- [ ] Show frame parse status, CRC, payload, warnings, timeline.
- [ ] Show debug WebSocket status and next action.
- [ ] Show UI coverage status and deferred surfaces.
- [ ] Show property suite and perf budget status.
- [ ] Show command evidence import/gate sweep/capture plan.
- [ ] Show command shortcuts grouped by identity, battery, historical sync, haptics, sensors, config, firmware, reboot.
- [ ] Keep destructive commands gated behind explicit confirmation.

## Privacy And Support

- [ ] Add Privacy route with local-data/export/privacy-lint summaries.
- [ ] Add Support route with logs/export bundle paths.
- [ ] Add About route with app version, Rust core version, and license placeholders.
- [ ] Add data deletion/export links when implemented.

## Parallel Agent Tasks

- [ ] Agent More-A: Extract More tab and build route list.
- [ ] Agent More-B: Finalize Device route and Connection Lab split.
- [ ] Agent More-C: Implement Capture route.
- [ ] Agent More-D: Implement Local Store and Health Sync.
- [ ] Agent More-E: Implement Raw Export.
- [ ] Agent More-F: Implement Algorithms settings.
- [ ] Agent More-G: Implement Debug route and command groups.
- [ ] Agent More-H: Implement Privacy, Support, About.
- [ ] Agent More-I: Add previews and simulator screenshot verification.

## Acceptance Checks

- [ ] More can be worked on without changing Home/Health/Coach code.
- [ ] Device route continues to update live BLE state.
- [ ] Every operational action is disabled unless its backing bridge exists and inputs are valid.
- [ ] Raw export and Health sync clearly show pending/unavailable states.
- [ ] Debug/destructive commands are not reachable by accidental taps.
