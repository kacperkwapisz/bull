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

    // 2. Simple moving-average bandpass: subtract a long-window mean (high-pass)
    //    then smooth with a short window (low-pass). This is crude but robust.
    //    High-pass cutoff ~0.5 Hz: window = sample_rate / 0.5 = 2*sr samples.
    //    Low-pass cutoff ~4 Hz: window = sample_rate / 4 / 2 ≈ sr/8 samples.
    let hp_window = (sample_rate_hz * 2.0).max(3.0) as usize;
    let lp_window = (sample_rate_hz / 8.0).max(1.0) as usize;

    let filtered = bandpass_ma(&ac, hp_window, lp_window);

    // 3. Peak detection: find local maxima above a dynamic threshold.
    //    Minimum peak spacing = sample_rate / 4 Hz (max 240 BPM).
    let min_spacing = (sample_rate_hz / 4.0).max(2.0) as usize;
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

    // Filter out physiologically implausible RR intervals (< 250ms = 240 BPM,
    // > 2000ms = 30 BPM).
    let valid_rr: Vec<f64> = rr_intervals_ms
        .iter()
        .copied()
        .filter(|rr| (250.0..=2000.0).contains(rr))
        .collect();

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

    // RMSSD: root mean square of successive differences.
    let rmssd_ms = if valid_rr.len() >= 2 {
        let sum_sq_diff: f64 = valid_rr
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).powi(2))
            .sum();
        Some((sum_sq_diff / (valid_rr.len() - 1) as f64).sqrt())
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

/// Moving-average bandpass: high-pass by subtracting a long MA, then low-pass
/// with a short MA.
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
            assert!(rmssd < 50.0, "expected low RMSSD for constant-rate sine, got {rmssd:.1}");
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
    fn test_insufficient_samples() {
        let result = extract_hr_from_ppg(&[100, 200, 300], 25.0);
        assert_eq!(result.beat_count, 0);
        assert!(result.mean_hr_bpm.is_none());
        assert!(result.quality_flags.contains(&"insufficient_samples".to_string()));
    }
}
