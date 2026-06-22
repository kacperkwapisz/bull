/// Winsorized EWMA baseline engine for personal biometric baselines.
///
/// Ported from Noop's `Baselines.swift` (Winsorized EWMA production model).
/// Each nightly metric (HRV RMSSD, resting HR, resp rate, skin temp) maintains
/// an independent EWMA center + EWMA-of-absolute-deviation spread tracker.
///
/// Key properties:
///   - Robust: Winsor clamping (± 3× spread) + hard outlier rejection (> 5×)
///   - Cold-start gated: calibrating (< 4 nights) → provisional → trusted (14+)
///   - Anti-anchoring: faster adaptation in first 8 nights prevents a high seed
///     from locking the baseline (the Reddit HRV report fix)
///   - Staleness tracking: > 14 nights without update → stale
///   - Z-score: (value − baseline) / (1.253 × spread) where 1.253 converts
///     EWMA-abs-dev to approximate Gaussian σ
///
/// References: EWMA smoothing (Roberts 1959), Winsorization (Dixon 1960).
use crate::{BullResult, store::BullStore};

// ---------------------------------------------------------------------------
// MetricConfig — per-metric tuning
// ---------------------------------------------------------------------------

/// Per-metric configuration for physiological bounds, smoothing, and dispersion.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricConfig {
    /// Physiological lower bound — hard reject below.
    pub min_val: f64,
    /// Physiological upper bound — hard reject above.
    pub max_val: f64,
    /// Minimum dispersion (σ floor in abs-dev space).
    pub floor_spread: f64,
    /// Baseline center EWMA half-life (nights).
    pub half_life_b: f64,
    /// Spread EWMA half-life (nights, slower than center).
    pub half_life_s: f64,
}

/// HRV RMSSD config: 5–250 ms, floor σ = 5 ms.
pub const HRV_CONFIG: MetricConfig = MetricConfig {
    min_val: 5.0,
    max_val: 250.0,
    floor_spread: 5.0,
    half_life_b: 14.0,
    half_life_s: 21.0,
};

/// Resting HR config: 30–120 bpm, floor σ = 2 bpm.
pub const RHR_CONFIG: MetricConfig = MetricConfig {
    min_val: 30.0,
    max_val: 120.0,
    floor_spread: 2.0,
    half_life_b: 14.0,
    half_life_s: 21.0,
};

/// Respiratory rate config: 4–40 rpm, floor σ = 0.5 rpm.
pub const RESP_CONFIG: MetricConfig = MetricConfig {
    min_val: 4.0,
    max_val: 40.0,
    floor_spread: 0.5,
    half_life_b: 14.0,
    half_life_s: 21.0,
};

/// Skin temperature config: 20–42 °C, floor σ = 0.3 °C.
pub const SKIN_TEMP_CONFIG: MetricConfig = MetricConfig {
    min_val: 20.0,
    max_val: 42.0,
    floor_spread: 0.3,
    half_life_b: 14.0,
    half_life_s: 21.0,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Winsorization clamp: fold only within ± WINSOR_K × spread.
pub const WINSOR_K: f64 = 3.0;

/// Hard outlier gate: > HARD_OUTLIER_K × spread away → seen but not folded.
pub const HARD_OUTLIER_K: f64 = 5.0;

/// Minimum valid nights before baseline is usable (provisionally trusted).
pub const MIN_NIGHTS_SEED: usize = 4;

/// Minimum valid nights before fully trusted.
pub const MIN_NIGHTS_TRUST: usize = 14;

/// Missing-night count after which a baseline is marked stale.
pub const STALE_DAYS: usize = 14;

/// Valid-night count below which the baseline adapts faster (anti-anchoring).
pub const EARLY_ADAPT_NIGHTS: usize = 8;

/// Faster center half-life during early life (nights).
pub const EARLY_HALF_LIFE_B: f64 = 3.0;

/// Spread inflation during early life for Winsor clamping.
pub const EARLY_SPREAD_INFLATE: f64 = 2.5;

/// Converts EWMA-abs-dev to approximate Gaussian σ: E[|X−μ|] = σ·√(2/π) ≈ σ/1.253.
pub const ABS_DEV_TO_SIGMA: f64 = 1.253;

// ---------------------------------------------------------------------------
// BaselineStatus
// ---------------------------------------------------------------------------

/// Confidence status of the baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineStatus {
    /// < 4 nights — cold-start, no score.
    Calibrating,
    /// 4–13 nights — usable but higher uncertainty.
    Provisional,
    /// 14+ nights — statistically reliable.
    Trusted,
    /// Usable but no update for > 14 nights.
    Stale,
}

