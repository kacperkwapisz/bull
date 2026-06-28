# Sleep end-to-end fix — implementation plan

Status: PLAN ONLY (no code yet). Grounded in current code as of prod revision
`90354e9`.

## 0. Ground truth from the code (why sleep is broken)

Confirmed by reading the pipeline:

1. **SQLite store accumulates competing rows per day.**
   `daily_sleep_metrics` PK = `nightly_sleep_id = "nightly-sleep.{start_unix}"`
   (`Rust/core/src/bridge.rs` `persist_nightly_sleep_record`, ~6884; schema
   `Rust/core/src/store.rs:1400`). A recompute that picks a *different* start
   minute writes a **new** row instead of replacing the prior winner for that
   `date_key`. Old-algorithm rows (`sleep_window_segmented_to_most_recent_night`)
   therefore survive alongside new ones (`sleep_window_segmented_to_best_candidate`).

2. **`sleep.list_nightly` returns every row, newest-first, no per-day dedupe.**
   `store.list_daily_sleep_metrics(limit)` (`store.rs:4080+`) →
   `sleep_list_nightly_bridge` (`bridge.rs:6913`). iOS renders all of them.

3. **`export_curated` leaks the stale row into Postgres.**
   `export_curated_bridge` (`bridge.rs:5371`) maps **all** sleep rows →
   `computeUserStore` (`parse-bundle.ts`) feeds them to `ingestMetrics`, which
   upserts `daily_sleep` by `(userId, day)` (`metrics-ingest.ts:149`, unique
   index `daily_sleep_user_day_uq`). With multiple rows sharing a `day`, the
   **last write wins** — and because the list is newest-first, the *oldest*
   (often stale 650-min) row can clobber the good one. Postgres can't dup per
   day, but it can hold the wrong row.

4. **Candidate scoring is too permissive for desk-sitting.**
   `select_sleep_cluster` (`metric_features.rs:5461`): accept threshold `0.45`,
   weights `0.30*low_motion + 0.30*hr + 0.25*timing + 0.15*duration`. Low-motion
   alone contributes 0.30 and timing can contribute up to 0.25 even with no HR
   dip, so a long, still, well-timed evening desk session can clear 0.45 with
   `hr_score = 0`. HR dip is computed vs the window's own median (`hr_baseline`),
   which collapses when the whole window is sedentary.

5. **Recompute never clears stale rows first.**
   `computeUserStore` calls `sleep_score_from_features` with
   `persist_nightly:true` but never calls `clear_daily_sleep_metrics`. The
   `sleep.clear_cached_scores` bridge (`bridge.rs:7841`) and
   `POST /v1/data/sleep/recalculate` (`routes/data.ts:492`) exist but are
   manual and clear *everything* (no targeted rebuild).

Files in play:
- Algorithm: `Rust/core/src/metric_features.rs` (`select_sleep_cluster`,
  `sleep_window_feature`, `run_sleep_feature_score_report_for_store`).
- Persistence: `Rust/core/src/bridge.rs` (`persist_nightly_sleep_record`,
  `sleep_list_nightly_bridge`, `export_curated_bridge`,
  `sleep_clear_cached_scores_bridge`), `Rust/core/src/store.rs`
  (`upsert_daily_sleep_metric`, `list_daily_sleep_metrics`,
  `clear_daily_sleep_metrics`, schema).
- Server compute: `BullAPI/src/services/parse-bundle.ts`,
  `BullAPI/src/services/data-query.ts`, `BullAPI/src/services/metrics-ingest.ts`,
  `BullAPI/src/routes/data.ts`.
- iOS render: `BullSwift/HealthDataStore+Sleep.swift`.
- Tests: `Rust/core/tests/metric_features_tests.rs`, `bridge.rs` unit tests.

## Invariants (hold across every phase)

- **I1 — One winner per `date_key`.** At any moment, at most one
  `daily_sleep_metrics` row per `date_key` is surfaced to `sleep.list_nightly`
  and to `export_curated`. (Either enforced by storage key or by a dedupe view.)
