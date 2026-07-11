//! PPG (photoplethysmogram) signal processing for heart-rate and RR-interval
//! extraction from R20 optical frames.
//!
//! The green-LED PPG waveform has a large DC offset (~200K) with a small AC
//! pulsatile component (~1–5% of DC). Each cardiac cycle produces one peak in
//! the AC signal. The pipeline:
//!   1. Remove DC offset (high-pass / subtract running mean)
//!   2. Bandpass filter ~0.5–4 Hz (30–240 BPM)
//!   3. Peak detection → beat timestamps
//!   4. RR intervals → instantaneous HR + RMSSD HRV

/// Malik-style ectopic rejection (Malik et al. 1989).
/// Drops any beat whose RR interval deviates > 20% from a centered 5-beat
/// local median. Beats with too few neighbours are kept.
fn malik_ectopic_filter(nn: &[f64]) -> Vec<f64> {
    const THRESHOLD: f64 = 0.20;
    const RADIUS: usize = 2; // window = 2*2+1 = 5 beats
    if nn.len() <= RADIUS {
        return nn.to_vec();
    }
    let mut kept = Vec::with_capacity(nn.len());
    for i in 0..nn.len() {
        let lo = i.saturating_sub(RADIUS);
        let hi = (i + RADIUS + 1).min(nn.len());
        let mut neighbours: Vec<f64> = (lo..hi).filter(|&j| j != i).map(|j| nn[j]).collect();
        if neighbours.len() < 2 {
            kept.push(nn[i]);
            continue;
        }
        neighbours.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let med = if neighbours.len() % 2 == 1 {
            neighbours[neighbours.len() / 2]
        } else {
            (neighbours[neighbours.len() / 2 - 1] + neighbours[neighbours.len() / 2]) / 2.0
        };
        if med <= 0.0 {
            kept.push(nn[i]);
            continue;
        }
        let deviation = (nn[i] - med).abs() / med;
        if deviation <= THRESHOLD {
            kept.push(nn[i]);
        }
        // else: drop as ectopic
    }
    kept
}

/// Result of processing a window of PPG samples.
#[derive(Debug, Clone)]
pub struct PpgHeartRateResult {
    /// Detected beat-to-beat intervals in milliseconds.
    pub rr_intervals_ms: Vec<f64>,
    /// Mean heart rate in BPM (from mean RR), or None if < 2 beats.
    pub mean_hr_bpm: Option<f64>,
    /// RMSSD HRV in milliseconds, or None if < 2 RR intervals.
    pub rmssd_ms: Option<f64>,
    /// Number of peaks (beats) detected.
    pub beat_count: usize,
    /// Quality flags for downstream consumers.
    pub quality_flags: Vec<String>,
}

