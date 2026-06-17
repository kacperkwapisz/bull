# Sync architecture — incremental pull + thin client

Goal: the phone is a **pipe + viewer**, the server is the **system of record**.
Re-syncing must be cheap and idempotent; the local DB stays small and bounded.
This supersedes the on-device decode/parse-everything approach (the source of the
earlier storage blowup) for the **bulk historical path**. Live/realtime data
keeps a lightweight on-device decode for immediate display.

## Principles

- **Idempotent by construction.** Re-receiving the same record is a no-op. Dedup
  on a semantic key (`device_timestamp` [+ `packet_type`]), not on transport ids.
- **Derive the resume point, don't build a subsystem.** The "what's already
  synced" watermark is `MAX(device_timestamp)` over stored raw records — a query,
  not a bespoke cursor store.
- **Two watermarks, two hops.**
  - strap → phone: `MAX(device_timestamp)` of buffered raw → request only newer.
  - phone → server: server reports `processed_through_ts`; phone uploads only the
    delta, then prunes locally.
- **Raw in, compute out.** Phone buffers raw frames; the single Rust parser runs
  server-side. History can be re-derived by re-parsing on the server without
  re-pulling the band.
- **Integrity over guesses.** Track sequence continuity per revision; detect and
  re-request gaps instead of silently skipping. Honest "unavailable" over made-up
  values.

## Phase A — Local incremental + UI honesty (phone only, no server change)

| # | Task | Where | Status |
|---|------|-------|--------|
| A-1 | **Drop the historical % / ETA progress bar.** It has no stable total and misled more than it helped. Replace with an honest indeterminate state: "Syncing — N packets". Keep packet count as telemetry. | `DeviceView.swift` | ⬜ |
| A-2 | **`device_timestamp` on `decoded_frames`** (schema v17) — the data packet's own `timestamp_seconds`, populated at insert; indexed `(packet_type, device_timestamp)`. | `store.rs` | ✅ |
| A-3 | ~~Structural content/timestamp dedup~~ **Rejected.** Content (`sha256`) dedup breaks the `evidence_id` pipeline (a deduped raw insert orphans the following `decoded_frames` insert → FK violation) and would drop byte-identical realtime samples. A `(device_timestamp, packet_type)` unique index is lossy for sub-second realtime. Dedup is done by **skip-on-receipt** (A-5) instead. | — | ✅ (decided) |
| A-4 | **Watermark getters** — `historical_watermarks()` (`MAX(device_timestamp)` per `packet_type`) + `historical_watermark_max()`; bridge `store.historical_watermarks`. | `store.rs`, `bridge.rs` | ✅ |
| A-5 | **Skip-already-uploaded re-pulls** (the dedup mechanism) — persistent `historical_sync_watermark` (schema v18, `sync_state` table, survives pruning); each drain marks already-uploaded frames (`device_timestamp <= watermark`) synced without re-uploading, then advances the watermark after a confirmed upload. A band re-pull is never resent. (Done at the drain boundary rather than the receive path; receive-path skip to also avoid local re-decode is a later battery optimization.) | `store.rs`, `BullFrameDrainUploader.swift` | ✅ |
| A-6 | **`SEND_HISTORICAL_DATA` has no "since" arg** (confirmed: empty payload). Incremental at the band is driven by the `historicalDataResult` ACK advancing the read pointer (already implemented). Verify on-device whether the ACK actually advances across sessions. | BLE | ⬜ |

## Phase B — Gap integrity

| # | Task | Where | Status |
|---|------|-------|--------|
| B-1 | **Sequence continuity per revision** — track last sequence per packet revision during a burst; record missing ranges. | `BullBLEClient+*` | ⬜ |
| B-2 | **Gap re-request** — re-pull flagged ranges before declaring a window complete. | BLE | ⬜ |

## Phase C — Stop on-device parse for bulk path (storage win)

| # | Task | Where | Status |
|---|------|-------|--------|
| C-1 | **Stop writing `parsed_payload_json` / `decoded_frames` for historical bulk** — buffer raw only. Keep a minimal decode for the live preview. | `store.rs`, pipeline | ⬜ |
| C-2 | **Shrink retained window** — once compute is server-side, the local cap can drop well below 32 MB. | drain worker | ⬜ |

## Phase D — Server: parse + watermark read-back

| # | Task | Where | Status |
|---|------|-------|--------|
| D-1 | **Run `bull-core` server-side** — native `bull-bridge-serve` sidecar (newline JSON over stdio), bundled in the API image via a multi-stage musl build (root build context). | BullAPI, Dockerfile | ✅ |
| D-2 | **Parse pipeline** — bundle → import frames → `run_pipeline` → sleep score (persist) → recovery/strain/stress scores → fold into Postgres result tables (`dailySleep`/`vitalsDaily`/`dailyRecovery`/`dailyStrain`/`dailyStress`) → flip `pending→parsed`. Idempotent. **Auto-triggered** by a fire-and-forget pending-sweep on upload. | BullAPI | ✅ |
| D-3 | **`GET /v1/data/high-watermark`** → newest parsed `timeframeEnd`; read APIs (`/v1/data/recovery|sleep|strain|stress|energy|vitals|spo2`) already serve the result tables. | BullAPI | ✅ |
| D-4 | **Phone uploads only the delta** above the server watermark; **drain-to-empty** after 2xx. Skip-already-uploaded (A-5) covers the upload dedup; switching the cap to drain-to-empty waits until the app reads from the server (else local screens blank). | drain worker | ⬜ (post-deploy) |
| D-5 | **App reads results from the server** and **drops on-device compute** — the device-side thin-client switch. Server read APIs exist; this must be validated against a deployed, populated server, so it is the post-deploy phase. | `HealthDataStore+*`, views | ⬜ (post-deploy) |

## Notes

- A-1..A-5 directly fix the "150k packets every sync" UX; they are reversible and
  need zero server change.
- **A-3 learning:** Bull stores raw (`evidence_id` PK) then decoded (`frame_id`,
  FK→raw) as a 1:1 pair. Structural content dedup collapses that pair and orphans
  the decoded insert; and byte-identical realtime frames are legitimately
  distinct. So dedup must happen *before* the pair is written — i.e.
  skip-on-receipt against the watermark — not via a DB unique constraint.
- D folds in the metrics-accuracy work (server-side, one parser).
- Verify after each unit: `cargo build && cargo test --no-fail-fast`,
  `git grep -i goose` empty, RE sweep clean, build + install on device.