- **I2 — Replace, never accumulate.** Recomputing a day supersedes the prior
  winner for that day instead of adding a competitor.
- **I3 — Physiology-gated sleep.** A window may be reported as sleep only with
  supporting physiology (HR dip vs an out-of-window awake baseline, or
  HRV/respiration), OR very strong stillness+timing for the main overnight
  window. Low motion alone is never sufficient.
- **I4 — Honest unknown.** When evidence is insufficient, emit no sleep row (or
  a `needs_review`/`unknown` row), never a fabricated window. (Aligns with the
  AGENTS data-integrity rule.)
- **I5 — Server is the only compute.** All classification stays in `Rust/core`
  + BullAPI. iOS only renders, dedupes for display, and may collect labels.
- **I6 — Provenance preserved.** Every emitted row carries algorithm id/version,
  confidence, reason codes, and the evidence window. No silent overwrites.
- **I7 — No raw-evidence loss.** Recompute/backfill rebuilds from retained raw
  frames or object-storage bundles; pruning only after durable projection
  (already true in `parse-bundle.ts`, keep it).

---

## Phase 1 — Persistence/data-model cleanup (lowest risk, unblocks everything)

Goal: make storage enforce I1/I2 so even the *current* algorithm stops
producing duplicates and stale Postgres rows. Do this first so later algorithm
changes can be verified cleanly.

### 1a. Key nightly sleep by resolved day, not by start timestamp
- `Rust/core/src/store.rs`: keep `nightly_sleep_id` column but change the
  **uniqueness contract** to `(date_key)` for the *resolved winner*. Two options:
  - **Option A (preferred):** add `UNIQUE(date_key)` semantics via an upsert that
    targets `date_key`. Requires either making `date_key` the conflict target
    (add `UNIQUE` index on `date_key`) or doing delete-by-day + insert in one txn.
  - Option B: keep PK on `nightly_sleep_id` but derive id as
    `nightly-sleep.{date_key}` so re-resolution overwrites in place.
  - Decision point: Option B is the smallest change and directly satisfies I2.
    Risk: a day with two genuine sleeps (overnight + later nap) needs a separate
    row. Handle naps with a distinct id namespace, e.g.
    `nightly-sleep.{date_key}` for main + `nap.{date_key}.{ordinal}` for naps,
    and a `sleep_kind` column (`main` | `nap`).
- Add column `sleep_kind TEXT NOT NULL DEFAULT 'main'` and (if Option B)
  `superseded_at TEXT NULL` for soft-supersede.
- `persist_nightly_sleep_record` (`bridge.rs`): compute id from `date_key` +
  `sleep_kind`; delete/replace prior main winner for the day in one transaction.

### 1b. Dedupe at read in `list_daily_sleep_metrics`
- `store.rs::list_daily_sleep_metrics`: return at most one `main` row per
  `date_key` (highest `confidence`, then newest `updated_at`) plus accepted naps.
  Implement via `ROW_NUMBER() OVER (PARTITION BY date_key, sleep_kind ORDER BY
  confidence DESC, updated_at DESC)`-style selection (SQLite: correlated
  subquery or `GROUP BY` with max). This is a belt-and-suspenders guard for I1
  even if 1a leaves legacy rows behind.

### 1c. Filter stale/old-algorithm rows out of the surfaced set
- Treat rows whose `provenance`/`quality_flags` contain
  `sleep_window_segmented_to_most_recent_night` (old algorithm) as non-surfaced
  unless no newer row exists. Prefer: on recompute, **delete** old-algorithm
  rows for the recomputed day (see Phase 3 targeted clear).

### 1d. `export_curated` must export the deduped winners only
- `export_curated_bridge`: source sleep rows from the deduped read path (1b), so
  Postgres `daily_sleep` receives exactly one row per day. This closes the
  newest-first / last-write-wins clobber.

