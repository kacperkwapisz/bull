# Storage re-platform — thin client + upload-drain (plan + Phase 0 checklist)

Tracked working-notes doc (like TIGER-PORT-INVENTORY.md; AGENTS.md is the
gitignored one). Legend: ⬜ pending · 🟡 in-progress · ✅ done.

---

## The finding

A real WHOOP 5.0 historical sync produced **155,029 captured frames** and a
**1.7 GB local DB + 1.3 GB un-checkpointed WAL** (~3 GB). The app was SIGKILLed
(jetsam / OOM) because the metrics pipeline loads the entire history
(`decoded_frames_between("0000","9999")`) into RAM and re-scans it.

Root cause (confirmed in code): Bull persists **every** frame forever —
`raw_evidence` + `decoded_frames` (256 MB `payload_hex` + **626 MB**
`parsed_payload_json`) — and:

- the retention cap measures only `LENGTH(payload_hex)` → **blind to the 626 MB
  JSON column**;
- compaction only blanks `payload_hex`, **never deletes rows**;
- it runs on app-lifecycle, **not after sync**; the **WAL is never checkpointed**.

The frame breakdown (155 k): `REALTIME_RAW_DATA` 86 k, `HISTORICAL_DATA` 64 k
(of which only k18 = 2,190, and only 31 produced gravity — a separate
decode-coverage gap to revisit later).

## Target architecture (agreed): thin client + server brain

The equilibrium mature wearables use (and what the decompiled WHOOP Android app
does: buffer raw → upload in batches → prune after ack → parse server-side):

- **Phone = dumb pipe + viewer + live preview.** It uploads raw frames, pulls
  *all* historical/derived data from the server, and computes *only* the
  ephemeral live tile locally. **On-device historical computation is retired.**
- **Realtime = disposable on-device preview.** Same parser, ephemeral, not a
  system of record. The band also records it, so it returns authoritatively in
  the historical sync — no need to persist the preview.
- **Server = system of record.** `bull-core` (pure Rust, already builds server
  binaries) parses the R2 raw bundles → results in Postgres → served to the app,
  which caches them for offline.
- **One parser, two targets** (phone live-lib + server binary) — never two
  implementations. `bull-core` decode/extract work (Tier 1 + Tier 2) is **reused
  server-side**, not rewritten.

Justified on Bull's own engineering needs (bounded device storage, no jetsam,
scalable history/re-processing). RE-scrub policy (TIGER-PORT-INVENTORY.md) still
applies to anything entering the tree.

## The upload-drain model (core of the storage fix)

Retention is tied to **upload success**, not a time guess:

> phone **bundles un-uploaded frames → `POST /v1/data/uploads`** → server stores
> the raw bytes in R2 (`status = pending`, checksum-deduped) → **on 2xx the phone
> deletes those frames** → repeat. Retry/backoff on failure (never delete before
> a confirmed upload).

Delete-after-success is safe because the server holds the durable raw copy in R2
*before* the phone drops it. This self-bounds local storage and makes the server
the system of record with zero data loss.

## BullAPI is already scaffolded for this

- `POST /v1/data/uploads` — multipart `bundle` file → stored in **R2**
  (`storageKey`), recorded in **`upload_bundles`** with `checksum` (dedupe),
  `byteSize`, `timeframeStart/End`, and **`status: pending | parsed | failed`**
  + `parsedAt` / `parseError`. The `pending → parsed` lifecycle exists
  specifically for server-side parsing.
- `GET /v1/data/uploads/:id/download` — presigned raw-bundle fetch.
- `GET /v1/data/recovery` · `/sleep` · `/spo2` · `/strain` · `/metrics` ·
  `/summary` — **results-fetch API already exists**, backed by
  `dailyRecovery / dailySleep / dailyStrain / dailyStress / dailyEnergy /
  vitalsDaily / spo2Samples` tables.

So the drain needs **no server change** — it reuses `POST /v1/data/uploads`.
Server-side parsing (filling those result tables from pending bundles) is the
next phase.

## Untangle: two raw stores on the phone today

- **Spool files** (`BullSpoolArchiveUploader`): overnight raw spools already
  zipped, uploaded to `/v1/data/uploads`, and deleted locally. ✅ already drains.
- **`decoded_frames` / `raw_evidence` tables**: the 1.7 GB hoard, **not drained.**

Phase 0 unifies these so each frame is **bundled → uploaded → deleted once** —
not spooled-and-uploaded *and* separately hoarded in the DB.

## Migration phases

| Phase | What | Status |
|-------|------|--------|
| **0** | **Upload-drain + WAL checkpoint** — frames bundle → upload → delete on 2xx; unify with spool path; WAL truncate. Stops the crash; permanent plumbing (not interim). On-device compute untouched (temporary). | 🟡 |
| 1 | **Server parses pending bundles** with `bull-core` → fills result tables. | ⬜ |
| 2 | App **pulls results from `GET /v1/data/*`** + caches; **delete on-device historical compute**. | ⬜ |
| 3 | Thin realtime preview path (the deferred FFI-lag fix), ephemeral, same parser. | ⬜ |
| 4 | Cleanup → phone = live buffer + results cache + upload queue only. | ⬜ |

---

## Phase 0 — Upload-drain + WAL checkpoint (branch `feat/phase0-storage-stabilize`)

Baseline at branch: **854 Rust tests / 0 failed**; 1.7 GB device DB **wiped**
(clean reinstall).

> ⚠️ Do **not** run a full historical re-sync on device until P0-2/P0-3 land — it
> will re-bloat and crash again. Short syncs are fine.

| # | Task | Where | Status |
|---|------|-------|--------|
| **P0-1** | **WAL checkpoint** — `wal_checkpoint(TRUNCATE)` in `store.maintain`; verify/adjust `wal_autocheckpoint` so the WAL can't reach 1.3 GB | `store.rs` | ⬜ |
| **P0-2** | **Frame drain queue** — track un-uploaded frames; build a bundle of N un-uploaded frames; on confirmed upload (2xx), **delete those rows**; retry/backoff on failure; reconcile with the spool path so each frame uploads once | `store.rs`, `bridge.rs`, `BullSwift` | ⬜ |
| **P0-3** | **Trigger drain after capture/sync** (+ app background); run WAL checkpoint after a drain pass | `BullSwift` | ⬜ |
| **P0-4** | **Tests** — drain selects/bundles/deletes correctly; nothing deleted before success; idempotent (checksum dedupe); WAL truncates; DB stays bounded under simulated high volume | `store.rs` tests | ⬜ |
| **P0-5** | **On-device verify** — re-sync a real large history; confirm DB stays small, WAL bounded, frames drain to R2, no crash | device | ⬜ |
| **V** | After each unit: `cargo build && cargo test --no-fail-fast`; `git grep -i goose` empty; RE sweep clean; build + install on iPhone | — | ⬜ |

On-device historical compute stays **untouched** (temporary stand-in on the
now-small drained DB) until Phase 2 deletes it.

### Decisions

- **Bundle size — by compressed BYTES** (decided). Target ~1–2 MB/bundle, capped
  under the server `MAX_BUNDLE_BYTES` (confirm the exact cap when wiring P0-2).

### Phase 0 commits

```
(none yet)
```