impl BaselineStatus {
    pub fn from_counts(n_valid: usize, nights_since_update: usize) -> Self {
        if nights_since_update > STALE_DAYS && n_valid >= MIN_NIGHTS_SEED {
            return Self::Stale;
        }
        if n_valid < MIN_NIGHTS_SEED {
            Self::Calibrating
        } else if n_valid < MIN_NIGHTS_TRUST {
            Self::Provisional
        } else {
            Self::Trusted
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Calibrating => "calibrating",
            Self::Provisional => "provisional",
            Self::Trusted => "trusted",
            Self::Stale => "stale",
        }
    }

    /// True if at least provisionally usable (n_valid ≥ MIN_NIGHTS_SEED).
    pub fn is_usable(&self) -> bool {
        matches!(self, Self::Provisional | Self::Trusted)
    }
}

// ---------------------------------------------------------------------------
// BaselineState
// ---------------------------------------------------------------------------

/// Immutable snapshot of a personal baseline for one metric.
#[derive(Debug, Clone, PartialEq)]
pub struct BaselineState {
    /// EWMA center (the personal "mean").
    pub baseline: f64,
    /// EWMA of absolute deviations, floored at config.floor_spread.
    /// Multiply by 1.253 to approximate Gaussian σ.
    pub spread: f64,
    /// Count of valid nights contributing to the state.
    pub n_valid: usize,
    /// Consecutive nights with no valid value (staleness).
    pub nights_since_update: usize,
    /// Derived status.
    pub status: BaselineStatus,
}

impl BaselineState {
    pub fn is_usable(&self) -> bool {
        self.status.is_usable()
    }

    /// Robust z-score: (value − baseline) / (1.253 × spread).
    /// Returns None when calibrating (cold-start).
    pub fn z_score(&self, value: f64) -> Option<f64> {
        if !self.status.is_usable() {
            return None;
        }
        let sigma = (ABS_DEV_TO_SIGMA * self.spread).max(1e-9);
        Some((value - self.baseline) / sigma)
    }
}

// ---------------------------------------------------------------------------
// Core EWMA update
// ---------------------------------------------------------------------------

/// Convert a half-life in nights to an EWMA smoothing factor (lambda).
fn lambda(half_life: f64) -> f64 {
    1.0 - 0.5_f64.powf(1.0 / half_life)
}