### 1e. Make Postgres upsert deterministic
- `metrics-ingest.ts`: when multiple sleep rows for the same `day` somehow still
  arrive, pick the highest-confidence/most-recent before upsert (defensive),
  rather than relying on array order. Keep `(userId, day)` unique.

Verification (Phase 1):
- New `store.rs` unit test: two computes for same night with different start
  minutes → exactly one surfaced row, latest wins.
- `cargo test` green for store + bridge.
- Local sidecar run on a copied store: `sleep.list_nightly` returns ≤1 main row
  per `date_key`.

---

## Phase 2 — Server-side sleep algorithm redesign (`metric_features.rs`)

Goal: satisfy I3/I4. Port the *ideas* from the public `SleepStager.swift`
(stillness spine, HR confirmation, nap guard, morning-stillness guard,
off-wrist/HR-gap guard, confidence/quality flags) into Rust. No hard max-cluster
cap (explicitly rejected). Keep the existing report/field shapes
(`SleepWindowFeature`, `SleepStageSegmentFeature`) so downstream/iOS is stable.

### 2a. Candidate generation (replace cluster-by-gap-only)
- Build a per-minute epoch series over the feature window (motion intensity + HR
  + optional HRV/respiration coverage).
- Candidate windows = contiguous low-motion "stillness spine" segments allowing
  short interruptions (existing 120-min gap stays as a *merge* tolerance, not the
  only structure). Generate **multiple** candidates (overnight + possible naps).
- Keep `feature_window_start_iso` bounds already wired in `bridge.rs` +
  `parse-bundle.ts`.

### 2b. Sleep/wake epoch classification
- Per-epoch sleep probability from motion + HR relative to an **out-of-window
  awake baseline HR** (not the window's own median — that is the bug behind
  desk-sitting). Compute awake baseline from high-motion epochs across the
  surrounding day(s) / retained window.
- Apply existing `SLEEP_HR_WAKE_REFERENCE_MAX_FRACTION` against that external
  baseline. Smooth with the existing `MIN_SMOOTHED_SLEEP_STAGE_DURATION_MINUTES`.

### 2c. Main sleep vs nap detection
- Main sleep: longest qualifying window near the personal sleep midpoint
  (`target_midpoint_minutes_since_midnight`), span ≥ overnight threshold.
- Nap: short qualifying window (≥ `SLEEP_WINDOW_MIN_SPAN_MINUTES`, e.g. 30 min)
  that requires **strong HR dip** vs awake baseline (keep/strengthen
  `sleep_candidate_nap_without_strong_hr_dip` penalty). Emit naps as separate
  `sleep_kind = nap` rows (ties into Phase 1a).

### 2d. HR/HRV/respiration support
- Require physiology support for acceptance (I3):
  - HR dip vs external awake baseline ≥ threshold, OR
  - HRV/respiration evidence consistent with sleep, OR
  - (main window only) very strong stillness + on-target timing + sufficient
    duration, with a clearly lower confidence and a `low_physiology_support`
    flag.
- When HR is fallback/sparse/missing: do **not** accept low-motion-only windows
  off-target; mark `stage_hr_unavailable` and reduce confidence; prefer I4.

### 2e. Desk-sitting false-positive rejection
- Add explicit guards:
  - `desk_sitting_suspected`: long low-motion window, daytime/evening timing,
    HR within awake-baseline band (no dip) → reject as sleep.
  - Morning-stillness guard: quiet wakeful lie-in after wake shouldn't extend the
    window (cap extension when HR has already risen to awake band).
  - Off-wrist / HR-gap guard: long HR gaps or off-wrist motion signature →
    don't classify the gap as deep sleep; flag and shrink.

### 2f. Confidence / reason / provenance output
- Replace ad-hoc flags with a structured set on `SleepWindowFeature.provenance`:
  - `confidence_0_to_1`, `acceptance_reason`, `rejection_reasons[]`,
    `evidence: {hr_dip_fraction, awake_baseline_bpm, motion_mean, timing_dev,
    hr_coverage, hrv_available}`, `algorithm_id/version`.