/// Extract heart rate and RR intervals from a sequence of green PPG i32 samples.
///
/// `sample_rate_hz`: the sampling rate of the PPG signal. For R20 historical
/// data with 25 samples per frame, the rate depends on the frame interval.
/// If frames arrive at 1 Hz, sample_rate = 25 Hz.
///
/// `samples`: concatenated green PPG i32 values from consecutive R20 frames.
pub fn extract_hr_from_ppg(samples: &[i32], sample_rate_hz: f64) -> PpgHeartRateResult {
    let mut quality_flags = Vec::new();

    if samples.len() < 10 || sample_rate_hz <= 0.0 {
        return PpgHeartRateResult {
            rr_intervals_ms: Vec::new(),
            mean_hr_bpm: None,
            rmssd_ms: None,
            beat_count: 0,
            quality_flags: vec!["insufficient_samples".into()],
        };
    }

    // 1. Convert to f64 and remove DC offset (subtract per-window mean).
    let mean = samples.iter().map(|s| *s as f64).sum::<f64>() / samples.len() as f64;
    let ac: Vec<f64> = samples.iter().map(|s| *s as f64 - mean).collect();

    // 2. 4th-order Butterworth bandpass 0.7–3.5 Hz (42–210 BPM).
    //    Two cascaded biquad sections computed from sample rate via bilinear transform.
    let filtered = butterworth_bandpass(&ac, 0.7, 3.5, sample_rate_hz);

    // 3. Peak detection: find local maxima above a dynamic threshold.
    //
    // Minimum peak spacing comes from the signal's own autocorrelation
    // fundamental when one is detectable. A resting pulse waveform carries a
    // prominent dicrotic (secondary) peak per cardiac cycle; with a fixed
    // 240-BPM spacing the detector counts it as a beat and reports a clean,
    // consistent 2x heart rate that interval filters cannot see. Spacing tied
    // to the autocorrelation period suppresses intra-cycle peaks generically.
    let fallback_spacing = (sample_rate_hz / 4.0).max(2.0) as usize;
    let min_spacing = match autocorrelation_period_samples(&filtered, sample_rate_hz) {
        Some(period_samples) => ((period_samples * 0.7) as usize).max(fallback_spacing),
        None => {
            quality_flags.push("autocorrelation_period_unavailable".into());
            fallback_spacing
        }
    };
    let peaks = detect_peaks(&filtered, min_spacing);

    if peaks.len() < 2 {
        quality_flags.push("fewer_than_2_peaks".into());
        return PpgHeartRateResult {
            rr_intervals_ms: Vec::new(),
            mean_hr_bpm: None,
            rmssd_ms: None,
            beat_count: peaks.len(),
            quality_flags,
        };
    }

    // 4. Compute RR intervals in milliseconds.
    let rr_intervals_ms: Vec<f64> = peaks
        .windows(2)
        .map(|pair| (pair[1] - pair[0]) as f64 / sample_rate_hz * 1000.0)
        .collect();

    // Step 1: Range filter — drop physiologically implausible intervals
    // (< 300ms = 200 BPM, > 2000ms = 30 BPM). Task Force 1996.
    let range_filtered: Vec<f64> = rr_intervals_ms
        .iter()
        .copied()
        .filter(|rr| (300.0..=2000.0).contains(rr))
        .collect();

    // Step 2: Malik ectopic rejection — drop beats deviating > 20% from
    // local 5-beat median (Malik et al. 1989). Removes physiologically
    // impossible beat-to-beat jumps before computing HRV.
    let valid_rr = malik_ectopic_filter(&range_filtered);

    if valid_rr.is_empty() {
        quality_flags.push("no_plausible_rr_intervals".into());
        return PpgHeartRateResult {
            rr_intervals_ms: Vec::new(),
            mean_hr_bpm: None,
            rmssd_ms: None,
            beat_count: peaks.len(),
            quality_flags,
        };
    }

    // Mean HR from mean RR.
    let mean_rr = valid_rr.iter().sum::<f64>() / valid_rr.len() as f64;
    let mean_hr_bpm = 60_000.0 / mean_rr;

    // RMSSD: root mean square of successive differences (Task Force 1996).
    let rmssd_ms = if valid_rr.len() >= 2 {
        let sum_sq_diff: f64 = valid_rr
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).powi(2))
            .sum();
        let raw_rmssd = (sum_sq_diff / (valid_rr.len() - 1) as f64).sqrt();
        // At 25 Hz, peak detection has ±40ms resolution. Subtract the
        // quantization noise floor: RMSSD_corrected = sqrt(RMSSD² − noise²)
        // where noise ≈ sample_interval / sqrt(2) ≈ 28ms at 25 Hz.
        let sample_interval_ms = 1000.0 / sample_rate_hz;
        let noise_floor_ms = sample_interval_ms / std::f64::consts::SQRT_2;
        let corrected = if raw_rmssd > noise_floor_ms {
            (raw_rmssd.powi(2) - noise_floor_ms.powi(2)).sqrt()
        } else {
            0.0
        };
        // Cap at physiological maximum (150ms). Above this is almost certainly
        // PPG artifact, not real HRV — even elite athletes rarely exceed 120ms.
        let capped = corrected.min(150.0);
        if raw_rmssd > 150.0 {
            quality_flags.push("rmssd_capped_ppg_jitter".into());
        }
        Some(capped)
    } else {
        None
    };

    // Plausibility check on HR.
    if !(25.0..=250.0).contains(&mean_hr_bpm) {
        quality_flags.push("hr_outside_plausible_range".into());
    }

    PpgHeartRateResult {
        rr_intervals_ms: valid_rr,
        mean_hr_bpm: Some(mean_hr_bpm),
        rmssd_ms,
        beat_count: peaks.len(),
        quality_flags,
    }
}

