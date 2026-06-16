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
| A-2 | **`device_timestamp` on `raw_evidence`** (migration) — the packet's own timestamp_seconds, extracted at capture. Backfill best-effort from existing decoded rows. | `store.rs` | ⬜ |
| A-3 | **Structural dedup** — unique index on `(device_timestamp, packet_type)` (or content `sha256`); inserts become `INSERT OR IGNORE`. Re-sends are harmless no-ops. | `store.rs` | ⬜ |
| A-4 | **Watermark getter** — `historical_watermark()` = `MAX(device_timestamp)`; bridge method. | `store.rs`, `bridge.rs` | ⬜ |
| A-5 | **Skip-already-synced on receipt** — drop historical frames with `timestamp <= watermark` before write/process; remove the % plumbing (`historicalSyncProgressFraction` et al.). | `BullBLEClient+*` | ⬜ |
| A-6 | **Probe `SEND_HISTORICAL_DATA` "since" arg** — if the strap accepts a start timestamp/sequence, pass the watermark to cut the re-pull at the source. If not, dedup + skip still keep state correct. | BLE | ⬜ |

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
| D-1 | **Run `bull-core` server-side** (WASM-in-Bun or native sidecar). | BullAPI | ⬜ |
| D-2 | **Parse pipeline** — bundle → decode → compute → write Postgres result tables → flip `upload_bundles.status` pending→parsed. Idempotent re-parse. | BullAPI | ⬜ |
| D-3 | **`GET /v1/data/high-watermark?device_id=…`** → `{ processed_through_ts }`; advance after a bundle is parsed. | BullAPI | ⬜ |
| D-4 | **Phone uploads only the delta** above the server watermark; prune locally after 2xx. | drain worker | ⬜ |

## Notes

- A-1..A-5 directly fix the "150k packets every sync" UX; they are reversible and
  need zero server change.
- D folds in the metrics-accuracy work (server-side, one parser).
- Verify after each unit: `cargo build && cargo test --no-fail-fast`,
  `git grep -i goose` empty, RE sweep clean, build + install on device.