- Keep existing quality-flag strings additive for backward-compat, add
  `sleep_algorithm = bull.sleep.v2`.

### 2g. Conservative unknown/rest handling
- If no candidate clears acceptance, return `None` (no row) OR a `needs_review`
  row with `confidence < threshold` and `acceptance_reason = "unknown_rest"`.
  Decision point: emit a `needs_review` row (so the day isn't silently blank in
  the UI) but mark it so iOS shows a "needs review / unavailable" state (I4).

### 2h. Versioning
- Bump `stage_model_version` / introduce `algorithm_id = bull.sleep.v2` so old
  rows are trivially distinguishable for Phase 3 cleanup and verification.

Verification (Phase 2): see Phase 5 test matrix. All new unit tests in
`tests/metric_features_tests.rs` plus `bridge.rs` sleep tests must pass.

---

## Phase 3 — Recompute / backfill (rebuild affected days safely)

Goal: rebuild from raw without data loss (I7), purge old-algorithm rows, verify
no stale flags remain.

### 3a. Targeted clear (replace the all-or-nothing clear)
- `Rust/core/src/store.rs`: add `clear_daily_sleep_metrics_for_days(days: &[str])`
  and/or `clear_daily_sleep_metrics_with_algorithm_before(version)`.
- `bridge.rs`: extend `sleep_clear_cached_scores` args to accept optional
  `date_keys` and/or `algorithm_id_below`. Default behavior unchanged when no
  filter passed.

### 3b. Recompute order in `computeUserStore` (`parse-bundle.ts`)
- Before persisting recomputed nights: clear old-algorithm rows for the days in
  scope (the backfill day set already computed). Then run
  `sleep_score_from_features` with `persist_nightly:true` (v2). Because Phase 1a
  keys by day, re-resolution replaces in place.
- Keep prune-after-projection ordering intact (I7). Do **not** prune raw before
  sleep persists.

### 3c. Full historical rebuild path (one-off, controlled)
- For days older than the retained raw window, rebuild from object-storage
  bundles: re-import bundle frames into a scratch store and recompute, OR accept
  that pre-retention days keep their last computed v2 value. Decision point:
  scope the one-off rebuild to the retained window + any days still showing
  old-algorithm flags; document that deep history is best-effort.