/// Cardiac fundamental period (in samples) from normalized autocorrelation.
///
/// Scans lags spanning 40–200 BPM and returns the strongest lag, preferring a
/// subharmonic (double lag) when it correlates nearly as well: peak-picking
/// errors report harmonics of the true pulse rate, never subharmonics, so when
/// both lag and 2×lag fit the waveform the longer period is the fundamental.
/// Returns `None` when no lag correlates convincingly (motion artifact, poor
/// perfusion) — the caller falls back to conservative spacing.
fn autocorrelation_period_samples(signal: &[f64], sample_rate_hz: f64) -> Option<f64> {
    let lag_min = (sample_rate_hz * 60.0 / 200.0).round() as usize; // 200 BPM
    let lag_max = (sample_rate_hz * 60.0 / 40.0).round() as usize; // 40 BPM
    if signal.len() < lag_max * 2 || lag_min < 2 {
        return None;
    }
    let normalized_autocorr = |lag: usize| -> f64 {
        let n = signal.len() - lag;
        let mut cross = 0.0;
        let mut energy_a = 0.0;
        let mut energy_b = 0.0;
        for i in 0..n {
            cross += signal[i] * signal[i + lag];
            energy_a += signal[i] * signal[i];
            energy_b += signal[i + lag] * signal[i + lag];
        }
        let denom = (energy_a * energy_b).sqrt();
        if denom <= f64::EPSILON { 0.0 } else { cross / denom }
    };
    let mut best_lag = 0usize;
    let mut best_r = f64::MIN;
    for lag in lag_min..=lag_max {
        let r = normalized_autocorr(lag);
        if r > best_r {
            best_r = r;
            best_lag = lag;
        }
    }
    // A weak best correlation is not a cardiac rhythm.
    const MIN_FUNDAMENTAL_CORRELATION: f64 = 0.30;
    if best_r < MIN_FUNDAMENTAL_CORRELATION {
        return None;
    }
    // Subharmonic preference (run twice: catches 2x and 4x harmonic picks).
    const SUBHARMONIC_ACCEPT_FRACTION: f64 = 0.75;
    for _ in 0..2 {
        let double = best_lag * 2;
        if double > lag_max || signal.len() < double * 2 {
            break;
        }
        let r_double = normalized_autocorr(double);
        if r_double >= best_r * SUBHARMONIC_ACCEPT_FRACTION {
            best_lag = double;
            best_r = r_double;
        } else {
            break;
        }
    }
    Some(best_lag as f64)
}

/// 4th-order Butterworth bandpass implemented as two cascaded biquad sections.
/// Coefficients derived at runtime from (f_low, f_high, fs) via bilinear transform.
fn butterworth_bandpass(signal: &[f64], f_low: f64, f_high: f64, fs: f64) -> Vec<f64> {
    let sections = butter_bandpass_sos(f_low, f_high, fs);
    let mut out = signal.to_vec();
    for s in &sections {
        out = sosfilt_section(s, &out);
    }
    out
}

/// One second-order section: [b0, b1, b2, a0(=1), a1, a2].
type Sos = [f64; 6];

/// Design a 4th-order Butterworth bandpass as two SOS biquads.
/// Uses the standard analog prototype → bilinear-transform approach.
fn butter_bandpass_sos(f_low: f64, f_high: f64, fs: f64) -> [Sos; 2] {
    use std::f64::consts::PI;
    // Pre-warp analog frequencies
    let w_low = 2.0 * fs * (PI * f_low / fs).tan();
    let w_high = 2.0 * fs * (PI * f_high / fs).tan();
    let w0 = (w_low * w_high).sqrt();
    let bw = w_high - w_low;

    // 2nd-order Butterworth analog lowpass poles: s = e^{j*pi*(2k+n+1)/(2n)}, n=2
    // For n=2: poles at angles 3π/4 and 5π/4, i.e. conjugate pair with
    // real = -sin(π/4) = -√2/2, imag = ±cos(π/4) = ±√2/2
    // Analog bandpass transform of each pole pair gives two biquad sections.
    let sqrt2 = std::f64::consts::SQRT_2;

    // Section 1: lowpass-derived bandpass section
    // Analog: H_lp(s) = 1/(s^2 + √2·s + 1), bandwidth-transformed to bandpass
    // Using matched bilinear for the bandpass directly:
    // Pre-warped center and bandwidth give us the digital biquad coefficients.
    let q1 = w0 / (bw * sqrt2 / 2.0 + (bw * bw / 4.0 + w0 * w0).sqrt() - w0);
    let q2 = w0 / (bw * sqrt2 / 2.0 - (bw * bw / 4.0 + w0 * w0).sqrt() + w0).abs();

    // Actually, let's use the direct bilinear transform of each analog section.
    // It's cleaner to compute via the cookbook formulas for bandpass biquads.
    // For a bandpass biquad with center w0 and bandwidth bw at sample rate fs:
    //   w0_d = 2π * f0 / fs  (digital center frequency)
    //   α = sin(w0_d) * sinh(ln(2)/2 * bw_d / sin(w0_d))
    // But for a 4th-order Butterworth we need two sections with different Q.

    let f0 = (f_low * f_high).sqrt();
    let w0_d = 2.0 * PI * f0 / fs;
    let cos_w0 = w0_d.cos();
    let sin_w0 = w0_d.sin();

    // 2nd-order Butterworth has two conjugate pole pairs that, when bandpass-
    // transformed, give two biquads. The Q factors for a 2nd-order Butterworth
    // bandpass are derived from the analog prototype poles.
    // For Butterworth order N=2, the analog poles have Q = 1/√2 ≈ 0.7071.
    // The bandpass transform splits each pole into two, with effective Q values:
    let bw_ratio = (f_high - f_low) / f0;
    // Two quality factors from the 2nd-order Butterworth prototype
    let q_factors = [
        1.0 / (bw_ratio * 0.5 * sqrt2),  // Higher Q section
        1.0 / (bw_ratio * sqrt2),          // Lower Q section  
    ];

    let mut sections = [[0.0f64; 6]; 2];
    for (i, &q) in q_factors.iter().enumerate() {
        let alpha = sin_w0 / (2.0 * q);
        // Bandpass biquad (constant-0dB-peak-gain form):
        let b0 = alpha;
        let b1 = 0.0;
        let b2 = -alpha;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;
        sections[i] = [b0 / a0, b1 / a0, b2 / a0, 1.0, a1 / a0, a2 / a0];
    }
    sections
}

