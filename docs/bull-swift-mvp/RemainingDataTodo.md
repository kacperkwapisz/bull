# Remaining Data TODO

Runtime rule: do not show fabricated metric values. A surface may show live, local, bridge-derived, or unavailable data only.

## Sleep

- [x] Import primary sleep directly from band packets and persist nightly sleep records locally.
      (V24 history HR + gravity-derived motion now feed the local sleep score;
      historical-sync frames persist into `decoded_frames`; nightly windows are
      segmented and persisted into `daily_sleep_metrics` via `sleep.list_nightly`.)
- [x] Populate sleep stage timeline from band-derived sleep stage output.
- [~] Build sleep trend history for score, time asleep, REM, deep, HR dip, sleep time, wake time, and latency.
      (Sleep Score trend is wired to persisted nightly history; the remaining
      sub-trends still need to be sourced from `daily_sleep_metrics`/stage minutes.)
- [ ] Add real target sleep amount input and persist it.
- [ ] Compute Sleep Needed from target sleep, sleep debt, recent sleep history, and planned wake time.
- [ ] Compute Sleep Bank from target sleep amount and stored nightly sleep history.
- [ ] Derive sleep insights from actual sleep score components and confidence fields.
- [ ] Phase 0 validation: capture a real overnight historical sync and confirm the
      V24 gravity-delta motion proxy tracks sleep/wake; calibrate the sleep stage
      thresholds (currently tuned for the signed-i16 amplitude scale) against it.

## Recovery

- HRV scale verification: `metrics.rr_hr_consistency` (Rust `rr_hr_consistency.rs`) now
  proves the V24 history `rr_intervals_ms` field against the band's own co-located HR
  (`60000 / mean(RR_ms)` must reproduce reported HR), using device-internal data only
  (no official labels). Run it over owned worn captures; a `verified` verdict is the
  evidence required to lift the `hrv_rr_interval_scale_unverified` blocker in
  `metric_readiness.rs` and promote the V24 RR source into the HRV pipeline
  (`hrv_plan_from_row` currently consumes only the speculative R17 i16 candidate).
- [ ] Capture worn V24 frames carrying both HR and RR, then run `metrics.rr_hr_consistency`
      until it returns `verified`; only then wire V24 RR into `run_hrv_feature_report`
      and flip the readiness `extraction_ready` for HRV inputs.
- [ ] Compute recovery score from local HRV, resting HR, respiratory rate, sleep, strain, and temperature inputs.
- [ ] Populate recovery history rows for score, HRV, resting HR, respiratory rate, SpO2, and wrist temperature.
- [ ] Resolve respiratory rate, SpO2, and wrist temperature packet semantics from band data.
- [ ] Replace manual vitals placeholders with packet-derived or user-entered values with provenance.

## Strain And Activity

- [ ] Persist activity sessions with start/end, type, HR summary, zone durations, calories, and sync status.
- [ ] Compute daily strain from activity sessions and HR load.
- [ ] Add real step count extraction from motion/history packets.
- [ ] Add calorie/energy estimator from profile, HR, movement, and activity sessions.
- [ ] Build strain trends for score, exercise duration, daytime HR, total energy, and step count.

## Stress And Energy Bank

- [ ] Persist daily stress windows instead of only computing the current day in memory.
- [ ] Add activity masking to split activity stress from non-activity stress.
- [ ] Use stored sleep windows for sleep stress instead of inferred clock windows.
- [ ] Persist Energy Bank history and compute long-range trends.
- [ ] Calibrate Energy Bank charge/drain rates against stored recovery, sleep, and activity history.

## Cardio Load

- [ ] Validate the local Cardio Load formula against multiple real workout sessions.
- [ ] Persist computed daily Cardio Load rows so charts do not recompute from raw sessions every render.
- [ ] Confirm HR zone durations from band activity metrics; keep HR fallback only as marked local estimate.
- [ ] Add migration/backfill for historical activity sessions once band import supports it.

## Algorithms, References, Calibration

- [ ] Load algorithm and reference definitions only from the Rust bridge.
- [ ] Wire reference comparisons to real captured input windows, not static benchmark payloads.
- [ ] Define and persist real calibration labels.
- [ ] Implement calibration runs with train/holdout splits from local metric history.
- [ ] Show calibration outputs only after a completed local calibration run.

## Home, Coach, More

- [ ] Make Home widgets share the same live/local/bridge/unavailable data contracts as Health.
- [ ] Ensure Coach prompts explicitly receive current provenance and missing-data states.
- [ ] Replace remaining placeholder routes with empty states or real screens.
- [ ] Remove debug preview-only strings from runtime surfaces before TestFlight builds.