/// Incorporate one new nightly value into the baseline state.
///
/// - `state == None`: seed the first night.
/// - `value == None` or out-of-range: skip-and-hold (carry forward).
/// - Hard outlier (> 5× spread, only when not young): seen but not folded.
/// - Otherwise: Winsorized EWMA center + EWMA-abs-dev spread update.
pub fn baseline_update(
    state: Option<&BaselineState>,
    value: Option<f64>,
    cfg: &MetricConfig,
) -> BaselineState {
    let lb = lambda(cfg.half_life_b);
    let ls = lambda(cfg.half_life_s);

    // First night ever.
    let Some(state) = state else {
        if let Some(v) = value {
            if v.is_finite() && v >= cfg.min_val && v <= cfg.max_val {
                return BaselineState {
                    baseline: v,
                    spread: cfg.floor_spread,
                    n_valid: 1,
                    nights_since_update: 0,
                    status: BaselineStatus::Calibrating,
                };
            }
        }
        let seed = (cfg.min_val + cfg.max_val) / 2.0;
        return BaselineState {
            baseline: seed,
            spread: cfg.floor_spread,
            n_valid: 0,
            nights_since_update: 1,
            status: BaselineStatus::Calibrating,
        };
    };

    // Missing night: skip-and-hold.
    let Some(value) = value else {
        let m = state.nights_since_update + 1;
        return BaselineState {
            baseline: state.baseline,
            spread: state.spread,
            n_valid: state.n_valid,
            nights_since_update: m,
            status: BaselineStatus::from_counts(state.n_valid, m),
        };
    };

    // Physiological range gate.
    if !value.is_finite() || value < cfg.min_val || value > cfg.max_val {
        let m = state.nights_since_update + 1;
        return BaselineState {
            baseline: state.baseline,
            spread: state.spread,
            n_valid: state.n_valid,
            nights_since_update: m,
            status: BaselineStatus::from_counts(state.n_valid, m),
        };
    }

    let is_young = state.n_valid < EARLY_ADAPT_NIGHTS;

    // Hard outlier rejection (only once seeded AND no longer young).
    if state.n_valid >= MIN_NIGHTS_SEED && !is_young {
        let dev = (value - state.baseline).abs();
        if dev > HARD_OUTLIER_K * state.spread {
            return BaselineState {
                baseline: state.baseline,
                spread: state.spread,
                n_valid: state.n_valid,
                nights_since_update: 0,
                status: BaselineStatus::from_counts(state.n_valid, 0),
            };
        }
    }

    // First real value after a None-placeholder seed.
    if state.n_valid == 0 {
        return BaselineState {
            baseline: value,
            spread: cfg.floor_spread,
            n_valid: 1,
            nights_since_update: 0,
            status: BaselineStatus::Calibrating,
        };
    }

    // Winsorized EWMA update.
    let eff_spread = if is_young {
        state.spread * EARLY_SPREAD_INFLATE
    } else {
        state.spread
    };
    let eff_lb = if is_young {
        lambda(EARLY_HALF_LIFE_B)
    } else {
        lb
    };

    let lo = state.baseline - WINSOR_K * eff_spread;
    let hi = state.baseline + WINSOR_K * eff_spread;
    let clamped = value.clamp(lo, hi);
    let new_baseline = eff_lb * clamped + (1.0 - eff_lb) * state.baseline;

    // Spread uses UNCLAMPED value so true deviations are tracked.
    let abs_dev = (value - new_baseline).abs();
    let new_spread = (ls * abs_dev + (1.0 - ls) * state.spread).max(cfg.floor_spread);
    let new_n = state.n_valid + 1;

    BaselineState {
        baseline: new_baseline,
        spread: new_spread,
        n_valid: new_n,
        nights_since_update: 0,
        status: BaselineStatus::from_counts(new_n, 0),
    }
}

/// Replay an ordered sequence of nightly values (oldest first) to build state.
/// `None` entries are missing nights (skip-and-hold).
pub fn fold_history(values: &[Option<f64>], cfg: &MetricConfig) -> BaselineState {
    let mut state: Option<BaselineState> = None;
    for v in values {
        let next = baseline_update(state.as_ref(), *v, cfg);
        state = Some(next);
    }
    state.unwrap_or_else(|| {
        let seed = (cfg.min_val + cfg.max_val) / 2.0;
        BaselineState {
            baseline: seed,
            spread: cfg.floor_spread,
            n_valid: 0,
            nights_since_update: 0,
            status: BaselineStatus::Calibrating,
        }
    })
}

// ---------------------------------------------------------------------------
// Recovery scorer — z-score + logistic composite
// ---------------------------------------------------------------------------

/// Weights for the recovery composite (Noop's Charge model).
pub const W_HRV: f64 = 0.55;
pub const W_RHR: f64 = 0.20;
pub const W_SLEEP: f64 = 0.15;
pub const W_RESP: f64 = 0.05;
pub const W_SKIN_TEMP: f64 = 0.05;

/// Logistic spread: ±2 z-units ≈ full Red–Green band (15%–95%).
pub const LOGISTIC_K: f64 = 1.6;
/// Logistic offset so Z=0 → ~58% (population average recovery).
pub const LOGISTIC_Z0: f64 = -0.20;

