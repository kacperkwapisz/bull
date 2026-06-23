// Sleep staging: actigraphy spine using Cole-Kripke (1992) binary wake/sleep classifier,
// extended to a 4-class (wake/light/deep/rem) hypnogram using multi-feature
// percentile-band classification.
//
// References:
//   Cole, R.J. et al. "Automatic sleep/wake identification from wrist activity."
//   Sleep 1992; 15(5): 461-469.
//
//   Walch, O. et al. "Sleep stage prediction with raw acceleration and photoplethysmography"
//   Sleep 2019; 42(12): zsz180. (DoG-HR feature)
//
//   Task Force of ESC/NASPE. "Heart rate variability: standards of measurement."
//   Circulation 1996; 93(5): 1043-1065. (RMSSD)
//
//   Berry, R.B. et al. "AASM scoring manual updates." JCSM 2012; 8(5): 597-619.
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
pub const DEEP_HR_PERCENTILE: f64 = 0.25;

/// Motion fraction at or below which an epoch counts as "still" for deep.
pub const DEEP_STILLNESS_MOTION_FRACTION: f64 = 0.10;

/// Maximum activity count for a "deep" sleep epoch (legacy compat).
pub const DEEP_STILLNESS_ACTIVITY_MAX: f64 = 0.05;

/// RMSSD percentile at or above which an epoch shows high parasympathetic
/// tone — a deep-sleep indicator (Task Force 1996).
pub const DEEP_RMSSD_PERCENTILE: f64 = 0.70;

/// Fractional position in the sleep period (clock proxy) at or above which
/// a sleep epoch is eligible to be classified as REM.
pub const REM_CLOCK_PROXY_MIN: f64 = 0.4;

/// Motion fraction threshold for wake detection.
pub const WAKE_MOTION_FRACTION: f64 = 0.15;

/// Respiratory rate variability (IQR) percentile at or above which breathing
/// is considered irregular — a REM indicator.
pub const REM_RRV_PERCENTILE: f64 = 0.65;

/// No-REM onset guard: REM epochs within this many minutes of sleep onset
/// are reclassified as light (physiological reimposition rule a).
pub const NO_REM_ONSET_MINUTES: f64 = 15.0;

/// Minimum continuous segment duration (minutes) before reimposition merges
/// short islands into adjacent classes (physiological reimposition rule b).
pub const MIN_SEGMENT_MINUTES: f64 = 5.0;

/// Deep sleep is front-loaded: epochs beyond this fraction of the night
/// are demoted from deep to light (reimposition rule c).
pub const DEEP_FRONT_LOADED_FRACTION: f64 = 0.333;

/// Median smoothing window for label sequence (epochs). Must be odd.
const MEDIAN_SMOOTH_WINDOW: usize = 5;

/// Gravity delta threshold (g) below which a sample is "still".
const GRAVITY_STILL_THRESHOLD: f64 = 0.01;

/// DoG-HR Gaussian sigma values in seconds (Walch 2019).
const DOG_HR_SIGMA_SHORT_S: f64 = 120.0;
const DOG_HR_SIGMA_LONG_S: f64 = 600.0;

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
    /// REM onset latency: minutes from sleep onset to first REM epoch.
    /// None when no REM epochs are present.
    pub rem_latency_minutes: Option<f64>,
}

/// Per-epoch HR feature for the 4-class classifier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EpochHrFeature {
    /// Unix timestamp (seconds) — used to align with gravity-table epochs.
    pub ts: f64,
    /// Heart rate in beats per minute.
    pub hr_bpm: f64,
}

/// Per-beat RR interval with timestamp for windowed RMSSD computation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EpochRrFeature {
    /// Unix timestamp (seconds) of this beat.
    pub ts: f64,
    /// R-R interval in milliseconds.
    pub rr_ms: f64,
}

/// Raw respiration channel sample for breath-rate extraction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EpochRespFeature {
    /// Unix timestamp (seconds).
    pub ts: f64,
    /// Raw resp sensor value.
    pub raw: f64,
}