/// Apply one SOS biquad section (direct form II transposed).
fn sosfilt_section(sos: &Sos, input: &[f64]) -> Vec<f64> {
    let [b0, b1, b2, _a0, a1, a2] = *sos;
    let mut out = Vec::with_capacity(input.len());
    let (mut z1, mut z2) = (0.0, 0.0);
    for &x in input {
        let y = b0 * x + z1;
        z1 = b1 * x - a1 * y + z2;
        z2 = b2 * x - a2 * y;
        out.push(y);
    }
    out
}

/// Moving-average bandpass: high-pass by subtracting a long MA, then low-pass
/// with a short MA. Kept as fallback.
#[allow(dead_code)]
fn bandpass_ma(signal: &[f64], hp_window: usize, lp_window: usize) -> Vec<f64> {
    let hp = subtract_moving_average(signal, hp_window);
    moving_average(&hp, lp_window)
}

fn subtract_moving_average(signal: &[f64], window: usize) -> Vec<f64> {
    let ma = moving_average(signal, window);
    signal.iter().zip(ma.iter()).map(|(s, m)| s - m).collect()
}

fn moving_average(signal: &[f64], window: usize) -> Vec<f64> {
    if window <= 1 || signal.is_empty() {
        return signal.to_vec();
    }
    let half = window / 2;
    let mut result = Vec::with_capacity(signal.len());
    let mut sum = 0.0;
    let mut count = 0usize;
    // Initialize with first `half` elements.
    for i in 0..signal.len() {
        // Add the right edge.
        let right = i + half;
        if right < signal.len() && count < window {
            sum += signal[right.min(signal.len() - 1)];
            count += 1;
        }
        // For the first `half` samples, keep expanding the window.
        if i <= half {
            sum += signal[i];
            count += 1;
        }
        // Remove the left edge when past the full window.
        if i > half {
            sum += signal[(i + half).min(signal.len() - 1)];
            count += 1;
            if count > window {
                sum -= signal[i - half - 1];
                count -= 1;
            }
        }
        result.push(sum / count as f64);
    }
    // ponytail: simple causal MA; the windowed version above has edge effects
    // but works well enough for PPG where we care about the middle of the window.
    // If quality is insufficient, replace with a proper symmetric FIR.
    result
}

