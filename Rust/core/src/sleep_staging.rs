// Sleep staging: actigraphy spine using Cole-Kripke (1992) binary wake/sleep classifier,
// extended to a 4-class (wake/light/deep/rem) hypnogram using HR + motion features.
//
// Reference: Cole, R.J. et al. "Automatic sleep/wake identification from wrist activity."
// Sleep 1992; 15(5): 461-469.
//
// This file is intentionally pure (no DB access). The bridge wrapper in bridge.rs
// calls gravity_rows_between and passes the tuples here.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Named constants — never inline these at call sites
// ---------------------------------------------------------------------------

/// Multiplicative scale factor applied to each activity count before the
/// Cole-Kripke weighted sum. 0.001 converts raw inter-sample magnitude
/// differences (g-units) to the activity index expected by Cole 1992.
pub const COLE_KRIPKE_SCALE_FACTOR: f64 = 0.001;

/// Wake threshold: D >= 1.0 → wake epoch (Cole 1992).
pub const COLE_KRIPKE_WAKE_THRESHOLD: f64 = 1.0;

/// Duration of each actigraphy epoch in minutes.
/// 30 s epochs (0.5 min) per standard actigraphy conventions.
/// WASO and SOL resolution doubles vs the previous 1-min setting.
pub const COLE_KRIPKE_EPOCH_MINUTES: f64 = 0.5;

/// Staging method emitted in every output that has at least one epoch.
pub const STAGING_METHOD_ACTIGRAPHY: &str = "actigraphy_uncalibrated";

/// Staging method emitted when the gravity window contained no rows.
pub const STAGING_METHOD_NO_IMU: &str = "no_imu_data";

// ---------------------------------------------------------------------------
// 4-class threshold constants — expose as named consts, never magic literals
// ---------------------------------------------------------------------------

/// HR percentile below which a sleep epoch is considered "deep" (low HR).
/// An epoch whose HR is at or below the session's p25 personal percentile
/// is a candidate for deep sleep (together with low motion).
pub const DEEP_HR_PERCENTILE: f64 = 0.25;

/// Maximum activity count for a "deep" sleep epoch.
/// Epochs with activity_count at or below this threshold are considered still
/// enough to be classified as deep (when HR is also low).
pub const DEEP_STILLNESS_ACTIVITY_MAX: f64 = 0.05;

/// Fractional position in the sleep period (clock proxy) at or above which
/// a sleep epoch is eligible to be classified as REM.
/// 0.4 ≈ first 40% of the night is treated as non-REM territory.
pub const REM_CLOCK_PROXY_MIN: f64 = 0.4;

/// No-REM onset guard: REM epochs within this many minutes of sleep onset
/// are reclassified as light (physiological reimposition rule a).
pub const NO_REM_ONSET_MINUTES: f64 = 15.0;

/// Minimum continuous segment duration (minutes) before reimposition merges
/// short islands into adjacent classes (physiological reimposition rule b).
pub const MIN_SEGMENT_MINUTES: f64 = 5.0;

// Cole-Kripke 7-term weighted coefficients (w[-4..+2]).
// D = (1/100) * sum_k(COEFFS[k+4] * scaled_count[epoch + offset_k])
// offsets: -4, -3, -2, -1, 0, +1, +2
const COLE_KRIPKE_COEFFS: [f64; 7] = [106.0, 54.0, 58.0, 76.0, 230.0, 74.0, 67.0];
const COLE_KRIPKE_OFFSETS: [i64; 7] = [-4, -3, -2, -1, 0, 1, 2];

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Input to the pure sleep-staging classifier.
/// `database_path` lives only in `SleepStagingBridgeArgs`; it is not needed
/// by the algorithm itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SleepStagingInput {
    pub device_id: String,
    pub sleep_start_ts: f64,
    pub sleep_end_ts: f64,
}

/// One classified 1-minute epoch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SleepEpoch {
    /// Unix timestamp (seconds) of the epoch start.
    pub ts: f64,
    /// Inter-sample magnitude-difference activity count (unit-less).
    pub activity_count: f64,
    /// "wake", "light", "deep", or "rem" (4-class); or "wake"/"sleep" (binary).
    pub stage: String,
}

/// Output of `stage_sleep` and `stage_sleep_four_class`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SleepStagingOutput {
    pub epochs: Vec<SleepEpoch>,
    /// Either `STAGING_METHOD_ACTIGRAPHY` or `STAGING_METHOD_NO_IMU`.
    pub staging_method: String,
    /// Fraction of epochs classified as wake (0.0 when no epochs).
    pub wake_fraction: f64,
    /// Total minutes classified as sleep.
    pub sleep_minutes: f64,
    // ----- AASM metrics (populated by stage_sleep_four_class; 0/empty in binary spine) -----
    /// Total sleep time: non-wake epochs × epoch minutes.
    pub tst_minutes: f64,
    /// Time in bed: entire window duration in minutes.
    pub time_in_bed_minutes: f64,
    /// Sleep efficiency: TST / TIB (0.0 when TIB is zero).
    pub sleep_efficiency_fraction: f64,
    /// Sleep-onset latency: minutes from window start to first non-wake epoch.
    pub sol_minutes: f64,
    /// Wake after sleep onset: wake epochs after first sleep onset × epoch minutes.
    pub waso_minutes: f64,
    /// Minutes per stage class.
    pub stage_minutes: BTreeMap<String, f64>,
}

/// Per-epoch HR feature for the 4-class classifier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EpochHrFeature {
    /// Unix timestamp (seconds) — used to align with gravity-table epochs.
    pub ts: f64,
    /// Heart rate in beats per minute.
    pub hr_bpm: f64,
}

// ---------------------------------------------------------------------------
// Public entry point — binary spine (Plan 26-01 compatibility)
// ---------------------------------------------------------------------------