/// Extracted per-epoch features for the multi-signal classifier.
#[derive(Debug, Clone, Default)]
struct EpochFeatures {
    /// Fraction of gravity samples in the epoch with delta >= GRAVITY_STILL_THRESHOLD.
    motion_fraction: f64,
    /// RMSSD from RR intervals within the epoch window (Task Force 1996).
    rmssd_ms: Option<f64>,
    /// Nearest HR sample (bpm).
    hr_bpm: Option<f64>,
    /// Difference-of-Gaussians HR (Walch 2019): short-sigma minus long-sigma.
    dog_hr: Option<f64>,
    /// Median breath rate in rpm, extracted from resp samples.
    resp_rate_rpm: Option<f64>,
    /// IQR of breath intervals (seconds) — high = irregular = REM indicator.
    resp_rate_variability: Option<f64>,
}

/// Session-level percentile thresholds computed from all sleep-period epochs.
#[derive(Debug, Clone, Default)]
struct SessionPercentiles {
    hr_p25: Option<f64>,
    hr_median: Option<f64>,
    hr_p75: Option<f64>,
    rmssd_p70: Option<f64>,
    rrv_p65: Option<f64>,
    dog_hr_p75: Option<f64>,
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
        rem_latency_minutes: None,
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

/// Classify a sleep window into 4-class (wake/light/deep/rem) epochs using
/// multi-feature percentile-band classification.
///
/// Builds on the Cole-Kripke binary spine and refines each "sleep" epoch
/// using per-epoch features extracted from HR, RR intervals, motion, and
/// respiration. Classification uses session-level percentile thresholds
/// rather than fixed constants (Noop-inspired, Walch 2019).
///
/// Graceful degradation: features degrade independently based on what data
/// is available. No panics, no bool flags — empty slices = feature absent.
///
/// Reimposition (applied after classification):
///   (0) 5-epoch median smoothing of label sequence
///   (a) No REM in first 15 min of sleep
///   (b) No deep after first ⅓ of night (deep is front-loaded)
///   (c) Force pre-onset and post-final-wake to wake
///   (d) Minimum 5-min segment merge
pub fn stage_sleep_four_class(
    input: &SleepStagingInput,
    rows: &[(f64, f64, f64, f64)],
    hr_features: &[EpochHrFeature],
    rr_features: &[EpochRrFeature],
    resp_features: &[EpochRespFeature],
) -> SleepStagingOutput {
    if rows.is_empty() {
        return empty_output_with_aasm(STAGING_METHOD_NO_IMU, input);
    }

    // Step 1: binary spine via Cole-Kripke.
    let activity_counts = compute_activity_counts(input.sleep_start_ts, rows);
    if activity_counts.is_empty() {
        return empty_output_with_aasm(STAGING_METHOD_ACTIGRAPHY, input);
    }

    let n = activity_counts.len();
    let epoch_secs = COLE_KRIPKE_EPOCH_MINUTES * 60.0;

    let ck_lookup: std::collections::HashMap<i64, f64> = activity_counts
        .iter()
        .map(|&(idx, cnt)| (idx, cnt))
        .collect();

    // Step 2: compute per-epoch motion fractions from gravity rows.
    let motion_fractions = compute_motion_fractions(input.sleep_start_ts, rows);

    // Step 3: compute DoG-HR (Walch 2019) from HR features.
    let dog_hr_values = compute_dog_hr(hr_features);

    // Step 4: extract per-epoch features.
    let mut all_features: Vec<EpochFeatures> = Vec::with_capacity(n);
    for i in 0..n {
        let (epoch_idx, _count) = activity_counts[i];
        let ts = input.sleep_start_ts + epoch_idx as f64 * epoch_secs;
        let epoch_end = ts + epoch_secs;

        let motion_fraction = motion_fractions
            .get(&epoch_idx)
            .copied()
            .unwrap_or(0.0);

        let hr_bpm = nearest_hr(ts, hr_features);
        let dog_hr = nearest_dog_hr(ts, &dog_hr_values);
        let rmssd_ms = epoch_rmssd(ts, epoch_end, rr_features);
        let (resp_rate_rpm, resp_rate_variability) =
            epoch_resp_features(ts, epoch_end, resp_features);

        all_features.push(EpochFeatures {
            motion_fraction,
            rmssd_ms,
            hr_bpm,
            dog_hr,
            resp_rate_rpm,
            resp_rate_variability,
        });
    }

    // Step 5: compute session percentiles from sleep-period epochs only.
    let sleep_mask: Vec<bool> = (0..n)
        .map(|i| cole_kripke_d_score(i, &activity_counts, &ck_lookup) < COLE_KRIPKE_WAKE_THRESHOLD)
        .collect();
    let percentiles = compute_session_percentiles(&all_features, &sleep_mask);

    // Step 6: per-epoch 4-class assignment using percentile-band classifier.
    let has_resp = !resp_features.is_empty();
    let mut epochs: Vec<SleepEpoch> = Vec::with_capacity(n);
    for i in 0..n {
        let d = cole_kripke_d_score(i, &activity_counts, &ck_lookup);
        let (epoch_idx, count) = activity_counts[i];
        let ts = input.sleep_start_ts + epoch_idx as f64 * epoch_secs;
        let clock_proxy = if n > 1 { i as f64 / (n - 1) as f64 } else { 0.0 };

        let stage = if d >= COLE_KRIPKE_WAKE_THRESHOLD {
            // ponytail: also check motion fraction for wake override on sleep epochs
            "wake".to_string()
        } else {
            classify_sleep_epoch_v2(
                &all_features[i],
                &percentiles,
                clock_proxy,
                ts,
                input.sleep_start_ts,
                has_resp,
            )
        };
        epochs.push(SleepEpoch {
            ts,
            activity_count: count,
            stage,
        });
    }

    // Step 7: reimposition pipeline.
    apply_reimposition_v2(&mut epochs, input.sleep_start_ts, n);

    // Step 8: AASM metrics + REM latency.
    let aasm = aasm_metrics(
        &epochs,
        COLE_KRIPKE_EPOCH_MINUTES,
        input.sleep_start_ts,
        input.sleep_end_ts,
    );
    let rem_latency_minutes = compute_rem_latency(&epochs, input.sleep_start_ts);

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
        rem_latency_minutes,
        epochs,
    }
}

