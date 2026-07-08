# Sync architecture — local-first compute + optional backup

Goal: the phone is the **primary store and compute surface**. The app captures
BLE frames, stores them in the local SQLite database, computes scores on-device
through the linked Bull Rust core, and uses the cloud only for optional Coach AI
and opt-in backup/restore.

> **Status (2026-07-08): local-first compute is underway.** The thin-client model
> documented here on 2026-06-18 has been superseded. The iOS app now computes
> scores locally via `BullSwift/LocalHomeService.swift` and
> `HealthDataStore+LocalCompute.swift`, reading from the local SQLite store. The
> local store retains the recent scoring window needed for on-device metrics;
> `BullFrameDrainUploader.syncedPruneCutoff` keeps 16 days locally. Frame upload
> is an opt-in backup path, not the metric compute path.

## Principles

- **Local-first by default.** Daily health metrics should be available from the
  local store without requiring a server round trip.
- **Device data provenance.** Physiological metrics derive from the connected
  device's own sensor data. HealthKit reads remain limited to body weight for
  profile autofill.
- **Honest states over guesses.** Missing local source data should produce empty,
  stale, or unavailable states rather than inferred values.
- **Bounded local retention.** Keep enough local frame history for scoring and
  troubleshooting while bounding storage growth. The current scoring retention
  target is 16 days.
- **Optional cloud.** BullAPI supports Coach AI proxying and opt-in backup/restore.
  Upload success must not be required for score availability.
- **Idempotent backup.** When a user opts into backup, repeated uploads should be
  safe. Deduplication and pruning should preserve the local scoring window.

## Current data flow

1. **Capture:** the app receives live and historical BLE frames from the device.
2. **Store:** frames and decoded records are written to the local SQLite store.
3. **Compute:** local services read SQLite data and call the linked Rust core to
   produce sleep, recovery, strain, stress, energy, vitals, and activity results.
4. **Render:** Home, Health, Coach context, and detail views read local computed
   summaries and show unavailable states when data is insufficient.
5. **Optional cloud:** if enabled, frame upload backs up data to BullAPI and Coach
   AI can use app-provided summaries through the proxy.
6. **Prune:** synced/eligible frames may be pruned only past the retained local
   scoring window (`BullFrameDrainUploader.syncedPruneCutoff`).

## Phase A — Local incremental + UI honesty

| # | Task | Where | Status |
|---|------|-------|--------|
| A-1 | **Drop the historical % / ETA progress bar.** It has no stable total and misled more than it helped. Replace with an honest indeterminate state: "Syncing — N packets". Keep packet count as telemetry. | `DeviceView.swift` | ⬜ |
| A-2 | **`device_timestamp` on `decoded_frames`** (schema v17) — the data packet's own `timestamp_seconds`, populated at insert; indexed `(packet_type, device_timestamp)`. | `store.rs` | ✅ |
| A-3 | ~~Structural content/timestamp dedup~~ **Rejected.** Content (`sha256`) dedup breaks the `evidence_id` pipeline (a deduped raw insert orphans the following `decoded_frames` insert → FK violation) and would drop byte-identical realtime samples. A `(device_timestamp, packet_type)` unique index is lossy for sub-second realtime. Dedup is done at the sync/backup boundary instead. | — | ✅ (decided) |
| A-4 | **Watermark getters** — `historical_watermarks()` (`MAX(device_timestamp)` per `packet_type`) + `historical_watermark_max()`; bridge `store.historical_watermarks`. | `store.rs`, `bridge.rs` | ✅ |
| A-5 | **Skip-already-backed-up re-pulls** — persistent `historical_sync_watermark` (schema v18, `sync_state` table, survives pruning); backup can mark already-uploaded frames synced without re-sending, while local compute still keeps the retained scoring window. | `store.rs`, `BullFrameDrainUploader.swift` | ✅ |
| A-6 | **`SEND_HISTORICAL_DATA` has no "since" arg** (confirmed: empty payload). Incremental at the band is driven by the `historicalDataResult` ACK advancing the read pointer (already implemented). Verify on-device whether the ACK actually advances across sessions. | BLE | ⬜ |

## Phase B — Gap integrity

| # | Task | Where | Status |
|---|------|-------|--------|
| B-1 | **Sequence continuity per revision** — track last sequence per packet revision during a burst; record missing ranges. | `BullBLEClient+*` | ⬜ |
| B-2 | **Gap re-request** — re-pull flagged ranges before declaring a window complete. | BLE | ⬜ |

## Phase C — Local retention and backup pruning

| # | Task | Where | Status |
|---|------|-------|--------|
| C-1 | **Keep local data needed for scoring.** On-device compute reads from SQLite, so pruning must not remove data inside the scoring window. | `store.rs`, pipeline, `BullFrameDrainUploader.swift` | ✅ |
| C-2 | **Bound retained history.** `BullFrameDrainUploader.syncedPruneCutoff` retains 16 days locally and only prunes eligible synced data outside that window. | drain worker | ✅ |

## Phase D — Optional cloud services

| # | Task | Where | Status |
|---|------|-------|--------|
| D-1 | **BullAPI remains optional.** Keep Apple-authenticated cloud paths for Coach AI proxying and opt-in backup/restore. | BullAPI | ✅ |
| D-2 | **Backup upload is not compute.** Uploading frames can preserve data remotely, but the app must not wait for server parsing/recompute before showing local scores. | BullAPI, app sync | ✅ |
| D-3 | **Coach AI uses app summaries.** Coach surfaces should explain missing data using the same local metric summaries shown in the app. | Coach UI/API | ✅ |

## Historical note

The 2026-06-18 version of this document described a thin-client migration where
the server ran the parser and all compute, and the app read scores back through
`/v1/data/*`. That plan is preserved in git history, but it is no longer the
current architecture. The active direction is local-first on-device compute with
optional cloud backup.

## Notes

- Verify after implementation units: `cargo build && cargo test --no-fail-fast`,
  `git grep -i goose` empty, build + install on device.
- Do not weaken the data boundary: physiological metrics come from the connected
  device's own sensors, not third-party health stores.