### 3d. Production-safe execution
- Prod DB is READ-ONLY for investigation. Any mutation (clear + recompute) must
  be an **explicit, approved** step. Sequence:
  1. Snapshot current `daily_sleep` (Postgres) and a copy of one user's SQLite
     store for before/after diffing.
  2. Run recompute for a **single test user** first (the dev's own account).
  3. Verify (Phase 5 queries) before any broader run.

Verification (Phase 3):
- After recompute, `sleep.list_nightly` for affected days contains **zero** rows
  with `sleep_window_segmented_to_most_recent_night` and zero with
  `algorithm_id < bull.sleep.v2`.
- No drop in raw frame counts (`debug.db_overview` before/after).

---

## Phase 4 — iOS display fixes (`HealthDataStore+Sleep.swift`)

Goal: render-only correctness (I5). Even with server fixed, harden the client.

### 4a. Dedupe by `date_key`
- `nightlySleepRecords(from:)`: group by `dateKey`, keep the highest-confidence
  (then newest `startTimeUnixMs`) main record per day. Naps shown separately.

### 4b. Suppress stale/old-algorithm rows
- Drop rows whose quality flags include `sleep_window_segmented_to_most_recent_night`
  or whose `algorithm_id` is below `bull.sleep.v2` when a newer row exists for the
  day. (Read `algorithm_id`/flags through the row payload; extend
  `NightlySleepRecord` if needed.)

### 4c. Confidence / needs-review states
- Extend `NightlySleepRecord` + `PrimarySleepDetail` with `confidence` (already
  present) and an `needsReview`/`unavailable` flag derived from
  `acceptance_reason`. UI: show honest "needs review / not enough data" instead
  of a number when below threshold (I4).

### 4d. Keep computation server-side
- No local classification. Only grouping/filtering/formatting. (I5.)

Verification (Phase 4): unit tests for `nightlySleepRecords` dedupe + filter
with fixtures containing duplicates, a stale old-algorithm row, and a
needs-review row. Manual: app shows one row per night, no 650-min ghost.

---

## Phase 5 — Tests & validation

### 5a. Rust unit/regression matrix (`tests/metric_features_tests.rs`)
Each as a deterministic fixture (motion + HR epoch series), asserting accept/
reject + window bounds + reason codes:
1. **Normal night** (00:30→07:45, HR dip present) → one main window, high conf.
2. **Desk sitting** (long still evening, HR at awake band, no dip) → **rejected**
   (`desk_sitting_suspected`), no sleep row.
3. **True nap** (30–45 min daytime, clear HR dip) → one `nap` row, no main.
4. **Morning stillness** (awake lie-in after wake, HR risen) → window does not
   extend into the lie-in.
5. **Off-wrist / HR gap** (long HR gap mid-window) → flagged, gap not scored as
   deep sleep, window shrinks.
6. **Long valid sleep** (>12h recovery sleep, strong evidence) → accepted (no
   hard cap), high conf.
7. **Missing HR** (motion only) → off-target low-motion **not** accepted;
   on-target main window accepted only with reduced conf + `stage_hr_unavailable`.
8. **Sparse data** (few epochs) → `None`/needs_review, never fabricated.

### 5b. Persistence/dedupe tests (`store.rs`, `bridge.rs`)
- Recompute same night twice with different starts → one surfaced row (I2).
- `export_curated` emits one sleep row per day (I1).
- Targeted clear removes only the requested days/old algorithm.

### 5c. Swift tests
- `nightlySleepRecords` dedupe + stale filter + needs-review mapping.

### 5d. Production verification (read-only first)
- Postgres:
  - `SELECT day, sleep_score, total_sleep_minutes FROM daily_sleep WHERE user_id=$1 ORDER BY day DESC LIMIT 14;`
    → no implausible 650-min/day, one row per day (guaranteed by unique index;
    check values are v2).
- Sidecar (`sleep.list_nightly` via `/v1/data/query`):
  - assert ≤1 main row per `date_key`; assert no
    `sleep_window_segmented_to_most_recent_night`; assert `algorithm_id` = v2.
- `debug.db_overview`: raw frame counts unchanged pre/post (I7).

### 5e. Deploy / rollback
- Deploy order: Phase 1 (storage) + Phase 2 (algorithm) ship together in the
  Rust core build, then BullAPI changes (Phase 1d/1e/3). Deploy core to server,
  trigger server-side recompute (per AGENTS thin-client/server-compute rule).
- Rollback: keep `bull.sleep.v1` code path behind the existing algorithm_id
  selection so reverting `algorithm_id` restores prior behavior; storage changes
  are additive (new columns nullable/defaulted) so they are backward-safe. Keep
  the prior prod revision (`90354e9…`) pinned for fast redeploy.
- iOS (Phase 4) ships independently; it is defensive and safe with old or new
  server output.

### Build/verify commands
- Rust: `cd Rust/core && cargo build && cargo test` (74 suites expected green +
  new sleep tests).
- BullAPI: typecheck/tests per BullAPI tooling.
- Swift: build `BullSwift` in Xcode (cannot build from cargo alone).

---

## Suggested execution order (gated, stop for approval before prod mutation)

1. Phase 1 (storage) + tests — local only.
2. Phase 2 (algorithm v2) + tests — local only.
3. Phase 4 (iOS) + tests — local only.
4. Phase 3a/3b (targeted clear + recompute wiring) + tests — local only.
5. **STOP** — present results, get explicit approval before any prod recompute.
6. Phase 3d: single test user recompute → verify (5d) → approval → broader run.
7. Deploy core + BullAPI; trigger server recompute; verify; ship iOS.