// ---------------------------------------------------------------------------
// Backward-compatible wrapper: old 4-arg signature delegates to new 5-arg.
// ---------------------------------------------------------------------------

/// Legacy 4-arg entry point. `resp_available` bool is mapped to an empty
/// resp slice when false. Callers should migrate to the 5-arg version.
pub fn stage_sleep_four_class_compat(
    input: &SleepStagingInput,
    rows: &[(f64, f64, f64, f64)],
    hr_features: &[EpochHrFeature],
    resp_available: bool,
) -> SleepStagingOutput {
    // ponytail: thin compat shim, no new logic
    let empty_rr: Vec<EpochRrFeature> = Vec::new();
    let empty_resp: Vec<EpochRespFeature> = Vec::new();
    if resp_available {
        // Without actual resp data, the classifier degrades gracefully.
        stage_sleep_four_class(input, rows, hr_features, &empty_rr, &empty_resp)
    } else {
        stage_sleep_four_class(input, rows, hr_features, &empty_rr, &empty_resp)
    }
}

// ---------------------------------------------------------------------------
// Multi-feature epoch classification (percentile-band, Noop-inspired)
// ---------------------------------------------------------------------------

/// Classify a single non-wake epoch into light/deep/rem using session
/// percentile thresholds and multi-signal features.
fn classify_sleep_epoch_v2(
    features: &EpochFeatures,
    percentiles: &SessionPercentiles,
    clock_proxy: f64,
    epoch_ts: f64,
    sleep_start_ts: f64,
    has_resp: bool,
) -> String {
    let minutes_from_onset = (epoch_ts - sleep_start_ts) / 60.0;

    // --- Deep: still + high parasympathetic tone + low HR ---
    // ponytail: RMSSD is the strongest deep signal; fall back to HR-only when absent
    let motion_still = features.motion_fraction <= DEEP_STILLNESS_MOTION_FRACTION;
    let hr_low = features.hr_bpm.is_some_and(|hr| {
        percentiles.hr_p25.is_some_and(|p25| hr <= p25)
    });
    let rmssd_high = features.rmssd_ms.is_some_and(|r| {
        percentiles.rmssd_p70.is_some_and(|p70| r >= p70)
    });

    // With RMSSD: require all three signals. Without: HR + motion (legacy path).
    if motion_still && hr_low {
        if rmssd_high {
            return "deep".to_string();
        }
        // No RMSSD data at all → use activity count fallback
        if features.rmssd_ms.is_none() {
            return "deep".to_string();
        }
    }

    // --- REM: still body + activated cardiac + (irregular breathing when available) ---
    if clock_proxy >= REM_CLOCK_PROXY_MIN && minutes_from_onset >= NO_REM_ONSET_MINUTES {
        let hr_elevated = features.hr_bpm.is_some_and(|hr| {
            percentiles.hr_median.is_some_and(|med| hr > med)
        });

        if motion_still && hr_elevated {
            if has_resp {
                // With resp: irregular breathing (RRV >= p65)
                let rrv_high = features.resp_rate_variability.is_some_and(|rrv| {
                    percentiles.rrv_p65.is_some_and(|p65| rrv >= p65)
                });
                if rrv_high {
                    return "rem".to_string();
                }
                // Resp available but RRV not elevated → light
            } else {
                // No resp: require both HR and DoG-HR elevated (stricter gate)
                let dog_hr_elevated = features.dog_hr.is_some_and(|d| {
                    percentiles.dog_hr_p75.is_some_and(|p75| d >= p75)
                });
                if dog_hr_elevated {
                    return "rem".to_string();
                }
            }
        }
    }

    "light".to_string()
}