/// Classify a sleep window into 1-minute wake/sleep epochs.
///
/// `rows` is a slice of (ts, x, y, z) tuples already fetched from the gravity
/// table ordered by ts ascending. Units: ts in seconds (Unix), x/y/z in g.
///
/// Returns a [`SleepStagingOutput`] with `staging_method = STAGING_METHOD_NO_IMU`
/// when `rows` is empty. AASM fields are zero/empty on the binary spine output.
pub fn stage_sleep(input: &SleepStagingInput, rows: &[(f64, f64, f64, f64)]) -> SleepStagingOutput {
    if rows.is_empty() {
        return empty_output(STAGING_METHOD_NO_IMU);
    }

    let activity_counts = compute_activity_counts(input.sleep_start_ts, rows);

    if activity_counts.is_empty() {
        return empty_output(STAGING_METHOD_ACTIGRAPHY);
    }

    let n = activity_counts.len();
    let mut epochs: Vec<SleepEpoch> = Vec::with_capacity(n);

    // Build lookup once for all epochs (PERF-02: avoid O(N²) HashMap rebuild per call).
    let ck_lookup: std::collections::HashMap<i64, f64> = activity_counts
        .iter()
        .map(|&(idx, cnt)| (idx, cnt))
        .collect();

    for i in 0..n {
        let d = cole_kripke_d_score(i, &activity_counts, &ck_lookup);
        let stage = if d >= COLE_KRIPKE_WAKE_THRESHOLD {
            "wake"
        } else {
            "sleep"
        };
        let (epoch_idx, count) = activity_counts[i];
        let ts = input.sleep_start_ts + epoch_idx as f64 * (COLE_KRIPKE_EPOCH_MINUTES * 60.0);
        epochs.push(SleepEpoch {
            ts,
            activity_count: count,
            stage: stage.to_string(),
        });
    }

    let total = epochs.len() as f64;
    let wake_count = epochs.iter().filter(|e| e.stage == "wake").count() as f64;
    let sleep_count = epochs.iter().filter(|e| e.stage == "sleep").count() as f64;

    SleepStagingOutput {
        epochs,
        staging_method: STAGING_METHOD_ACTIGRAPHY.to_string(),
        wake_fraction: if total > 0.0 { wake_count / total } else { 0.0 },
        sleep_minutes: sleep_count * COLE_KRIPKE_EPOCH_MINUTES,
        tst_minutes: 0.0,
        time_in_bed_minutes: 0.0,
        sleep_efficiency_fraction: 0.0,
        sol_minutes: 0.0,
        waso_minutes: 0.0,
        stage_minutes: BTreeMap::new(),
    }
}

// ---------------------------------------------------------------------------
// Validation note (MANUAL — not automated):
// The epoch-level classification of `stage_sleep_four_class` is uncalibrated
// actigraphy. It should be cross-validated against independent reference sleep
// stage labels on real overnight sessions before being treated as calibrated.
// The known literature ceiling for EEG-free actigraphy staging is ~65-73%
// epoch-level agreement; output is surfaced with an uncalibrated marker until
// such validation is recorded. This is a MANUAL gate — unit tests below only
// guard output well-formedness, not physiological accuracy.
// ---------------------------------------------------------------------------

/// Classify a sleep window into 1-minute 4-class (wake/light/deep/rem) epochs.
///
/// Builds on the Cole-Kripke binary spine from [`stage_sleep`] and refines
/// each "sleep" epoch using HR and motion features:
/// - "deep":  HR <= session p25 AND activity_count <= DEEP_STILLNESS_ACTIVITY_MAX
/// - "rem":   clock proxy (fractional position in night) >= REM_CLOCK_PROXY_MIN
///   AND hr_bpm > session median (proxy for higher cardiorespiratory activity)
///   AND `resp_available` is true (graceful degradation when resp absent)
/// - "light": remaining sleep epochs
/// - "wake":  unchanged from the binary spine
///
/// Physiological reimposition (applied in order after per-epoch classification):
///   (a) No REM in first 15 minutes of sleep: REM epochs < NO_REM_ONSET_MINUTES
///       from sleep onset are reclassified as "light".
///   (b) Minimum 5-minute segment merge: contiguous runs shorter than
///       ceil(MIN_SEGMENT_MINUTES / COLE_KRIPKE_EPOCH_MINUTES) epochs are
///       absorbed into the longer adjacent neighbour's class.
///
/// When `hr_features` is empty, all "sleep" epochs fall back to "light"
/// (no HR data available — still a valid 4-class output, never panics).
///
/// When `resp_available` is false, REM classification is suppressed and
/// would-be REM epochs are classified as "light". Pass `false` when the
/// resp_samples table has no rows for this session.
///
/// `staging_method` is `STAGING_METHOD_ACTIGRAPHY` ("actigraphy_uncalibrated")
/// for non-empty input; `STAGING_METHOD_NO_IMU` for empty input.
pub fn stage_sleep_four_class(
    input: &SleepStagingInput,
    rows: &[(f64, f64, f64, f64)],
    hr_features: &[EpochHrFeature],
    resp_available: bool,
) -> SleepStagingOutput {
    if rows.is_empty() {
        return empty_output_with_aasm(STAGING_METHOD_NO_IMU, input);
    }

    // Step 1: binary spine.
    let activity_counts = compute_activity_counts(input.sleep_start_ts, rows);
    if activity_counts.is_empty() {
        return empty_output_with_aasm(STAGING_METHOD_ACTIGRAPHY, input);
    }

    let n = activity_counts.len();
    let total_sleep_secs = input.sleep_end_ts - input.sleep_start_ts;

    // Step 2: compute HR statistics for classification (p25 and median).
    let (hr_p25, hr_median) = hr_percentiles(hr_features);

    // Build lookup once for all epochs (PERF-02: avoid O(N²) HashMap rebuild per call).
    let ck_lookup: std::collections::HashMap<i64, f64> = activity_counts
        .iter()
        .map(|&(idx, cnt)| (idx, cnt))
        .collect();

    // Step 3: per-epoch 4-class assignment.
    let mut epochs: Vec<SleepEpoch> = Vec::with_capacity(n);
    for i in 0..n {
        let d = cole_kripke_d_score(i, &activity_counts, &ck_lookup);
        let (epoch_idx, count) = activity_counts[i];
        let ts = input.sleep_start_ts + epoch_idx as f64 * (COLE_KRIPKE_EPOCH_MINUTES * 60.0);

        let stage = if d >= COLE_KRIPKE_WAKE_THRESHOLD {
            "wake".to_string()
        } else {
            // Refine sleep epoch using HR + motion features.
            let epoch_hr = nearest_hr(ts, hr_features);
            classify_sleep_epoch(
                i,
                n,
                count,
                epoch_hr,
                hr_p25,
                hr_median,
                total_sleep_secs,
                ts,
                input.sleep_start_ts,
                resp_available,
            )
        };
        epochs.push(SleepEpoch {
            ts,
            activity_count: count,
            stage,
        });
    }

    // Step 4: physiological reimposition.
    apply_reimposition(&mut epochs, input.sleep_start_ts);

    // Step 5: AASM metrics.
    let aasm = aasm_metrics(
        &epochs,
        COLE_KRIPKE_EPOCH_MINUTES,
        input.sleep_start_ts,
        input.sleep_end_ts,
    );
    let total = epochs.len() as f64;
    let wake_count = epochs.iter().filter(|e| e.stage == "wake").count() as f64;
    let non_wake_count = total - wake_count;

    SleepStagingOutput {
        staging_method: STAGING_METHOD_ACTIGRAPHY.to_string(),
        wake_fraction: if total > 0.0 { wake_count / total } else { 0.0 },
        sleep_minutes: non_wake_count * COLE_KRIPKE_EPOCH_MINUTES,
        tst_minutes: aasm.tst_minutes,
        time_in_bed_minutes: aasm.time_in_bed_minutes,
        sleep_efficiency_fraction: aasm.sleep_efficiency_fraction,
        sol_minutes: aasm.sol_minutes,
        waso_minutes: aasm.waso_minutes,
        stage_minutes: aasm.stage_minutes,
        epochs,
    }
}