/// Sleep performance center ("good night" at ~85% efficiency).
pub const SLEEP_PERF_CENTER: f64 = 0.85;
/// Sleep performance scale (±2 z spans the normal range).
pub const SLEEP_PERF_SCALE: f64 = 0.12;

/// Skin-temp penalty scale (°C): 1°C deviation ≈ 1 z-unit penalty.
pub const SKIN_TEMP_SCALE_C: f64 = 1.0;

/// Recovery band thresholds.
pub const BAND_RED_MAX: f64 = 34.0;
pub const BAND_YELLOW_MAX: f64 = 67.0;

/// Input for recovery scoring.
#[derive(Debug, Clone)]
pub struct RecoveryInput {
    /// Tonight's HRV RMSSD (ms).
    pub hrv: f64,
    /// Tonight's resting HR (bpm).
    pub rhr: f64,
    /// Tonight's respiration rate (optional).
    pub resp: Option<f64>,
    /// Sleep performance / rest quality (0–1, optional).
    pub sleep_perf: Option<f64>,
    /// Skin temperature deviation from baseline (±°C, optional).
    pub skin_temp_dev: Option<f64>,
}

/// Output of recovery scoring.
#[derive(Debug, Clone)]
pub struct RecoveryOutput {
    /// Recovery score 0–100.
    pub score: f64,
    /// "red", "yellow", or "green".
    pub band: &'static str,
    /// Composite z-score before logistic squash.
    pub composite_z: f64,
    /// Per-driver z-scores (for transparency).
    pub hrv_z: Option<f64>,
    pub rhr_z: Option<f64>,
    pub resp_z: Option<f64>,
    pub sleep_z: Option<f64>,
    pub skin_temp_z: Option<f64>,
}

/// Z-score + logistic recovery score (0–100). Returns None when HRV baseline
/// is not yet usable (cold-start).
pub fn recovery_score(
    input: &RecoveryInput,
    hrv_baseline: &BaselineState,
    rhr_baseline: Option<&BaselineState>,
    resp_baseline: Option<&BaselineState>,
) -> Option<RecoveryOutput> {
    // Cold-start gate: HRV is the dominant driver.
    if !hrv_baseline.is_usable() {
        return None;
    }

    let z_score_val = |value: f64, mean: f64, spread: f64| -> f64 {
        let sigma = (ABS_DEV_TO_SIGMA * spread).max(1e-9);
        (value - mean) / sigma
    };

    let mut terms: Vec<(f64, f64)> = Vec::new(); // (z, weight)

    // HRV: higher is better.
    let hrv_z = z_score_val(input.hrv, hrv_baseline.baseline, hrv_baseline.spread);
    terms.push((hrv_z, W_HRV));

    // RHR: lower is better → (mean − value) / σ.
    let rhr_z = rhr_baseline.and_then(|b| {
        if b.is_usable() {
            let z = z_score_val(b.baseline, input.rhr, b.spread); // note: swapped
            Some(z)
        } else {
            None
        }
    });
    if let Some(z) = rhr_z {
        terms.push((z, W_RHR));
    }

    // Resp: lower is better (optional).
    let resp_z = input.resp.and_then(|r| {
        resp_baseline.and_then(|b| {
            if b.is_usable() {
                Some(z_score_val(b.baseline, r, b.spread))
            } else {
                None
            }
        })
    });
    if let Some(z) = resp_z {
        terms.push((z, W_RESP));
    }

    // Sleep performance: no baseline needed; centered at SLEEP_PERF_CENTER.
    let sleep_z = input.sleep_perf.map(|sp| (sp - SLEEP_PERF_CENTER) / SLEEP_PERF_SCALE);
    if let Some(z) = sleep_z {
        terms.push((z, W_SLEEP));
    }

    // Skin temp: symmetric penalty on |deviation|.
    let skin_z = input.skin_temp_dev.map(|dev| -dev.abs() / SKIN_TEMP_SCALE_C);
    if let Some(z) = skin_z {
        terms.push((z, W_SKIN_TEMP));
    }

    if terms.is_empty() {
        return None;
    }

    let total_weight: f64 = terms.iter().map(|(_, w)| w).sum();
    if total_weight <= 0.0 {
        return None;
    }

    let composite_z: f64 = terms.iter().map(|(z, w)| z * w).sum::<f64>() / total_weight;
    let score = (100.0 / (1.0 + (-LOGISTIC_K * (composite_z - LOGISTIC_Z0)).exp()))
        .clamp(0.0, 100.0);

    let band = if score < BAND_RED_MAX {
        "red"
    } else if score < BAND_YELLOW_MAX {
        "yellow"
    } else {
        "green"
    };

    Some(RecoveryOutput {
        score,
        band,
        composite_z,
        hrv_z: Some(hrv_z),
        rhr_z,
        resp_z,
        sleep_z,
        skin_temp_z: skin_z,
    })
}