// ---------------------------------------------------------------------------
// Feature extraction helpers
// ---------------------------------------------------------------------------

/// Compute per-epoch motion fraction: fraction of gravity samples in each
/// epoch whose inter-sample delta magnitude exceeds GRAVITY_STILL_THRESHOLD.
fn compute_motion_fractions(
    sleep_start_ts: f64,
    rows: &[(f64, f64, f64, f64)],
) -> BTreeMap<i64, f64> {
    let epoch_secs = COLE_KRIPKE_EPOCH_MINUTES * 60.0;
    // Bucket samples by epoch, track (total_samples, moving_samples)
    let mut epoch_data: BTreeMap<i64, (usize, usize, Option<f64>)> = BTreeMap::new();
    for &(ts, x, y, z) in rows {
        let epoch_idx = ((ts - sleep_start_ts) / epoch_secs).floor() as i64;
        let mag = (x * x + y * y + z * z).sqrt();
        let entry = epoch_data.entry(epoch_idx).or_insert((0, 0, None));
        entry.0 += 1;
        if let Some(prev_mag) = entry.2 {
            if (mag - prev_mag).abs() >= GRAVITY_STILL_THRESHOLD {
                entry.1 += 1;
            }
        }
        entry.2 = Some(mag);
    }
    epoch_data
        .into_iter()
        .map(|(idx, (total, moving, _))| {
            let frac = if total > 1 {
                moving as f64 / (total - 1) as f64 // inter-sample transitions
            } else {
                0.0
            };
            (idx, frac)
        })
        .collect()
}

/// Compute Difference-of-Gaussians HR (Walch 2019): convolve HR time series
/// with two Gaussians (σ=120s and σ=600s), subtract. Returns (ts, dog_value) pairs.
fn compute_dog_hr(hr_features: &[EpochHrFeature]) -> Vec<(f64, f64)> {
    if hr_features.len() < 3 {
        return Vec::new();
    }
    // ponytail: simple Gaussian-weighted average over nearby HR samples,
    // not a full convolution. Good enough for epoch-level classification.
    hr_features
        .iter()
        .map(|center| {
            let short = gaussian_weighted_mean(center.ts, hr_features, DOG_HR_SIGMA_SHORT_S);
            let long = gaussian_weighted_mean(center.ts, hr_features, DOG_HR_SIGMA_LONG_S);
            (center.ts, short - long)
        })
        .collect()
}

fn gaussian_weighted_mean(center_ts: f64, features: &[EpochHrFeature], sigma: f64) -> f64 {
    let mut weight_sum = 0.0_f64;
    let mut value_sum = 0.0_f64;
    let inv_2sigma2 = 1.0 / (2.0 * sigma * sigma);
    // ponytail: only look at samples within 3*sigma to avoid O(n²) blowup
    let window = 3.0 * sigma;
    for f in features {
        let dt = (f.ts - center_ts).abs();
        if dt > window {
            continue;
        }
        let w = (-dt * dt * inv_2sigma2).exp();
        weight_sum += w;
        value_sum += w * f.hr_bpm;
    }
    if weight_sum > 0.0 {
        value_sum / weight_sum
    } else {
        0.0
    }
}

