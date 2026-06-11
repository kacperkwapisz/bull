# Remaining Data TODO

Runtime rule: do not show fabricated metric values. A surface may show live, local, bridge-derived, or unavailable data only.

## Sleep

- [ ] Import primary sleep directly from band packets and persist nightly sleep records locally.
- [ ] Populate sleep stage timeline from band-derived sleep stage output.
- [ ] Build sleep trend history for score, time asleep, REM, deep, HR dip, sleep time, wake time, and latency.
- [ ] Add real target sleep amount input and persist it.
- [ ] Compute Sleep Needed from target sleep, sleep debt, recent sleep history, and planned wake time.
- [ ] Compute Sleep Bank from target sleep amount and stored nightly sleep history.
- [ ] Derive sleep insights from actual sleep score components and confidence fields.

## Recovery

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