// ---------------------------------------------------------------------------
// Store integration — fold from daily_recovery_metrics
// ---------------------------------------------------------------------------

/// Per-device EWMA baselines (new Winsorized model).
#[derive(Debug, Clone)]
pub struct PersonalBaseline {
    pub hrv: BaselineState,
    pub resting_hr: BaselineState,
}

impl PersonalBaseline {
    /// Reconstruct baselines by replaying all `daily_recovery_metrics` rows.
    pub fn fold_from_store(store: &BullStore) -> BullResult<Self> {
        let rows = store.daily_recovery_metrics_all_ordered()?;
        let mut hrv_state: Option<BaselineState> = None;
        let mut rhr_state: Option<BaselineState> = None;
        for row in &rows {
            let hrv_val = row.hrv_rmssd_ms.filter(|v| v.is_finite());
            let rhr_val = row.resting_hr_bpm.filter(|v| v.is_finite());
            hrv_state = Some(baseline_update(hrv_state.as_ref(), hrv_val, &HRV_CONFIG));
            rhr_state = Some(baseline_update(rhr_state.as_ref(), rhr_val, &RHR_CONFIG));
        }
        Ok(Self {
            hrv: hrv_state.unwrap_or_else(|| fold_history(&[], &HRV_CONFIG)),
            resting_hr: rhr_state.unwrap_or_else(|| fold_history(&[], &RHR_CONFIG)),
        })
    }
}

/// Legacy per-device EWMA baselines (old variance-based model).
/// Kept for bridge compat — callers should migrate to PersonalBaseline.
#[derive(Debug, Clone, Default)]
pub struct EwmaBaseline {
    pub hrv: EwmaState,
    pub resting_hr: EwmaState,
}

impl EwmaBaseline {
    pub fn fold_history(store: &BullStore) -> BullResult<Self> {
        let rows = store.daily_recovery_metrics_all_ordered()?;
        let mut baseline = Self::default();
        for row in &rows {
            if let Some(hrv) = row.hrv_rmssd_ms {
                if hrv.is_finite() { baseline.hrv.fold(hrv); }
            }
            if let Some(rhr) = row.resting_hr_bpm {
                if rhr.is_finite() { baseline.resting_hr.fold(rhr); }
            }
        }
        Ok(baseline)
    }
}

// ponytail: EwmaState kept as a thin compat shim for existing callers. Remove when
// all callers migrate to BaselineState.

/// Legacy compat shim — wraps BaselineState for callers using the old API.
#[derive(Debug, Clone, Default)]
pub struct EwmaState {
    pub mean: f64,
    pub variance: f64,
    pub night_count: usize,
}

/// Legacy alpha constant (kept for test compat).
pub const ALPHA: f64 = 0.0483;

impl EwmaState {
    pub fn fold(&mut self, x: f64) {
        if self.night_count == 0 {
            self.mean = x;
            self.variance = 0.0;
        } else {
            let old_mean = self.mean;
            self.mean = (1.0 - ALPHA) * old_mean + ALPHA * x;
            self.variance = (1.0 - ALPHA) * self.variance + ALPHA * (x - old_mean).powi(2);
        }
        self.night_count += 1;
    }