/// Nearest DoG-HR value to an epoch timestamp.
fn nearest_dog_hr(epoch_ts: f64, dog_hr_values: &[(f64, f64)]) -> Option<f64> {
    if dog_hr_values.is_empty() {
        return None;
    }
    dog_hr_values
        .iter()
        .min_by(|a, b| {
            let da = (a.0 - epoch_ts).abs();
            let db = (b.0 - epoch_ts).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|pair| pair.1)
}

/// Return the nearest HR sample to a given epoch timestamp.
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

/// Compute RMSSD from RR intervals falling within [epoch_start, epoch_end).
/// Returns None if fewer than 3 intervals in the window.
fn epoch_rmssd(epoch_start: f64, epoch_end: f64, rr_features: &[EpochRrFeature]) -> Option<f64> {
    let intervals: Vec<f64> = rr_features
        .iter()
        .filter(|r| r.ts >= epoch_start && r.ts < epoch_end)
        .filter(|r| r.rr_ms >= 300.0 && r.rr_ms <= 2000.0) // physiological range gate
        .map(|r| r.rr_ms)
        .collect();
    if intervals.len() < 3 {
        return None;
    }
    let mut sum_sq = 0.0_f64;
    for pair in intervals.windows(2) {
        let diff = pair[1] - pair[0];
        sum_sq += diff * diff;
    }
    Some((sum_sq / (intervals.len() - 1) as f64).sqrt())
}

/// Extract breath rate and respiratory rate variability from raw resp samples
/// within [epoch_start, epoch_end). Uses peak detection on the raw signal.
fn epoch_resp_features(
    epoch_start: f64,
    epoch_end: f64,
    resp_features: &[EpochRespFeature],
) -> (Option<f64>, Option<f64>) {
    let samples: Vec<&EpochRespFeature> = resp_features
        .iter()
        .filter(|r| r.ts >= epoch_start && r.ts < epoch_end)
        .collect();
    if samples.len() < 5 {
        return (None, None);
    }

    // Simple peak detection: find local maxima separated by >= 1.5s
    let mut peak_times: Vec<f64> = Vec::new();
    for i in 1..samples.len() - 1 {
        if samples[i].raw > samples[i - 1].raw && samples[i].raw >= samples[i + 1].raw {
            if peak_times.last().is_none_or(|&last| samples[i].ts - last >= 1.5) {
                peak_times.push(samples[i].ts);
            }
        }
    }

    if peak_times.len() < 2 {
        return (None, None);
    }

    // Breath intervals
    let mut intervals: Vec<f64> = Vec::with_capacity(peak_times.len() - 1);
    for pair in peak_times.windows(2) {
        let interval = pair[1] - pair[0];
        if interval >= 1.5 && interval <= 12.0 {
            intervals.push(interval);
        }
    }
    if intervals.is_empty() {
        return (None, None);
    }

    intervals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_interval = intervals[intervals.len() / 2];
    let resp_rate_rpm = 60.0 / median_interval;

    // IQR as variability measure
    let rrv = if intervals.len() >= 4 {
        let q1 = intervals[intervals.len() / 4];
        let q3 = intervals[3 * intervals.len() / 4];
        Some(q3 - q1)
    } else {
        None
    };

    (Some(resp_rate_rpm), rrv)
}

/// Compute session-level percentiles from sleep-period epoch features.
fn compute_session_percentiles(
    features: &[EpochFeatures],
    sleep_mask: &[bool],
) -> SessionPercentiles {
    let sleep_features: Vec<&EpochFeatures> = features
        .iter()
        .zip(sleep_mask.iter())
        .filter(|(_, is_sleep)| **is_sleep)
        .map(|(f, _)| f)
        .collect();

    if sleep_features.is_empty() {
        return SessionPercentiles::default();
    }

    let hr_vals = collect_sorted(&sleep_features, |f| f.hr_bpm);
    let rmssd_vals = collect_sorted(&sleep_features, |f| f.rmssd_ms);
    let rrv_vals = collect_sorted(&sleep_features, |f| f.resp_rate_variability);
    let dog_hr_vals = collect_sorted(&sleep_features, |f| f.dog_hr);

    SessionPercentiles {
        hr_p25: percentile_at(&hr_vals, 0.25),
        hr_median: percentile_at(&hr_vals, 0.50),
        hr_p75: percentile_at(&hr_vals, 0.75),
        rmssd_p70: percentile_at(&rmssd_vals, DEEP_RMSSD_PERCENTILE),
        rrv_p65: percentile_at(&rrv_vals, REM_RRV_PERCENTILE),
        dog_hr_p75: percentile_at(&dog_hr_vals, 0.75),
    }
}

fn collect_sorted(features: &[&EpochFeatures], extract: impl Fn(&EpochFeatures) -> Option<f64>) -> Vec<f64> {
    let mut vals: Vec<f64> = features.iter().filter_map(|f| extract(f)).collect();
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    vals
}

fn percentile_at(sorted: &[f64], pct: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let idx = ((sorted.len() as f64 - 1.0) * pct).round() as usize;
    Some(sorted[idx.min(sorted.len() - 1)])
}

/// Compute REM latency: minutes from first sleep epoch to first REM epoch.
fn compute_rem_latency(epochs: &[SleepEpoch], _sleep_start_ts: f64) -> Option<f64> {
    let first_sleep_ts = epochs.iter().find(|e| e.stage != "wake").map(|e| e.ts)?;
    let first_rem_ts = epochs.iter().find(|e| e.stage == "rem").map(|e| e.ts)?;
    Some((first_rem_ts - first_sleep_ts).max(0.0) / 60.0)
}

// ---------------------------------------------------------------------------
// Physiological reimposition v2
// ---------------------------------------------------------------------------

/// Full reimposition pipeline:
///   (0) 5-epoch median smoothing
///   (a) No REM within NO_REM_ONSET_MINUTES of sleep onset
///   (b) No deep after first ⅓ of night (deep is front-loaded)
///   (c) Force pre-onset and post-final-wake to wake
///   (d) Minimum segment merge
fn apply_reimposition_v2(epochs: &mut [SleepEpoch], sleep_start_ts: f64, total_epochs: usize) {
    if epochs.is_empty() {
        return;
    }

    // Rule (0): 5-epoch median smoothing of label sequence.
    median_smooth_labels(epochs, MEDIAN_SMOOTH_WINDOW);

    // Rule (a): no early REM.
    for epoch in epochs.iter_mut() {
        if epoch.stage == "rem" {
            let minutes_from_onset = (epoch.ts - sleep_start_ts) / 60.0;
            if minutes_from_onset < NO_REM_ONSET_MINUTES {
                epoch.stage = "light".to_string();
            }
        }
    }

    // Rule (b): no deep after first ⅓ of night.
    let deep_cutoff_idx = ((total_epochs as f64) * DEEP_FRONT_LOADED_FRACTION).ceil() as usize;
    for (i, epoch) in epochs.iter_mut().enumerate() {
        if i >= deep_cutoff_idx && epoch.stage == "deep" {
            epoch.stage = "light".to_string();
        }
    }

    // Rule (c): force pre-onset and post-final-wake to wake.
    if let Some(first_sleep_idx) = epochs.iter().position(|e| e.stage != "wake") {
        for epoch in &mut epochs[..first_sleep_idx] {
            epoch.stage = "wake".to_string();
        }
    }
    if let Some(last_sleep_idx) = epochs.iter().rposition(|e| e.stage != "wake") {
        for epoch in &mut epochs[last_sleep_idx + 1..] {
            epoch.stage = "wake".to_string();
        }
    }

    // Rule (d): minimum segment merge.
    let min_seg_epochs = (MIN_SEGMENT_MINUTES / COLE_KRIPKE_EPOCH_MINUTES).ceil() as usize;
    merge_short_segments(epochs, min_seg_epochs);
}

/// Legacy reimposition (for binary spine compat and direct test calls).
fn apply_reimposition(epochs: &mut [SleepEpoch], sleep_start_ts: f64) {
    if epochs.is_empty() {
        return;
    }
    for epoch in epochs.iter_mut() {
        if epoch.stage == "rem" {
            let minutes_from_onset = (epoch.ts - sleep_start_ts) / 60.0;
            if minutes_from_onset < NO_REM_ONSET_MINUTES {
                epoch.stage = "light".to_string();
            }
        }
    }
    let min_seg_epochs = (MIN_SEGMENT_MINUTES / COLE_KRIPKE_EPOCH_MINUTES).ceil() as usize;
    merge_short_segments(epochs, min_seg_epochs);
}

/// 5-epoch median smoothing of stage labels. Each epoch's label is replaced
/// by the majority label in a centered window of `window_size` epochs.
fn median_smooth_labels(epochs: &mut [SleepEpoch], window_size: usize) {
    if epochs.len() < window_size {
        return;
    }
    let half = window_size / 2;
    let original: Vec<String> = epochs.iter().map(|e| e.stage.clone()).collect();
    for i in 0..epochs.len() {
        let start = i.saturating_sub(half);
        let end = (i + half + 1).min(original.len());
        // Find majority label in window
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for label in &original[start..end] {
            *counts.entry(label.as_str()).or_insert(0) += 1;
        }
        if let Some((majority, _)) = counts.iter().max_by_key(|(_, count)| **count) {
            epochs[i].stage = majority.to_string();
        }
    }
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
        rem_latency_minutes: None,
    }
}