/// Detect peaks (local maxima) in the signal with minimum spacing.
/// Uses adaptive threshold at 40% of the rolling max amplitude.
fn detect_peaks(signal: &[f64], min_spacing: usize) -> Vec<usize> {
    if signal.len() < 3 {
        return Vec::new();
    }

    // Compute signal amplitude range for threshold.
    let max_val = signal.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min_val = signal.iter().cloned().fold(f64::INFINITY, f64::min);
    let threshold = min_val + (max_val - min_val) * 0.4;

    let mut peaks = Vec::new();
    let mut last_peak: Option<usize> = None;

    for i in 1..signal.len() - 1 {
        if signal[i] > signal[i - 1]
            && signal[i] >= signal[i + 1]
            && signal[i] > threshold
        {
            if let Some(lp) = last_peak {
                if i - lp < min_spacing {
                    // Too close — keep the taller peak.
                    if signal[i] > signal[lp] {
                        *peaks.last_mut().unwrap() = i;
                        last_peak = Some(i);
                    }
                    continue;
                }
            }
            peaks.push(i);
            last_peak = Some(i);
        }
    }
    peaks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_synthetic_sine_ppg() {
        // Generate a synthetic 1 Hz sine wave (60 BPM) at 25 Hz sample rate,
        // 10 seconds = 250 samples.
        let sample_rate = 25.0;
        let duration_s = 10.0;
        let hr_hz = 1.0; // 60 BPM
        let n = (sample_rate * duration_s) as usize;
        let samples: Vec<i32> = (0..n)
            .map(|i| {
                let t = i as f64 / sample_rate;
                // DC offset 200_000 + AC amplitude 5_000
                (200_000.0 + 5_000.0 * (2.0 * std::f64::consts::PI * hr_hz * t).sin()) as i32
            })
            .collect();

        let result = extract_hr_from_ppg(&samples, sample_rate);
        assert!(result.beat_count >= 8, "expected ~10 beats, got {}", result.beat_count);
        let hr = result.mean_hr_bpm.expect("should have HR");
        assert!(
            (50.0..=70.0).contains(&hr),
            "expected ~60 BPM, got {hr:.1}"
        );
        // Sine wave has constant RR → RMSSD should be very low.
        if let Some(rmssd) = result.rmssd_ms {
            assert!(rmssd < 80.0, "expected low RMSSD for constant-rate sine, got {rmssd:.1}");
        }
    }

    #[test]
    fn test_variable_rate_ppg() {
        // Simulate varying HR: alternating 60 and 75 BPM (RR = 1000 and 800 ms).
        let sample_rate = 25.0;
        let mut samples = Vec::new();
        let rr_pattern = [1.0, 0.8, 1.0, 0.8, 1.0, 0.8, 1.0, 0.8]; // seconds
        let mut t = 0.0;
        for &rr in &rr_pattern {
            let n = (rr * sample_rate) as usize;
            for i in 0..n {
                let phase = i as f64 / n as f64;
                let v = 200_000.0 + 5_000.0 * (2.0 * std::f64::consts::PI * phase).sin();
                samples.push(v as i32);
            }
            t += rr;
        }
        let _ = t;

        let result = extract_hr_from_ppg(&samples, sample_rate);
        assert!(result.beat_count >= 6, "expected ~8 beats, got {}", result.beat_count);
        let hr = result.mean_hr_bpm.expect("should have HR");
        assert!(
            (55.0..=85.0).contains(&hr),
            "expected ~67 BPM avg, got {hr:.1}"
        );
        // RMSSD should be non-trivial with varying RR.
        let rmssd = result.rmssd_ms.expect("should have RMSSD");
        assert!(rmssd > 10.0, "expected measurable RMSSD, got {rmssd:.1}");
    }

    #[test]
    fn dicrotic_notch_does_not_double_resting_heart_rate() {
        // Resting pulse at 54 BPM with a prominent dicrotic (secondary) peak
        // per cycle: the classic sleep-time waveform. Naive peak spacing counts
        // the dicrotic wave as a beat and reports ~108 BPM with clean,
        // consistent intervals that interval filters cannot reject; the
        // autocorrelation fundamental must pin the true rate.
        let sample_rate = 25.0;
        let beat_hz = 0.9; // 54 BPM
        let total_seconds = 60.0;
        let n = (total_seconds * sample_rate) as usize;
        let mut samples = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f64 / sample_rate;
            let phase = 2.0 * std::f64::consts::PI * beat_hz * t;
            // Systolic peak + strong dicrotic wave (second harmonic, offset
            // phase) riding on a DC level, like AC-coupled reflectance PPG.
            let v = 200_000.0
                + 4_000.0 * phase.sin()
                + 2_600.0 * (2.0 * phase + 0.9).sin();
            samples.push(v as i32);
        }

        let result = extract_hr_from_ppg(&samples, sample_rate);
        let hr = result.mean_hr_bpm.expect("should have HR");
        assert!(
            (44.0..=64.0).contains(&hr),
            "expected ~54 BPM fundamental, got {hr:.1} (harmonic doubling?)"
        );
    }

    #[test]
    fn test_insufficient_samples() {
        let result = extract_hr_from_ppg(&[100, 200, 300], 25.0);
        assert_eq!(result.beat_count, 0);
        assert!(result.mean_hr_bpm.is_none());
        assert!(result.quality_flags.contains(&"insufficient_samples".to_string()));
    }
}