// ---------------------------------------------------------------------------
// 4-class classification helpers
// ---------------------------------------------------------------------------

/// Classify a single non-wake epoch into light/deep/rem.
///
/// `resp_available` controls whether REM classification is attempted. When the
/// resp stream is absent from the database (no rows for this session), callers
/// pass `false` and all would-be REM epochs fall back to "light". This prevents
/// spurious REM assignments when the respiratory signal needed to confirm
/// cardiorespiratory arousal is unavailable.
fn classify_sleep_epoch(
    epoch_index: usize,
    total_epochs: usize,
    activity_count: f64,
    hr_bpm: Option<f64>,
    hr_p25: Option<f64>,
    hr_median: Option<f64>,
    _total_sleep_secs: f64,
    epoch_ts: f64,
    sleep_start_ts: f64,
    resp_available: bool,
) -> String {
    // Clock proxy: fractional position of this epoch in the total epoch sequence.
    let clock_proxy = if total_epochs > 1 {
        epoch_index as f64 / (total_epochs - 1) as f64
    } else {
        0.0
    };

    match hr_bpm {
        None => "light".to_string(), // no HR data — conservative fallback
        Some(hr) => {
            let p25 = hr_p25.unwrap_or(f64::MAX);
            let median = hr_median.unwrap_or(f64::MAX);

            // Deep: low HR (at or below p25) AND very still.
            if hr <= p25 && activity_count <= DEEP_STILLNESS_ACTIVITY_MAX {
                return "deep".to_string();
            }

            // REM: later in the night AND HR above session median.
            // Skip REM classification when resp stream is absent — without
            // respiratory confirmation the HR-only signal has too many false
            // positives and "light" is the conservative default.
            let minutes_from_onset = (epoch_ts - sleep_start_ts) / 60.0;
            if resp_available
                && clock_proxy >= REM_CLOCK_PROXY_MIN
                && hr > median
                && minutes_from_onset >= NO_REM_ONSET_MINUTES
            {
                return "rem".to_string();
            }

            "light".to_string()
        }
    }
}

/// Return the nearest HR sample to a given epoch timestamp, or None if no
/// HR features are provided. "Nearest" means the feature whose `ts` has the
/// smallest absolute distance from `epoch_ts`.
fn nearest_hr(epoch_ts: f64, hr_features: &[EpochHrFeature]) -> Option<f64> {
    if hr_features.is_empty() {
        return None;
    }
    hr_features
        .iter()
        .min_by(|a, b| {
            let da = (a.ts - epoch_ts).abs();
            let db = (b.ts - epoch_ts).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|f| f.hr_bpm)
}

/// Compute the session p25 and median HR from the feature list.
/// Returns (None, None) when the list is empty.
fn hr_percentiles(hr_features: &[EpochHrFeature]) -> (Option<f64>, Option<f64>) {
    if hr_features.is_empty() {
        return (None, None);
    }
    let mut vals: Vec<f64> = hr_features.iter().map(|f| f.hr_bpm).collect();
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = vals.len();
    let p25_idx = ((n as f64 - 1.0) * DEEP_HR_PERCENTILE).floor() as usize;
    let med_idx = (n - 1) / 2;
    (Some(vals[p25_idx]), Some(vals[med_idx]))
}

// ---------------------------------------------------------------------------
// Physiological reimposition
// ---------------------------------------------------------------------------

/// Apply physiological reimposition rules in order:
///   (a) No REM within NO_REM_ONSET_MINUTES of sleep onset.
///   (b) Merge contiguous runs shorter than ceil(MIN_SEGMENT_MINUTES / EPOCH_MINUTES).
fn apply_reimposition(epochs: &mut [SleepEpoch], sleep_start_ts: f64) {
    if epochs.is_empty() {
        return;
    }

    // Rule (a): no early REM.
    for epoch in epochs.iter_mut() {
        if epoch.stage == "rem" {
            let minutes_from_onset = (epoch.ts - sleep_start_ts) / 60.0;
            if minutes_from_onset < NO_REM_ONSET_MINUTES {
                epoch.stage = "light".to_string();
            }
        }
    }

    // Rule (b): minimum segment merge.
    let min_seg_epochs = (MIN_SEGMENT_MINUTES / COLE_KRIPKE_EPOCH_MINUTES).ceil() as usize;
    merge_short_segments(epochs, min_seg_epochs);
}

/// Merge contiguous runs shorter than `min_len` epochs into the longer
/// adjacent neighbour's class.
///
/// Algorithm: identify runs, find short ones, absorb them into the longer
/// of their left/right neighbours. Repeats until stable (short runs can
/// cascade after a merge).
fn merge_short_segments(epochs: &mut [SleepEpoch], min_len: usize) {
    let n = epochs.len();
    if n == 0 || min_len <= 1 {
        return;
    }

    let max_iterations = n + 1;
    for _ in 0..max_iterations {
        // Find runs.
        let runs = collect_runs(epochs);
        let mut changed = false;

        for run in &runs {
            if run.len < min_len {
                // Find longer of left/right neighbour.
                let left_len = runs
                    .iter()
                    .filter(|r| r.end == run.start)
                    .map(|r| r.len)
                    .next()
                    .unwrap_or(0);
                let right_len = runs
                    .iter()
                    .filter(|r| r.start == run.end)
                    .map(|r| r.len)
                    .next()
                    .unwrap_or(0);

                let donor_class = if left_len == 0 && right_len == 0 {
                    continue; // isolated single-run sequence; leave as-is
                } else if left_len >= right_len {
                    runs.iter()
                        .find(|r| r.end == run.start)
                        .map(|r| r.class.clone())
                } else {
                    runs.iter()
                        .find(|r| r.start == run.end)
                        .map(|r| r.class.clone())
                };

                if let Some(cls) = donor_class {
                    for epoch in &mut epochs[run.start..run.end] {
                        epoch.stage = cls.clone();
                    }
                    changed = true;
                    break; // restart after each merge (runs are stale)
                }
            }
        }

        if !changed {
            break;
        }
    }
}

/// A contiguous run of epochs with the same stage.
struct Run {
    start: usize,
    end: usize, // exclusive
    len: usize,
    class: String,
}

fn collect_runs(epochs: &[SleepEpoch]) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    if epochs.is_empty() {
        return runs;
    }
    let mut start = 0;
    for i in 1..epochs.len() {
        if epochs[i].stage != epochs[start].stage {
            runs.push(Run {
                start,
                end: i,
                len: i - start,
                class: epochs[start].stage.clone(),
            });
            start = i;
        }
    }
    runs.push(Run {
        start,
        end: epochs.len(),
        len: epochs.len() - start,
        class: epochs[start].stage.clone(),
    });
    runs
}