    pub fn trust_level(&self) -> EwmaTrustLevel {
        EwmaTrustLevel::from_night_count(self.night_count)
    }

    pub fn is_ready(&self) -> bool {
        self.night_count >= 7
    }

    pub fn z_score(&self, value: f64) -> Option<f64> {
        if self.night_count < MIN_NIGHTS_SEED {
            return None;
        }
        let std_dev = self.variance.max(1e-6).sqrt();
        Some((value - self.mean) / std_dev)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EwmaTrustLevel {
    Calibrating,
    Provisional,
    Trusted,
}

impl EwmaTrustLevel {
    pub fn from_night_count(n: usize) -> Self {
        if n < MIN_NIGHTS_SEED {
            Self::Calibrating
        } else if n < MIN_NIGHTS_TRUST {
            Self::Provisional
        } else {
            Self::Trusted
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Calibrating => "calibrating",
            Self::Provisional => "provisional",
            Self::Trusted => "trusted",
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Winsorized EWMA baseline_update ----------------------------------

    #[test]
    fn test_first_night_seeds_baseline() {
        let s = baseline_update(None, Some(60.0), &HRV_CONFIG);
        assert_eq!(s.n_valid, 1);
        assert!((s.baseline - 60.0).abs() < 1e-9);
        assert!((s.spread - HRV_CONFIG.floor_spread).abs() < 1e-9);
        assert_eq!(s.status, BaselineStatus::Calibrating);
    }

    #[test]
    fn test_missing_night_skip_and_hold() {
        let s1 = baseline_update(None, Some(60.0), &HRV_CONFIG);
        let s2 = baseline_update(Some(&s1), None, &HRV_CONFIG);
        assert_eq!(s2.n_valid, 1);
        assert_eq!(s2.nights_since_update, 1);
        assert!((s2.baseline - 60.0).abs() < 1e-9);
    }

    #[test]
    fn test_out_of_range_rejected() {
        let s1 = baseline_update(None, Some(60.0), &HRV_CONFIG);
        // 300 ms is above HRV max (250)
        let s2 = baseline_update(Some(&s1), Some(300.0), &HRV_CONFIG);
        assert_eq!(s2.n_valid, 1, "out-of-range value must not increment n_valid");
    }

    #[test]
    fn test_fold_history_4_nights_is_provisional() {
        let vals: Vec<Option<f64>> = vec![Some(60.0), Some(58.0), Some(62.0), Some(59.0)];
        let s = fold_history(&vals, &HRV_CONFIG);
        assert_eq!(s.n_valid, 4);
        assert_eq!(s.status, BaselineStatus::Provisional);
        assert!(s.is_usable());
    }

    #[test]
    fn test_fold_history_14_nights_is_trusted() {
        let vals: Vec<Option<f64>> = (0..14).map(|i| Some(55.0 + i as f64)).collect();
        let s = fold_history(&vals, &HRV_CONFIG);
        assert_eq!(s.n_valid, 14);
        assert_eq!(s.status, BaselineStatus::Trusted);
    }

    #[test]
    fn test_staleness_after_14_missing() {
        let mut vals: Vec<Option<f64>> = (0..5).map(|i| Some(60.0 + i as f64)).collect();
        for _ in 0..15 {
            vals.push(None);
        }
        let s = fold_history(&vals, &HRV_CONFIG);
        assert_eq!(s.status, BaselineStatus::Stale);
    }

    #[test]
    fn test_z_score_none_when_calibrating() {
        let vals: Vec<Option<f64>> = vec![Some(60.0), Some(58.0), Some(62.0)];
        let s = fold_history(&vals, &HRV_CONFIG);
        assert!(s.z_score(65.0).is_none());
    }

    #[test]
    fn test_z_score_some_when_provisional() {
        let vals: Vec<Option<f64>> = vec![Some(60.0), Some(58.0), Some(62.0), Some(59.0)];
        let s = fold_history(&vals, &HRV_CONFIG);
        assert!(s.z_score(65.0).is_some());
    }

    #[test]
    fn test_anti_anchoring_pulls_down_from_high_seed() {
        // First few nights are artificially high, then real values come in lower.
        let vals: Vec<Option<f64>> = vec![
            Some(120.0), Some(115.0), Some(110.0), // high seed
            Some(55.0), Some(52.0), Some(54.0), Some(53.0), Some(55.0), // real values
        ];
        let s = fold_history(&vals, &HRV_CONFIG);
        // Without anti-anchoring, baseline would stay near ~110. With it, should converge toward ~55.
        assert!(
            s.baseline < 80.0,
            "anti-anchoring should pull baseline below 80, got {}",
            s.baseline
        );
    }

    #[test]
    fn test_winsor_clamp_limits_extreme_jumps() {
        // Build a stable baseline at 60, then inject a value just within hard-outlier range
        let mut vals: Vec<Option<f64>> = (0..20).map(|_| Some(60.0)).collect();
        vals.push(Some(80.0)); // moderate jump
        let s = fold_history(&vals, &HRV_CONFIG);
        // Baseline should move toward 80 but be Winsor-clamped, not jumping all the way
        assert!(
            s.baseline < 65.0,
            "Winsor clamp should limit shift, got {}",
            s.baseline
        );
        assert!(s.baseline > 60.0, "should still move slightly toward 80");
    }

    // ---- Recovery scorer --------------------------------------------------

    #[test]
    fn test_recovery_cold_start_returns_none() {
        let hrv_bl = fold_history(&[Some(60.0), Some(58.0)], &HRV_CONFIG); // only 2 nights
        let input = RecoveryInput {
            hrv: 60.0,
            rhr: 55.0,
            resp: None,
            sleep_perf: None,
            skin_temp_dev: None,
        };
        assert!(recovery_score(&input, &hrv_bl, None, None).is_none());
    }

    #[test]
    fn test_recovery_at_baseline_is_around_58() {
        // Z=0 for all drivers → logistic should yield ~58%
        let hrv_bl = fold_history(
            &(0..14).map(|_| Some(60.0)).collect::<Vec<_>>(),
            &HRV_CONFIG,
        );
        let rhr_bl = fold_history(
            &(0..14).map(|_| Some(55.0)).collect::<Vec<_>>(),
            &RHR_CONFIG,
        );
        let input = RecoveryInput {
            hrv: 60.0,
            rhr: 55.0,
            resp: None,
            sleep_perf: Some(SLEEP_PERF_CENTER),
            skin_temp_dev: Some(0.0),
        };
        let out = recovery_score(&input, &hrv_bl, Some(&rhr_bl), None).unwrap();
        assert!(
            (out.score - 58.0).abs() < 5.0,
            "at-baseline recovery should be ~58%, got {}",
            out.score
        );
    }

    #[test]
    fn test_recovery_high_hrv_gives_green() {
        let hrv_bl = fold_history(
            &(0..14).map(|_| Some(60.0)).collect::<Vec<_>>(),
            &HRV_CONFIG,
        );
        let input = RecoveryInput {
            hrv: 90.0, // well above baseline
            rhr: 50.0,
            resp: None,
            sleep_perf: Some(0.90),
            skin_temp_dev: None,
        };
        let out = recovery_score(&input, &hrv_bl, None, None).unwrap();
        assert_eq!(out.band, "green", "high HRV should give green, score={}", out.score);
    }

    #[test]
    fn test_recovery_low_hrv_gives_red() {
        let hrv_bl = fold_history(
            &(0..14).map(|_| Some(60.0)).collect::<Vec<_>>(),
            &HRV_CONFIG,
        );
        let input = RecoveryInput {
            hrv: 25.0, // well below baseline
            rhr: 75.0,
            resp: None,
            sleep_perf: Some(0.50),
            skin_temp_dev: None,
        };
        let out = recovery_score(&input, &hrv_bl, None, None).unwrap();
        assert_eq!(out.band, "red", "low HRV should give red, score={}", out.score);
    }

    // ---- Legacy compat shim -----------------------------------------------

    #[test]
    fn test_legacy_ewma_state_fold() {
        let mut state = EwmaState::default();
        state.fold(60.0);
        assert_eq!(state.night_count, 1);
        assert!((state.mean - 60.0).abs() < 1e-9);
    }

    #[test]
    fn test_legacy_trust_levels() {
        assert_eq!(EwmaTrustLevel::from_night_count(3), EwmaTrustLevel::Calibrating);
        assert_eq!(EwmaTrustLevel::from_night_count(4), EwmaTrustLevel::Provisional);
        assert_eq!(EwmaTrustLevel::from_night_count(14), EwmaTrustLevel::Trusted);
    }

    // ---- Store integration ------------------------------------------------

    fn insert_test_recovery_row(
        store: &BullStore,
        date_key: &str,
        hrv: Option<f64>,
        rhr: Option<f64>,
    ) {
        use crate::store::DailyRecoveryMetricInput;
        let id = format!("test-{}", date_key);
        store
            .insert_daily_recovery_metric(DailyRecoveryMetricInput {
                daily_metric_id: &id,
                date_key,
                timezone: "UTC",
                start_time_unix_ms: 1_700_000_000_000,
                end_time_unix_ms: 1_700_003_600_000,
                hrv_rmssd_ms: hrv,
                resting_hr_bpm: rhr,
                respiratory_rate_rpm: None,
                oxygen_saturation_percent: None,
                skin_temperature_delta_c: None,
                source_kind: "local_estimate",
                confidence: 1.0,
                inputs_json: "{}",
                quality_flags_json: "[]",
                provenance_json: "{}",
            })
            .expect("insert test row");
    }

    #[test]
    fn test_personal_baseline_empty_store() {
        let store = BullStore::open_in_memory().expect("store");
        let bl = PersonalBaseline::fold_from_store(&store).expect("fold");
        assert_eq!(bl.hrv.n_valid, 0);
        assert_eq!(bl.resting_hr.n_valid, 0);
    }

    #[test]
    fn test_personal_baseline_from_store() {
        let store = BullStore::open_in_memory().expect("store");
        insert_test_recovery_row(&store, "2024-01-01", Some(60.0), Some(55.0));
        insert_test_recovery_row(&store, "2024-01-02", Some(58.0), Some(56.0));
        insert_test_recovery_row(&store, "2024-01-03", Some(62.0), Some(54.0));
        let bl = PersonalBaseline::fold_from_store(&store).expect("fold");
        assert_eq!(bl.hrv.n_valid, 3);
        assert_eq!(bl.resting_hr.n_valid, 3);
    }

    #[test]
    fn test_personal_baseline_skips_null() {
        let store = BullStore::open_in_memory().expect("store");
        insert_test_recovery_row(&store, "2024-01-01", Some(60.0), Some(55.0));
        insert_test_recovery_row(&store, "2024-01-02", None, Some(56.0));
        insert_test_recovery_row(&store, "2024-01-03", Some(62.0), None);
        let bl = PersonalBaseline::fold_from_store(&store).expect("fold");
        assert_eq!(bl.hrv.n_valid, 2);
        assert_eq!(bl.resting_hr.n_valid, 2);
    }

    // Legacy EwmaBaseline compat
    #[test]
    fn test_legacy_fold_history_empty_store() {
        let store = BullStore::open_in_memory().expect("store");
        let bl = EwmaBaseline::fold_history(&store).expect("fold");
        assert_eq!(bl.hrv.night_count, 0);
    }

    #[test]
    fn test_legacy_fold_history_from_store() {
        let store = BullStore::open_in_memory().expect("store");
        insert_test_recovery_row(&store, "2024-01-01", Some(60.0), Some(55.0));
        insert_test_recovery_row(&store, "2024-01-02", Some(58.0), Some(56.0));
        let bl = EwmaBaseline::fold_history(&store).expect("fold");
        assert_eq!(bl.hrv.night_count, 2);
    }
}