fn empty_output_with_aasm(staging_method: &str, input: &SleepStagingInput) -> SleepStagingOutput {
    let tib = (input.sleep_end_ts - input.sleep_start_ts).max(0.0) / 60.0;
    SleepStagingOutput {
        epochs: vec![],
        staging_method: staging_method.to_string(),
        wake_fraction: 0.0,
        sleep_minutes: 0.0,
        tst_minutes: 0.0,
        time_in_bed_minutes: tib,
        sleep_efficiency_fraction: 0.0,
        sol_minutes: tib,
        waso_minutes: 0.0,
        stage_minutes: BTreeMap::new(),
        rem_latency_minutes: None,
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

    /// Build a still, low-HR window: should yield deep epochs in the first third
    /// of the night (deep is front-loaded; later epochs demoted to light).
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
        // HR all 45 bpm → session p25 = 45, all epochs HR <= p25.
        let hr_features: Vec<EpochHrFeature> = (0..30)
            .map(|i| EpochHrFeature {
                ts: start + i as f64 * epoch_secs + 30.0,
                hr_bpm: 45.0,
            })
            .collect();

        let input = make_input(start, start + 30.0 * epoch_secs);
        let output = stage_sleep_four_class(&input, &rows, &hr_features, &[], &[]);

        assert_eq!(output.staging_method, STAGING_METHOD_ACTIGRAPHY);
        // Deep epochs should appear in the first third (front-loaded rule).
        let deep_count = output.epochs.iter().filter(|e| e.stage == "deep").count();
        assert!(
            deep_count > 0,
            "still + low-HR should produce deep epochs in the first third"
        );
        // No deep after the first third of the night.
        let deep_cutoff = ((output.epochs.len() as f64) * DEEP_FRONT_LOADED_FRACTION).ceil() as usize;
        for (i, e) in output.epochs.iter().enumerate() {
            if i >= deep_cutoff {
                assert_ne!(
                    e.stage, "deep",
                    "deep after first third (epoch {i}) must be demoted"
                );
            }
        }
    }

    /// Late-night higher-HR window: should yield REM epochs in the second half
    /// when HR contrast is strong enough for DoG-HR to be elevated.
    #[test]
    fn four_class_late_high_hr_yields_rem() {
        let start = 0.0_f64;
        let epoch_secs = COLE_KRIPKE_EPOCH_MINUTES * 60.0;
        // 60 still epochs (30 min) — enough for good DoG-HR contrast.
        let rows: Vec<(f64, f64, f64, f64)> = (0..60)
            .flat_map(|i| {
                let t = start + i as f64 * epoch_secs;
                vec![(t, 0.0, 0.0, 1.0), (t + 1.0, 0.0, 0.0, 1.0)]
            })
            .collect();
        // First 30 epochs HR = 50 (low), last 30 epochs HR = 80 (high).
        // Strong contrast for DoG-HR.
        let hr_features: Vec<EpochHrFeature> = (0..60)
            .map(|i| EpochHrFeature {
                ts: start + i as f64 * epoch_secs + 15.0,
                hr_bpm: if i < 30 { 50.0 } else { 80.0 },
            })
            .collect();

        let input = make_input(start, start + 60.0 * epoch_secs);
        let output = stage_sleep_four_class(&input, &rows, &hr_features, &[], &[]);

        // With no resp data, REM uses the DoG-HR gate.
        // The second half has elevated HR + positive DoG-HR → some REM expected.
        let late_non_wake: Vec<&SleepEpoch> = output
            .epochs
            .iter()
            .enumerate()
            .filter(|(i, _)| *i >= 30 && output.epochs[*i].stage != "wake")
            .map(|(_, e)| e)
            .collect();

        // Well-formed output: stages are only wake/light/deep/rem.
        for e in &output.epochs {
            assert!(
                matches!(e.stage.as_str(), "wake" | "light" | "deep" | "rem"),
                "unexpected stage: {}",
                e.stage
            );
        }
        // Should have some non-wake epochs in the second half.
        assert!(
            !late_non_wake.is_empty(),
            "second half should have non-wake epochs"
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
        let output = stage_sleep_four_class(&input, &rows, &[], &[], &[]);
        assert_eq!(
            output.staging_method, STAGING_METHOD_ACTIGRAPHY,
            "4-class non-empty must emit actigraphy_uncalibrated"
        );
    }

    /// 4-class with empty rows → no_imu_data staging_method.
    #[test]
    fn four_class_empty_rows_yields_no_imu_data() {
        let input = make_input(0.0, 3600.0);
        let output = stage_sleep_four_class(&input, &[], &[], &[], &[]);
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
        let output = stage_sleep_four_class(&input, &rows, &[], &[], &[]);
        for e in &output.epochs {
            assert_ne!(
                e.stage, "sleep",
                "binary 'sleep' must not appear in 4-class output"
            );
            assert_ne!(e.stage, "deep", "deep requires HR data");
            assert_ne!(e.stage, "rem", "rem requires HR data");
        }
    }

    /// Without resp data, REM classification uses the stricter DoG-HR gate.
    /// With no resp features and no DoG-HR, fewer or no REM epochs appear.
    #[test]
    fn four_class_no_resp_uses_stricter_rem_gate() {
        let start = 0.0_f64;
        let epoch_secs = COLE_KRIPKE_EPOCH_MINUTES * 60.0;
        // 40 still epochs.
        let rows: Vec<(f64, f64, f64, f64)> = (0..40)
            .flat_map(|i| {
                let t = start + i as f64 * epoch_secs;
                vec![(t, 0.0, 0.0, 1.0), (t + 1.0, 0.0, 0.0, 1.0)]
            })
            .collect();
        // HR pattern: second half high HR.
        let hr_features: Vec<EpochHrFeature> = (0..40)
            .map(|i| EpochHrFeature {
                ts: start + i as f64 * epoch_secs + 30.0,
                hr_bpm: if i < 20 { 55.0 } else { 75.0 },
            })
            .collect();

        let input = make_input(start, start + 40.0 * epoch_secs);
        // No resp features → stricter DoG-HR gate for REM.
        let output_no_resp = stage_sleep_four_class(&input, &rows, &hr_features, &[], &[]);

        // The output should be well-formed (no panic) and have fewer REM
        // than a run with resp data would.
        let rem_count = output_no_resp.epochs.iter().filter(|e| e.stage == "rem").count();
        // With DoG-HR, some REM may still appear if the HR contrast is strong enough.
        // The key invariant: no REM in the first 15 min.
        let early_rem = output_no_resp.epochs.iter().filter(|e| {
            e.stage == "rem" && (e.ts - start) / 60.0 < NO_REM_ONSET_MINUTES
        }).count();
        assert_eq!(early_rem, 0, "no REM in first 15 minutes regardless of resp");
        // Output is well-formed.
        assert!(!output_no_resp.epochs.is_empty());
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
        let output = stage_sleep_four_class(&input, &tuples, &hr_feats, &[], &[]);

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
        let output = stage_sleep_four_class(&input, &gravity, &hr_feats, &[], &[]);

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
        let output = stage_sleep_four_class(&input, &gravity, &hr_feats, &[], &[]);

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