// ---------------------------------------------------------------------------
// AASM metrics
// ---------------------------------------------------------------------------

struct AasmMetrics {
    tst_minutes: f64,
    time_in_bed_minutes: f64,
    sleep_efficiency_fraction: f64,
    sol_minutes: f64,
    waso_minutes: f64,
    stage_minutes: BTreeMap<String, f64>,
}

/// Derive AASM summary metrics from a final reimposed hypnogram.
/// CR-02 fix: SOL derived from epoch timestamps, not array index × epoch_minutes.
/// CR-03 fix: TIB derived from declared window bounds, not count of data epochs.
fn aasm_metrics(
    epochs: &[SleepEpoch],
    epoch_minutes: f64,
    sleep_start_ts: f64,
    sleep_end_ts: f64,
) -> AasmMetrics {
    // CR-03: TIB = declared window duration (not count of sparse data epochs).
    let tib = ((sleep_end_ts - sleep_start_ts).max(0.0) / 60.0).max(epoch_minutes);

    // TST: sum of non-wake epochs.
    let tst = epochs.iter().filter(|e| e.stage != "wake").count() as f64 * epoch_minutes;

    let efficiency = if tib > 0.0 { tst / tib } else { 0.0 };

    // CR-02: SOL from epoch timestamp, not array index.
    let first_sleep_idx = epochs.iter().position(|e| e.stage != "wake");
    let sol = match first_sleep_idx {
        None => tib,
        Some(idx) => (epochs[idx].ts - sleep_start_ts).max(0.0) / 60.0,
    };

    // WASO: wake epochs that occur after sleep onset.
    let waso = match first_sleep_idx {
        None => 0.0,
        Some(onset) => {
            epochs[onset..].iter().filter(|e| e.stage == "wake").count() as f64 * epoch_minutes
        }
    };

    // Stage minutes.
    let mut stage_minutes: BTreeMap<String, f64> = BTreeMap::new();
    for epoch in epochs {
        *stage_minutes.entry(epoch.stage.clone()).or_insert(0.0) += epoch_minutes;
    }

    AasmMetrics {
        tst_minutes: tst,
        time_in_bed_minutes: tib,
        sleep_efficiency_fraction: efficiency,
        sol_minutes: sol,
        waso_minutes: waso,
        stage_minutes,
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn empty_output(staging_method: &str) -> SleepStagingOutput {
    SleepStagingOutput {
        epochs: vec![],
        staging_method: staging_method.to_string(),
        wake_fraction: 0.0,
        sleep_minutes: 0.0,
        tst_minutes: 0.0,
        time_in_bed_minutes: 0.0,
        sleep_efficiency_fraction: 0.0,
        sol_minutes: 0.0,
        waso_minutes: 0.0,
        stage_minutes: BTreeMap::new(),
    }
}

fn empty_output_with_aasm(staging_method: &str, input: &SleepStagingInput) -> SleepStagingOutput {
    // time_in_bed_minutes is still meaningful even with no IMU data.
    let tib = (input.sleep_end_ts - input.sleep_start_ts).max(0.0) / 60.0;
    SleepStagingOutput {
        epochs: vec![],
        staging_method: staging_method.to_string(),
        wake_fraction: 0.0,
        sleep_minutes: 0.0,
        tst_minutes: 0.0,
        time_in_bed_minutes: tib,
        sleep_efficiency_fraction: 0.0,
        sol_minutes: tib, // no onset → full window is latency
        waso_minutes: 0.0,
        stage_minutes: BTreeMap::new(),
    }
}

// ---------------------------------------------------------------------------
// Internal helpers (shared by binary spine and 4-class)
// ---------------------------------------------------------------------------

/// Bucket gravity rows into 1-minute epochs and compute per-epoch activity
/// counts as the sum of inter-sample magnitude differences.
///
/// Returns a sorted `Vec<(epoch_index, activity_count)>` where `epoch_index`
/// is floor((ts - sleep_start_ts) / 60).
fn compute_activity_counts(sleep_start_ts: f64, rows: &[(f64, f64, f64, f64)]) -> Vec<(i64, f64)> {
    // (epoch_index) -> (prev_magnitude: Option<f64>, cumulative_count: f64)
    let mut epoch_state: BTreeMap<i64, (Option<f64>, f64)> = BTreeMap::new();

    for &(ts, x, y, z) in rows {
        let offset = ts - sleep_start_ts;
        let epoch_idx = (offset / (COLE_KRIPKE_EPOCH_MINUTES * 60.0)).floor() as i64;

        let mag = (x * x + y * y + z * z).sqrt();
        let entry = epoch_state.entry(epoch_idx).or_insert((None, 0.0));

        if let Some(prev_mag) = entry.0 {
            entry.1 += (mag - prev_mag).abs();
        }
        entry.0 = Some(mag);
    }

    epoch_state
        .into_iter()
        .map(|(idx, (_prev, count))| (idx, count))
        .collect()
}

/// Compute the Cole-Kripke D score for epoch `i`.
///
/// D = (1/100) * Σ_k ( COEFFS[k] * scaled_count(i + OFFSETS[k]) )
///
/// Out-of-range neighbours contribute 0.
/// Compute the Cole-Kripke D-score for epoch `i`.
/// `lookup` must be a pre-built map of epoch_idx → activity_count, built once
/// by the caller before the epoch loop (PERF-02: avoids O(N²) per-call rebuild).
fn cole_kripke_d_score(
    i: usize,
    activity_counts: &[(i64, f64)],
    lookup: &std::collections::HashMap<i64, f64>,
) -> f64 {
    // CR-01 fix: look up neighbours by epoch_idx (temporal index) via the pre-built
    // HashMap, not by array position. Gaps in the gravity table produce holes in the
    // array; array[i+1] does NOT mean the temporally-adjacent minute when data is sparse.
    let current_epoch_idx = activity_counts[i].0;
    let mut d = 0.0_f64;
    for (coeff, &offset) in COLE_KRIPKE_COEFFS.iter().zip(COLE_KRIPKE_OFFSETS.iter()) {
        let neighbour_idx = current_epoch_idx + offset;
        let c = COLE_KRIPKE_SCALE_FACTOR * lookup.get(&neighbour_idx).copied().unwrap_or(0.0);
        d += coeff * c;
    }
    d / 100.0
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_input(sleep_start_ts: f64, sleep_end_ts: f64) -> SleepStagingInput {
        SleepStagingInput {
            device_id: "dev-test".to_string(),
            sleep_start_ts,
            sleep_end_ts,
        }
    }

    // ---------------------------------------------------------------------------
    // Plan 26-01 binary spine tests (retained)
    // ---------------------------------------------------------------------------

    // T1: empty rows → no_imu_data, empty epochs, zeros
    #[test]
    fn empty_rows_yields_no_imu_data() {
        let input = make_input(0.0, 3600.0);
        let output = stage_sleep(&input, &[]);
        assert_eq!(output.staging_method, STAGING_METHOD_NO_IMU);
        assert!(output.epochs.is_empty(), "epochs must be empty");
        assert_eq!(output.sleep_minutes, 0.0);
        assert_eq!(output.wake_fraction, 0.0);
    }

    // T2: still epoch (constant g vector) yields activity_count ≈ 0.0
    #[test]
    fn still_epoch_activity_count_is_zero() {
        let start = 1_000_000.0_f64;
        let rows: Vec<(f64, f64, f64, f64)> =
            (0..10).map(|i| (start + i as f64, 0.0, 0.0, 1.0)).collect();
        let input = make_input(start, start + 600.0);
        let output = stage_sleep(&input, &rows);

        assert!(!output.epochs.is_empty(), "should have at least one epoch");
        for epoch in &output.epochs {
            assert!(
                epoch.activity_count.abs() < 1e-9,
                "still epoch must have near-zero count, got {}",
                epoch.activity_count
            );
        }
    }

    // T3: Cole-Kripke D score — high-motion epoch → wake; still epoch → sleep.
    //
    // With COLE_KRIPKE_SCALE_FACTOR = 0.001 and the 7 coefficients summing to 665,
    // D = (665 * 0.001 / 100) * C = 0.00665 * C.
    // To exceed WAKE_THRESHOLD=1.0 we need C > 150.4 per epoch.
    // We generate C ≈ 200 by alternating between magnitude 0 and 200 each sample.
    #[test]
    fn cole_kripke_classifies_wake_and_sleep() {
        let start = 0.0_f64;
        let epoch_secs = COLE_KRIPKE_EPOCH_MINUTES * 60.0;

        let mut rows: Vec<(f64, f64, f64, f64)> = Vec::new();
        for epoch in 0..7i64 {
            let t0 = start + epoch as f64 * epoch_secs;
            // Alternate between |g|=0 and |g|=200 to produce activity_count ≈ 200.
            rows.push((t0, 0.0, 0.0, 0.0));
            rows.push((t0 + 1.0, 200.0, 0.0, 0.0));
        }
        let end = start + 7.0 * epoch_secs;
        let input = make_input(start, end);
        let output = stage_sleep(&input, &rows);

        let centre = &output.epochs[3];
        assert_eq!(centre.stage, "wake", "high-motion epoch must be wake");

        let rows_still: Vec<(f64, f64, f64, f64)> =
            vec![(start, 0.0, 0.0, 1.0), (start + 1.0, 0.0, 0.0, 1.0)];
        let input_still = make_input(start, start + epoch_secs);
        let output_still = stage_sleep(&input_still, &rows_still);
        assert_eq!(
            output_still.epochs[0].stage, "sleep",
            "still epoch must be sleep"
        );
    }

    // T4: edge handling — epochs near start/end do not panic
    #[test]
    fn edge_epochs_do_not_panic() {
        let start = 0.0_f64;
        let epoch_secs = COLE_KRIPKE_EPOCH_MINUTES * 60.0;
        let mut rows: Vec<(f64, f64, f64, f64)> = Vec::new();
        for epoch in 0..2i64 {
            let t0 = start + epoch as f64 * epoch_secs;
            rows.push((t0, 0.0, 0.0, 0.0));
            rows.push((t0 + 1.0, 1.0, 0.0, 0.0));
        }
        let input = make_input(start, start + 2.0 * epoch_secs);
        let output = stage_sleep(&input, &rows);
        assert_eq!(output.epochs.len(), 2);
    }

    // T5: non-empty rows → staging_method ALWAYS "actigraphy_uncalibrated"
    #[test]
    fn non_empty_rows_always_actigraphy_uncalibrated() {
        let start = 0.0_f64;
        let rows: Vec<(f64, f64, f64, f64)> =
            vec![(start, 0.0, 0.0, 1.0), (start + 1.0, 0.0, 0.0, 1.0)];
        let input = make_input(start, start + 3600.0);
        let output = stage_sleep(&input, &rows);
        assert_eq!(output.staging_method, STAGING_METHOD_ACTIGRAPHY);
        assert_ne!(output.staging_method, STAGING_METHOD_NO_IMU);
    }

    // T6: wake_fraction and sleep_minutes are computed correctly
    #[test]
    fn wake_fraction_and_sleep_minutes_are_correct() {
        let start = 0.0_f64;
        let epoch_secs = COLE_KRIPKE_EPOCH_MINUTES * 60.0;
        let mut rows: Vec<(f64, f64, f64, f64)> = Vec::new();
        rows.push((start, 0.0, 0.0, 1.0));
        rows.push((start + 1.0, 0.0, 0.0, 1.0));
        let t1 = start + epoch_secs;
        rows.push((t1, 0.0, 0.0, 0.0));
        rows.push((t1 + 1.0, 1.0, 0.0, 0.0));

        let input = make_input(start, start + 2.0 * epoch_secs);
        let output = stage_sleep(&input, &rows);

        assert_eq!(output.epochs.len(), 2);
        let wake_count = output.epochs.iter().filter(|e| e.stage == "wake").count();
        let sleep_count = output.epochs.iter().filter(|e| e.stage == "sleep").count();
        assert_eq!(output.wake_fraction, wake_count as f64 / 2.0);
        assert_eq!(
            output.sleep_minutes,
            sleep_count as f64 * COLE_KRIPKE_EPOCH_MINUTES
        );
    }

    // ---------------------------------------------------------------------------
    // Plan 26-02: 4-class classifier tests
    // ---------------------------------------------------------------------------

    /// Build a still, low-HR window: should yield "deep" epochs.
    #[test]
    fn four_class_still_low_hr_yields_deep() {
        let start = 0.0_f64;
        let epoch_secs = COLE_KRIPKE_EPOCH_MINUTES * 60.0;
        // 30 epochs, all still (activity_count = 0).
        let rows: Vec<(f64, f64, f64, f64)> = (0..30)
            .flat_map(|i| {
                let t = start + i as f64 * epoch_secs;
                vec![(t, 0.0, 0.0, 1.0), (t + 1.0, 0.0, 0.0, 1.0)]
            })
            .collect();
        // HR far below session range (all epochs = 45 bpm; session p25 will be 45, median 45).
        let hr_features: Vec<EpochHrFeature> = (0..30)
            .map(|i| EpochHrFeature {
                ts: start + i as f64 * epoch_secs + 30.0,
                hr_bpm: 45.0,
            })
            .collect();

        let input = make_input(start, start + 30.0 * epoch_secs);
        let output = stage_sleep_four_class(&input, &rows, &hr_features, true);

        assert_eq!(output.staging_method, STAGING_METHOD_ACTIGRAPHY);
        // All non-wake epochs should be deep (still + HR <= p25).
        let non_wake: Vec<&SleepEpoch> =
            output.epochs.iter().filter(|e| e.stage != "wake").collect();
        assert!(!non_wake.is_empty(), "should have non-wake epochs");
        for e in &non_wake {
            assert_eq!(
                e.stage, "deep",
                "still + low-HR epoch must be deep, got {}",
                e.stage
            );
        }
    }

    /// Late-night higher-HR window: should yield "rem" epochs after the first 15 min.
    #[test]
    fn four_class_late_high_hr_yields_rem() {
        let start = 0.0_f64;
        let epoch_secs = COLE_KRIPKE_EPOCH_MINUTES * 60.0;
        // 40 still epochs covering 40 minutes.
        let rows: Vec<(f64, f64, f64, f64)> = (0..40)
            .flat_map(|i| {
                let t = start + i as f64 * epoch_secs;
                vec![(t, 0.0, 0.0, 1.0), (t + 1.0, 0.0, 0.0, 1.0)]
            })
            .collect();
        // First 20 epochs HR = 55 (low), last 20 epochs HR = 75 (high).
        // Session median will be 65, p25 will be ~55.
        let hr_features: Vec<EpochHrFeature> = (0..40)
            .map(|i| EpochHrFeature {
                ts: start + i as f64 * epoch_secs + 30.0,
                hr_bpm: if i < 20 { 55.0 } else { 75.0 },
            })
            .collect();

        let input = make_input(start, start + 40.0 * epoch_secs);
        let output = stage_sleep_four_class(&input, &rows, &hr_features, true);

        // Epochs in the second half (index >= 16, i.e. clock_proxy >= 0.4) with HR > median
        // should become REM. Reimposition rule (a) already handled by clock proxy + onset guard.
        let late_epochs: Vec<&SleepEpoch> = output
            .epochs
            .iter()
            .enumerate()
            .filter(|(i, _)| *i >= 20 && output.epochs[*i].stage != "wake")
            .map(|(_, e)| e)
            .collect();

        let rem_count = late_epochs.iter().filter(|e| e.stage == "rem").count();
        assert!(
            rem_count > 0,
            "expected REM epochs in the second half, got: {:?}",
            late_epochs.iter().map(|e| &e.stage).collect::<Vec<_>>()
        );
    }

    /// Reimposition rule (a): REM epoch placed at minute 5 must be reclassified to light.
    #[test]
    fn reimposition_rule_a_removes_early_rem() {
        // Create a hand-crafted epoch sequence: place a REM at index 5 (minute 5).
        let start = 0.0_f64;
        let epoch_secs = COLE_KRIPKE_EPOCH_MINUTES * 60.0;
        let n = 20usize;

        // Still rows (all sleep in binary spine).
        let _rows: Vec<(f64, f64, f64, f64)> = (0..n)
            .flat_map(|i| {
                let t = start + i as f64 * epoch_secs;
                vec![(t, 0.0, 0.0, 1.0), (t + 1.0, 0.0, 0.0, 1.0)]
            })
            .collect();

        // Craft HR so epoch 5 looks like REM (high HR, clock_proxy >= 0.4).
        // With n=20, index 5 → clock_proxy = 5/19 ≈ 0.26 < 0.4 → will not be REM by classifier.
        // Instead directly test the reimposition function.
        let mut epochs: Vec<SleepEpoch> = (0..n)
            .map(|i| SleepEpoch {
                ts: start + i as f64 * epoch_secs,
                activity_count: 0.0,
                stage: if i == 5 {
                    "rem".to_string()
                } else {
                    "light".to_string()
                },
            })
            .collect();

        apply_reimposition(&mut epochs, start);

        // Epoch 5 is at minute 5, which is < NO_REM_ONSET_MINUTES (15) → must become "light".
        assert_eq!(
            epochs[5].stage, "light",
            "REM at minute 5 must be reclassified to light by rule (a)"
        );
    }

    /// Reimposition rule (b): a 2-epoch island must be absorbed into the longer neighbour.
    #[test]
    fn reimposition_rule_b_merges_short_segment() {
        let start = 0.0_f64;
        let epoch_secs = COLE_KRIPKE_EPOCH_MINUTES * 60.0;
        // Sequence: 10 light, 2 rem, 10 light — the 2-epoch rem island is < min_seg (5).
        let n = 22usize;
        let mut epochs: Vec<SleepEpoch> = (0..n)
            .map(|i| SleepEpoch {
                ts: start + i as f64 * epoch_secs,
                activity_count: 0.0,
                stage: if i >= 10 && i < 12 {
                    "rem".to_string()
                } else {
                    "light".to_string()
                },
            })
            .collect();

        let min_seg = (MIN_SEGMENT_MINUTES / COLE_KRIPKE_EPOCH_MINUTES).ceil() as usize;
        merge_short_segments(&mut epochs, min_seg);

        // The 2-epoch rem island must now be "light".
        for i in 10..12 {
            assert_eq!(
                epochs[i].stage, "light",
                "short rem island at epoch {} should be merged into light",
                i
            );
        }
    }

    /// AASM derivation on a known synthetic hypnogram.
    ///
    /// Hypnogram (epoch_minutes = 1.0):
    ///   [0-4]   wake  (5 epochs → SOL = 5 min)
    ///   [5-14]  light (10 epochs)
    ///   [15-16] wake  (2 epochs → WASO = 2 min)
    ///   [17-22] deep  (6 epochs)
    ///   [23-29] rem   (7 epochs)
    ///
    /// Expected:
    ///   TIB  = 30 min
    ///   TST  = 23 min (10 light + 6 deep + 7 rem)
    ///   SOL  = 5 min
    ///   WASO = 2 min
    ///   Efficiency = 23/30
    #[test]
    fn aasm_metrics_known_hypnogram() {
        let start = 0.0_f64;
        let epoch_secs = 60.0;
        let stages: Vec<&str> = (0..30)
            .map(|i| match i {
                0..=4 => "wake",
                5..=14 => "light",
                15..=16 => "wake",
                17..=22 => "deep",
                _ => "rem",
            })
            .collect();

        let epochs: Vec<SleepEpoch> = stages
            .iter()
            .enumerate()
            .map(|(i, &s)| SleepEpoch {
                ts: start + i as f64 * epoch_secs,
                activity_count: 0.0,
                stage: s.to_string(),
            })
            .collect();

        // sleep window: 30 minutes from ts=0 to ts=1800
        let sleep_end = start + 30.0 * epoch_secs;
        let aasm = aasm_metrics(&epochs, 1.0, start, sleep_end);

        assert_eq!(aasm.time_in_bed_minutes, 30.0, "TIB must be 30");
        assert_eq!(aasm.tst_minutes, 23.0, "TST must be 23");
        assert_eq!(aasm.sol_minutes, 5.0, "SOL must be 5");
        assert_eq!(aasm.waso_minutes, 2.0, "WASO must be 2");
        assert!(
            (aasm.sleep_efficiency_fraction - 23.0 / 30.0).abs() < 1e-9,
            "efficiency must be 23/30, got {}",
            aasm.sleep_efficiency_fraction
        );
        assert_eq!(*aasm.stage_minutes.get("wake").unwrap_or(&0.0), 7.0);
        assert_eq!(*aasm.stage_minutes.get("light").unwrap_or(&0.0), 10.0);
        assert_eq!(*aasm.stage_minutes.get("deep").unwrap_or(&0.0), 6.0);
        assert_eq!(*aasm.stage_minutes.get("rem").unwrap_or(&0.0), 7.0);
    }

    /// 4-class output always has staging_method == "actigraphy_uncalibrated" for non-empty rows.
    #[test]
    fn four_class_non_empty_always_actigraphy_uncalibrated() {
        let start = 0.0_f64;
        let epoch_secs = COLE_KRIPKE_EPOCH_MINUTES * 60.0;
        let rows = vec![(start, 0.0, 0.0, 1.0), (start + 1.0, 0.0, 0.0, 1.0)];
        let input = make_input(start, start + epoch_secs);
        let output = stage_sleep_four_class(&input, &rows, &[], true);
        assert_eq!(
            output.staging_method, STAGING_METHOD_ACTIGRAPHY,
            "4-class non-empty must emit actigraphy_uncalibrated"
        );
    }

    /// 4-class with empty rows → no_imu_data staging_method.
    #[test]
    fn four_class_empty_rows_yields_no_imu_data() {
        let input = make_input(0.0, 3600.0);
        let output = stage_sleep_four_class(&input, &[], &[], false);
        assert_eq!(output.staging_method, STAGING_METHOD_NO_IMU);
        assert!(output.epochs.is_empty());
    }

    /// 4-class with no HR features: sleep epochs fall back to "light".
    #[test]
    fn four_class_no_hr_features_falls_back_to_light() {
        let start = 0.0_f64;
        let epoch_secs = COLE_KRIPKE_EPOCH_MINUTES * 60.0;
        // Still rows (no wake from Cole-Kripke).
        let rows = vec![(start, 0.0, 0.0, 1.0), (start + 1.0, 0.0, 0.0, 1.0)];
        let input = make_input(start, start + epoch_secs);
        let output = stage_sleep_four_class(&input, &rows, &[], false);
        for e in &output.epochs {
            assert_ne!(
                e.stage, "sleep",
                "binary 'sleep' must not appear in 4-class output"
            );
            assert_ne!(e.stage, "deep", "deep requires HR data");
            assert_ne!(e.stage, "rem", "rem requires HR data");
        }
    }

    /// PROTO-03: when resp_available=false, REM classification is suppressed.
    /// Same scenario as four_class_late_high_hr_yields_rem but with resp absent.
    #[test]
    fn four_class_no_resp_suppresses_rem() {
        let start = 0.0_f64;
        let epoch_secs = COLE_KRIPKE_EPOCH_MINUTES * 60.0;
        // 40 still epochs.
        let rows: Vec<(f64, f64, f64, f64)> = (0..40)
            .flat_map(|i| {
                let t = start + i as f64 * epoch_secs;
                vec![(t, 0.0, 0.0, 1.0), (t + 1.0, 0.0, 0.0, 1.0)]
            })
            .collect();
        // HR pattern that would normally produce REM (second half high HR).
        let hr_features: Vec<EpochHrFeature> = (0..40)
            .map(|i| EpochHrFeature {
                ts: start + i as f64 * epoch_secs + 30.0,
                hr_bpm: if i < 20 { 55.0 } else { 75.0 },
            })
            .collect();

        let input = make_input(start, start + 40.0 * epoch_secs);
        // resp_available=false → no REM should appear.
        let output = stage_sleep_four_class(&input, &rows, &hr_features, false);

        let rem_count = output.epochs.iter().filter(|e| e.stage == "rem").count();
        assert_eq!(
            rem_count, 0,
            "resp_available=false must suppress all REM classification"
        );
    }
}

// ---------------------------------------------------------------------------
// Sleep staging parity validation (VAL-02 / ALG-SLP-04 synthetic gate)
// ---------------------------------------------------------------------------
// These tests verify that stage_sleep_four_class produces well-formed,
// physiologically plausible output on synthetic fixtures. They are a
// code-level regression guard, not a physiological-accuracy gate.
//
// Calibration status: OPEN — uncalibrated actigraphy staging requires
// cross-validation against independent reference stage labels on real
// overnight sessions before it can be treated as calibrated.

#[cfg(test)]
mod sleep_staging_parity_tests {
    use super::*;

    const BASE_TS: f64 = 1_700_000_000.0_f64;

    fn make_gravity(n_hours: f64, pattern: &str) -> Vec<(f64, f64, f64, f64)> {
        // Generate gravity rows at 25 Hz covering the sleep window.
        // ts starts at BASE_TS and goes for n_hours * 3600 seconds.
        let total_seconds = n_hours * 3600.0;
        let sample_rate = 25_usize; // Hz
        let total = (total_seconds as usize) * sample_rate;
        (0..total)
            .map(|i| {
                let t = BASE_TS + i as f64 / sample_rate as f64;
                match pattern {
                    "still" => (t, 0.0, 0.0, 1.0),
                    "active" => {
                        let angle = (i as f64 * 0.4).sin();
                        (t, angle, 0.0, (1.0 - angle * angle).sqrt().max(0.0))
                    }
                    _ => (t, 0.0, 0.0, 1.0),
                }
            })
            .collect()
    }

    fn simple_input(n_hours: f64) -> SleepStagingInput {
        SleepStagingInput {
            device_id: "test-device".to_string(),
            sleep_start_ts: BASE_TS,
            sleep_end_ts: BASE_TS + n_hours * 3600.0,
        }
    }

    // VAL-02 Fixture 1: still night → predominantly sleep epochs.
    #[test]
    fn test_staging_parity_still_night_mostly_sleep() {
        let n_hours = 7.0;
        let input = simple_input(n_hours);
        let tuples = make_gravity(n_hours, "still");
        let hr_feats: Vec<EpochHrFeature> = vec![];
        let output = stage_sleep_four_class(&input, &tuples, &hr_feats, false);

        let total_epochs = output.epochs.len();
        assert!(total_epochs > 0, "must produce epochs for 7-hour window");

        let wake_count = output.epochs.iter().filter(|e| e.stage == "wake").count();
        let sleep_count = total_epochs - wake_count;
        let sleep_fraction = sleep_count as f64 / total_epochs as f64;

        // A still night should yield >= 80% sleep epochs.
        assert!(
            sleep_fraction >= 0.80,
            "still night: sleep fraction {:.3} must be >= 0.80",
            sleep_fraction
        );

        // AASM metrics must be non-negative.
        assert!(output.tst_minutes >= 0.0, "TST must be >= 0");
        assert!(output.sol_minutes >= 0.0, "SOL must be >= 0");
        assert!(output.waso_minutes >= 0.0, "WASO must be >= 0");
        assert!(
            output.sleep_efficiency_fraction >= 0.0 && output.sleep_efficiency_fraction <= 1.0,
            "efficiency must be in [0,1]: {}",
            output.sleep_efficiency_fraction
        );
    }

    // VAL-02 Fixture 2: stage_minutes sums to ≈ TST (non-wake epochs).
    #[test]
    fn test_staging_parity_stage_minutes_sum_equals_tst() {
        let n_hours = 6.0;
        let input = simple_input(n_hours);
        let gravity = make_gravity(n_hours, "still");
        let hr_feats: Vec<EpochHrFeature> = vec![];
        let output = stage_sleep_four_class(&input, &gravity, &hr_feats, true);

        // stage_minutes should sum to TST (within float rounding).
        let stage_sum: f64 = output.stage_minutes.values().sum();
        let tst = output.tst_minutes;
        assert!(
            (stage_sum - tst).abs() < 1.0,
            "stage_minutes sum {:.3} must equal tst_minutes {:.3} within 1 min",
            stage_sum,
            tst
        );
    }

    // VAL-02 Fixture 3: epoch 30s resolution check — each epoch is COLE_KRIPKE_EPOCH_MINUTES.
    #[test]
    fn test_staging_parity_epoch_duration_is_30s() {
        let n_hours = 4.0;
        let input = simple_input(n_hours);
        let gravity = make_gravity(n_hours, "still");
        let hr_feats: Vec<EpochHrFeature> = vec![];
        let output = stage_sleep_four_class(&input, &gravity, &hr_feats, false);

        // Expected total epochs for 4h window = 4*60/0.5 = 480
        let expected_epochs = (n_hours * 60.0 / COLE_KRIPKE_EPOCH_MINUTES).round() as usize;
        // Allow ±1 for boundary handling.
        let actual = output.epochs.len();
        assert!(
            (actual as i64 - expected_epochs as i64).abs() <= 1,
            "4h window must yield ~{} 30s epochs, got {}",
            expected_epochs,
            actual
        );
    }
}
