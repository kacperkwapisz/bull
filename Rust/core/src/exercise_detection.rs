use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::energy_rollup::keytel_active_kcal_per_min;
use crate::metrics::{StrainInput, bull_strain_v1, resolve_effective_hrmax};
use crate::store::GravityRow;

// ── Algorithm constants (matching exercise.py exactly) ─────────────────────

// Must match the source string returned by resolve_effective_hrmax in metrics.rs.
const HRMAX_SOURCE_FALLBACK: &str = "fallback";

pub const MIN_EXERCISE_MIN: f64 = 10.0;
pub const MERGE_GAP_S: f64 = 60.0;
pub const HR_MARGIN_BPM: f64 = 30.0;
pub const MOTION_THRESHOLD: f64 = 0.20; // g/sample — 0.01 sat below MEMS gravity-sensor quantisation noise
pub const MOTION_SMOOTH_S: f64 = 3.0;
pub const ALIGN_TOLERANCE_S: f64 = 5.0;
pub const MIN_INTENSITY_Z2PLUS: f64 = 0.50;

// ── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HrSample {
    pub ts: f64,
    pub bpm: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExerciseProfile {
    pub resting_hr: Option<f64>,
    pub max_hr: Option<f64>,
    pub age: Option<u8>,
    pub sex: Option<String>,
    pub weight_kg: Option<f64>,
    pub height_cm: Option<f64>,
    pub daily_hr_p10: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExerciseSession {
    pub device_id: String,
    pub start_ts: f64,
    pub end_ts: f64,
    pub duration_s: f64,
    pub avg_hr: f64,
    pub peak_hr: f64,
    pub strain: f64,
    pub calories_kcal: f64,
    pub zone_time_pct: BTreeMap<u8, f64>,
    pub hrmax: f64,
    pub hrmax_source: String,
    pub rhr_source: String,
    pub avg_hrr_pct: f64,
}

// ── Internal aligned pair ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct AlignedPair {
    ts: f64,
    bpm: u8,
    smoothed_mag: f64,
}

// ── Main function ───────────────────────────────────────────────────────────

pub fn detect_exercise_sessions(
    hr: &[HrSample],
    gravity: &[GravityRow],
    profile: &ExerciseProfile,
) -> Vec<ExerciseSession> {
    // Step 1 — Resolve RHR
    let (rhr, rhr_source) = if let Some(rhr) = profile.resting_hr {
        (rhr, "profile_override".to_string())
    } else if let Some(p10) = profile.daily_hr_p10 {
        (p10, "daily_p10".to_string())
    } else {
        return Vec::new();
    };

    // Step 2 — Resolve HRmax
    let age_f64 = profile.age.map(|a| a as f64);
    let fallback_hrmax = profile
        .max_hr
        .unwrap_or_else(|| 220.0 - profile.age.unwrap_or(30) as f64);
    let (hrmax, hrmax_source) = resolve_effective_hrmax(fallback_hrmax, age_f64, &[]);
    if hrmax.is_nan() || hrmax <= rhr {
        return Vec::new();
    }

    // Step 3 — Smooth gravity: O(n) causal rolling mean over a [ts-MOTION_SMOOTH_S, ts] window.
    // Sort by ts, then use a two-pointer sliding window to avoid the O(n²) inner scan.
    let mut sorted_gravity = gravity.to_vec();
    sorted_gravity.sort_by(|a, b| a.ts.partial_cmp(&b.ts).unwrap_or(std::cmp::Ordering::Equal));
    let mags: Vec<f64> = sorted_gravity
        .iter()
        .map(|g| (g.x * g.x + g.y * g.y + g.z * g.z).sqrt() - 1.0)
        .collect();
    let mut smoothed_gravity: Vec<(f64, f64)> = Vec::with_capacity(sorted_gravity.len());
    let mut left = 0usize;
    let mut window_sum = 0.0f64;
    for right in 0..sorted_gravity.len() {
        window_sum += mags[right];
        while sorted_gravity[right].ts - sorted_gravity[left].ts > MOTION_SMOOTH_S {
            window_sum -= mags[left];
            left += 1;
        }
        let window_len = right - left + 1;
        smoothed_gravity.push((
            sorted_gravity[right].ts,
            (window_sum / window_len as f64).abs(),
        ));
    }

    // Step 4 — Align HR and smoothed gravity via nearest-neighbor within ALIGN_TOLERANCE_S
    let mut aligned: Vec<AlignedPair> = Vec::new();
    for sample in hr {
        // Find gravity row with minimum |ts_gravity - ts_hr| <= ALIGN_TOLERANCE_S
        let best = smoothed_gravity
            .iter()
            .filter(|(ts_g, _)| (ts_g - sample.ts).abs() <= ALIGN_TOLERANCE_S)
            .min_by(|(a, _), (b, _)| {
                let da = (a - sample.ts).abs();
                let db = (b - sample.ts).abs();
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            });
        if let Some((_, smoothed_mag)) = best {
            aligned.push(AlignedPair {
                ts: sample.ts,
                bpm: sample.bpm,
                smoothed_mag: *smoothed_mag,
            });
        }
    }

    // Step 5 — Apply dual-gate: keep pairs where bpm > rhr + HR_MARGIN_BPM AND smoothed_mag > MOTION_THRESHOLD
    let active: Vec<&AlignedPair> = aligned
        .iter()
        .filter(|p| p.bpm as f64 > rhr + HR_MARGIN_BPM && p.smoothed_mag > MOTION_THRESHOLD)
        .collect();

    if active.is_empty() {
        return Vec::new();
    }

    // Group into segments: consecutive active pairs where gap <= MERGE_GAP_S
    let mut segments: Vec<Vec<AlignedPair>> = Vec::new();
    let mut current_segment: Vec<AlignedPair> = vec![(*active[0]).clone()];
    for i in 1..active.len() {
        let gap = active[i].ts - active[i - 1].ts;
        if gap <= MERGE_GAP_S {
            current_segment.push((*active[i]).clone());
        } else {
            segments.push(current_segment);
            current_segment = vec![(*active[i]).clone()];
        }
    }
    segments.push(current_segment);

    // Step 6 — Merge adjacent segments with gap < MERGE_GAP_S
    // (Already handled in grouping above, but also merge between segment ends/starts)
    let merged = merge_segments(segments);

    // Step 7 — Filter by minimum duration
    let min_duration_s = MIN_EXERCISE_MIN * 60.0;
    let duration_filtered: Vec<Vec<AlignedPair>> = merged
        .into_iter()
        .filter(|seg| {
            let start = seg.first().map(|p| p.ts).unwrap_or(0.0);
            let end = seg.last().map(|p| p.ts).unwrap_or(0.0);
            (end - start) >= min_duration_s
        })
        .collect();

    // Step 8 + Step 9 — Intensity gate + compute per-session ExerciseSession
    let mut sessions: Vec<ExerciseSession> = Vec::new();
    let skip_intensity_gate = hrmax_source == HRMAX_SOURCE_FALLBACK && profile.age.is_none();

    for seg in duration_filtered {
        if seg.is_empty() {
            continue;
        }

        let start_ts = seg.first().map(|p| p.ts).unwrap_or(0.0);
        let end_ts = seg.last().map(|p| p.ts).unwrap_or(0.0);

        // Compute per-sample HRR% and zones
        let hrr_range = hrmax - rhr;
        let sample_hrr_pcts: Vec<f64> = seg
            .iter()
            .map(|p| ((p.bpm as f64 - rhr) / hrr_range * 100.0).clamp(0.0, 100.0))
            .collect();

        // Zone assignment: Edwards 5-zone based on HRR%
        // zone 1: < 50, zone 2: 50-60, zone 3: 60-70, zone 4: 70-80, zone 5: >= 80
        let zone_of = |hrr_pct: f64| -> u8 {
            if hrr_pct < 50.0 {
                1
            } else if hrr_pct < 60.0 {
                2
            } else if hrr_pct < 70.0 {
                3
            } else if hrr_pct < 80.0 {
                4
            } else {
                5
            }
        };

        let n = seg.len() as f64;

        // Step 8 — Intensity gate
        if !skip_intensity_gate {
            let z2plus_count = sample_hrr_pcts
                .iter()
                .filter(|&&pct| zone_of(pct) >= 2)
                .count();
            let fraction_z2plus = z2plus_count as f64 / n;
            if fraction_z2plus < MIN_INTENSITY_Z2PLUS {
                continue;
            }
        }

        // Step 9 — Compute per-session metrics
        let duration_s = end_ts - start_ts;
        let avg_hr = seg.iter().map(|p| p.bpm as f64).sum::<f64>() / n;
        let peak_hr = seg
            .iter()
            .map(|p| p.bpm as f64)
            .fold(f64::NEG_INFINITY, f64::max);
        let avg_hrr_pct = sample_hrr_pcts.iter().sum::<f64>() / n;

        // Zone time counts
        let mut zone_counts = [0usize; 5]; // index 0 = zone 1, index 4 = zone 5
        for &pct in &sample_hrr_pcts {
            let z = zone_of(pct) as usize;
            zone_counts[z - 1] += 1;
        }

        // zone_time_pct: zones 1-4 computed normally, zone 5 = 100 - sum(1-4) to absorb FP drift
        let mut zone_time_pct: BTreeMap<u8, f64> = BTreeMap::new();
        let total = seg.len();
        let mut sum_pct_1_to_4 = 0.0f64;
        for z in 1u8..=4 {
            let pct = 100.0 * zone_counts[z as usize - 1] as f64 / total as f64;
            zone_time_pct.insert(z, pct);
            sum_pct_1_to_4 += pct;
        }
        zone_time_pct.insert(5, (100.0 - sum_pct_1_to_4).max(0.0));

        // Zone minutes for strain computation (5 zones)
        let duration_min = duration_s / 60.0;
        let hr_zone_minutes: Vec<f64> = (0..5)
            .map(|i| zone_counts[i] as f64 / total as f64 * duration_min)
            .collect();

        // Strain via bull_strain_v1
        let strain_input = StrainInput {
            start_time: format!("{}", start_ts as i64),
            end_time: format!("{}", end_ts as i64),
            duration_minutes: duration_min,
            resting_hr_bpm: rhr,
            average_hr_bpm: avg_hr,
            max_hr_bpm: hrmax,
            hr_zone_minutes,
            input_ids: Vec::new(),
            profile_sex: profile.sex.clone(),
            profile_age: age_f64,
        };
        let strain_result = bull_strain_v1(&strain_input);
        let strain = strain_result
            .output
            .as_ref()
            .map(|o| o.score_0_to_21)
            .unwrap_or(0.0);

        // Calories: Keytel active EE for samples where hrr_pct >= 30
        // For samples < 30% HRR, use simplified resting estimate
        let weight_kg = profile.weight_kg.unwrap_or(70.0);
        let age_val = age_f64.unwrap_or(30.0);
        let sex = profile.sex.as_deref();

        let active_count = sample_hrr_pcts.iter().filter(|&&pct| pct >= 30.0).count();
        let resting_count = seg.len() - active_count;

        // Active EE: Keytel per-minute rate × active minutes
        // Assume 1 sample per second → each sample = 1/60 min
        let sample_duration_min = 1.0 / 60.0;
        let active_kcal = if active_count > 0 {
            let kcal_per_min = keytel_active_kcal_per_min(avg_hr, weight_kg, age_val, sex, hrmax);
            kcal_per_min * (active_count as f64 * sample_duration_min)
        } else {
            0.0
        };

        // Resting EE: weight-scaled RMR (22 kcal/kg/day = 22/1440 kcal/kg/min)
        let resting_kcal = weight_kg * 22.0 / 1440.0 * (resting_count as f64 * sample_duration_min);

        let calories_kcal = (active_kcal + resting_kcal).max(0.0);

        sessions.push(ExerciseSession {
            device_id: String::new(),
            start_ts,
            end_ts,
            duration_s,
            avg_hr,
            peak_hr,
            strain,
            calories_kcal,
            zone_time_pct,
            hrmax,
            hrmax_source: hrmax_source.clone(),
            rhr_source: rhr_source.clone(),
            avg_hrr_pct,
        });
    }

    sessions
}

// ── Merge helper ────────────────────────────────────────────────────────────

fn merge_segments(mut segments: Vec<Vec<AlignedPair>>) -> Vec<Vec<AlignedPair>> {
    loop {
        let mut merged = false;
        let mut result: Vec<Vec<AlignedPair>> = Vec::new();
        let mut i = 0;
        while i < segments.len() {
            if i + 1 < segments.len() {
                let end_ts = segments[i].last().map(|p| p.ts).unwrap_or(0.0);
                let start_next = segments[i + 1].first().map(|p| p.ts).unwrap_or(f64::MAX);
                if start_next - end_ts < MERGE_GAP_S {
                    // Merge segments[i] and segments[i+1]
                    let mut combined = segments[i].clone();
                    combined.extend(segments[i + 1].clone());
                    result.push(combined);
                    i += 2;
                    merged = true;
                    continue;
                }
            }
            result.push(segments[i].clone());
            i += 1;
        }
        segments = result;
        if !merged {
            break;
        }
    }
    segments
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_gravity(ts: f64, mag: f64) -> GravityRow {
        // mag = sqrt(x^2+y^2+z^2) - 1.0, so we set z = mag + 1.0 and x=y=0
        GravityRow {
            device_id: "test".to_string(),
            ts,
            x: 0.0,
            y: 0.0,
            z: mag + 1.0,
        }
    }

    fn default_profile() -> ExerciseProfile {
        ExerciseProfile {
            resting_hr: Some(55.0),
            max_hr: Some(185.0),
            age: Some(30),
            sex: Some("male".to_string()),
            weight_kg: Some(75.0),
            height_cm: Some(175.0),
            daily_hr_p10: None,
        }
    }

    #[test]
    fn test_alignment_within_tolerance() {
        // HR at ts=100, gravity at ts=103 → matched (3s < 5s tolerance)
        let hr = vec![HrSample {
            ts: 100.0,
            bpm: 100,
        }];
        let gravity = vec![make_gravity(103.0, 0.1)];
        let profile = default_profile();
        let _result = detect_exercise_sessions(&hr, &gravity, &profile);
        // Alignment matched: hr[0] gets gravity[0]
        // Single pair is not a session (too short), but alignment itself is tested via
        // a longer sequence below. For this unit test, create a sequence that passes gates.
        // We only test whether the alignment happens at all (via a helper):
        let aligned = align_for_test(&hr, &gravity);
        assert_eq!(
            aligned.len(),
            1,
            "HR at ts=100 should match gravity at ts=103 (3s < 5s)"
        );

        // HR at ts=100, gravity at ts=106 → not matched (6s > 5s tolerance)
        let gravity_far = vec![make_gravity(106.0, 0.1)];
        let aligned2 = align_for_test(&hr, &gravity_far);
        assert_eq!(
            aligned2.len(),
            0,
            "HR at ts=100 should NOT match gravity at ts=106 (6s > 5s)"
        );
    }

    // Helper to test alignment logic in isolation
    fn align_for_test(hr: &[HrSample], gravity: &[GravityRow]) -> Vec<AlignedPair> {
        let half_window = MOTION_SMOOTH_S / 2.0;
        let smoothed_gravity: Vec<(f64, f64)> = gravity
            .iter()
            .map(|g| {
                let window_mags: Vec<f64> = gravity
                    .iter()
                    .filter(|other| (other.ts - g.ts).abs() <= half_window)
                    .map(|other| {
                        (other.x * other.x + other.y * other.y + other.z * other.z).sqrt() - 1.0
                    })
                    .collect();
                let mean_mag = if window_mags.is_empty() {
                    (g.x * g.x + g.y * g.y + g.z * g.z).sqrt() - 1.0
                } else {
                    window_mags.iter().sum::<f64>() / window_mags.len() as f64
                };
                (g.ts, mean_mag.abs())
            })
            .collect();

        let mut aligned = Vec::new();
        for sample in hr {
            let best = smoothed_gravity
                .iter()
                .filter(|(ts_g, _)| (ts_g - sample.ts).abs() <= ALIGN_TOLERANCE_S)
                .min_by(|(a, _), (b, _)| {
                    let da = (a - sample.ts).abs();
                    let db = (b - sample.ts).abs();
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                });
            if let Some((_, smoothed_mag)) = best {
                aligned.push(AlignedPair {
                    ts: sample.ts,
                    bpm: sample.bpm,
                    smoothed_mag: *smoothed_mag,
                });
            }
        }
        aligned
    }

    fn make_session(duration_min: f64, fraction_z2plus: f64) -> Vec<ExerciseSession> {
        // Build a synthetic dataset that produces the desired session.
        // profile.max_hr=190 and age=30 → tanaka(30)=187 < 190 → effective hrmax=190
        // hrrange = 190 - 55 = 135; zone 2 starts at 50% HRR = 55 + 67.5 = 122.5
        let profile = ExerciseProfile {
            resting_hr: Some(55.0),
            max_hr: Some(190.0), // > tanaka(30)=187, so effective hrmax=190
            age: Some(30),
            sex: Some("male".to_string()),
            weight_kg: Some(75.0),
            height_cm: Some(175.0),
            daily_hr_p10: None,
        };
        let rhr = 55.0f64;
        // effective hrmax = max(profile.max_hr=190, tanaka(30)=187) = 190
        let effective_hrmax = 190.0f64;
        let n_samples = (duration_min * 60.0) as usize;

        // Zone 2 starts at 50% HRR with effective hrmax:
        // bpm = rhr + 0.55*(hrmax-rhr) = 55 + 0.55*135 = 55 + 74.25 = 129.25 → 129
        // Use 55% HRR for margin to ensure zone 2 classification
        let z2_bpm = (rhr + 0.55 * (effective_hrmax - rhr)) as u8; // 129
        // Zone 1 bpm: above gate (rhr + HR_MARGIN_BPM = 85), below zone 2 threshold (~122.5)
        let z1_bpm = (rhr + HR_MARGIN_BPM + 5.0) as u8; // 90, clearly zone 1

        let n_z2plus = (fraction_z2plus * n_samples as f64) as usize;
        let n_z1 = n_samples - n_z2plus;

        let mut hr = Vec::new();
        for i in 0..n_z2plus {
            hr.push(HrSample {
                ts: i as f64,
                bpm: z2_bpm,
            });
        }
        for i in 0..n_z1 {
            hr.push(HrSample {
                ts: (n_z2plus + i) as f64,
                bpm: z1_bpm,
            });
        }
        // Sort by ts
        hr.sort_by(|a, b| a.ts.partial_cmp(&b.ts).unwrap());

        // Gravity: all samples active (> threshold)
        let gravity: Vec<GravityRow> = hr.iter().map(|s| make_gravity(s.ts, 0.30)).collect();

        detect_exercise_sessions(&hr, &gravity, &profile)
    }

    #[test]
    fn test_merge_gap_bridging() {
        // Two active segments with 45s gap → merged
        // Two active segments with 65s gap → kept separate
        let profile = default_profile();
        let rhr = 55.0f64;
        let hrmax = 185.0f64;
        // Use bpm above zone 2 threshold to pass intensity gate
        // effective hrmax = max(185, tanaka(30)=187) = 187, hrrange=132; zone2 >= 121
        let z2_bpm = (rhr + 0.50 * (hrmax - rhr) + 2.0) as u8; // 122, safely zone 2

        // Build segment A: 0..900 (15 min), gap of 45s, segment B: 945..1845 (15 min)
        let mut hr_45gap = Vec::new();
        // Segment A: ts 0..=899
        for ts in 0i64..=899 {
            hr_45gap.push(HrSample {
                ts: ts as f64,
                bpm: z2_bpm,
            });
        }
        // Gap: 900..944 (45 seconds)
        // Segment B: ts 945..=1844
        for ts in 945i64..=1844 {
            hr_45gap.push(HrSample {
                ts: ts as f64,
                bpm: z2_bpm,
            });
        }
        let gravity_45: Vec<GravityRow> =
            hr_45gap.iter().map(|s| make_gravity(s.ts, 0.30)).collect();
        let sessions_45 = detect_exercise_sessions(&hr_45gap, &gravity_45, &profile);
        assert_eq!(sessions_45.len(), 1, "45s gap should merge into 1 session");

        // Build segment A: 0..=899, gap of 65s, segment B: 965..=1864
        let mut hr_65gap = Vec::new();
        for ts in 0i64..=899 {
            hr_65gap.push(HrSample {
                ts: ts as f64,
                bpm: z2_bpm,
            });
        }
        // Gap: 900..964 (65 seconds, >= MERGE_GAP_S=60)
        for ts in 965i64..=1864 {
            hr_65gap.push(HrSample {
                ts: ts as f64,
                bpm: z2_bpm,
            });
        }
        let gravity_65: Vec<GravityRow> =
            hr_65gap.iter().map(|s| make_gravity(s.ts, 0.30)).collect();
        let sessions_65 = detect_exercise_sessions(&hr_65gap, &gravity_65, &profile);
        assert_eq!(
            sessions_65.len(),
            2,
            "65s gap should produce 2 separate sessions"
        );
    }

    #[test]
    fn test_minimum_duration_filter() {
        // Session of 8 min rejected
        let sessions_8min = make_session(8.0, 0.60);
        assert_eq!(
            sessions_8min.len(),
            0,
            "8 min session should be rejected (< 10 min)"
        );

        // Session of 11 min kept
        let sessions_11min = make_session(11.0, 0.60);
        assert_eq!(
            sessions_11min.len(),
            1,
            "11 min session should be kept (>= 10 min)"
        );
    }

    #[test]
    fn test_intensity_gate_discard() {
        // Session with 30% Z2+ time discarded (< 50%)
        let sessions_30pct = make_session(12.0, 0.30);
        assert_eq!(
            sessions_30pct.len(),
            0,
            "Session with 30% Z2+ should be discarded"
        );

        // Session with 55% Z2+ time kept
        let sessions_55pct = make_session(12.0, 0.55);
        assert_eq!(
            sessions_55pct.len(),
            1,
            "Session with 55% Z2+ should be kept"
        );
    }

    #[test]
    fn test_zone_time_pct_sums_to_100() {
        let sessions = make_session(12.0, 0.60);
        assert!(!sessions.is_empty(), "Expected at least one session");
        for session in &sessions {
            let sum: f64 = session.zone_time_pct.values().sum();
            assert!(
                (sum - 100.0).abs() < 0.01,
                "zone_time_pct should sum to 100, got {sum}"
            );
        }
    }

    #[test]
    fn test_calories_positive() {
        // 30-min session at ~70% avg HRR with weight_kg=75, age=30, sex="male" → calories_kcal > 0
        let profile = ExerciseProfile {
            resting_hr: Some(55.0),
            max_hr: Some(185.0),
            age: Some(30),
            sex: Some("male".to_string()),
            weight_kg: Some(75.0),
            height_cm: Some(175.0),
            daily_hr_p10: None,
        };
        // 70% HRR: bpm = rhr + 0.70*(hrmax-rhr) = 55 + 0.70*130 = 146
        let rhr = 55.0f64;
        let hrmax = 185.0f64;
        let target_bpm = (rhr + 0.70 * (hrmax - rhr)) as u8; // 146
        let n_samples = 30 * 60;
        let hr: Vec<HrSample> = (0..n_samples)
            .map(|i| HrSample {
                ts: i as f64,
                bpm: target_bpm,
            })
            .collect();
        let gravity: Vec<GravityRow> = hr.iter().map(|s| make_gravity(s.ts, 0.30)).collect();
        let sessions = detect_exercise_sessions(&hr, &gravity, &profile);
        assert!(
            !sessions.is_empty(),
            "Expected at least one session for 30-min workout"
        );
        assert!(
            sessions[0].calories_kcal > 0.0,
            "calories_kcal should be positive, got {}",
            sessions[0].calories_kcal
        );
    }

    #[test]
    fn test_rhr_fallback_daily_p10() {
        let profile = ExerciseProfile {
            resting_hr: None,
            max_hr: Some(185.0),
            age: Some(30),
            sex: Some("male".to_string()),
            weight_kg: Some(75.0),
            height_cm: Some(175.0),
            daily_hr_p10: Some(50.0),
        };
        // Use bpm well above rhr(50)+30=80: use 120
        let rhr = 50.0f64;
        let hrmax = 185.0f64;
        let z2_bpm = (rhr + 0.55 * (hrmax - rhr)) as u8; // ~125
        let n_samples = 12 * 60;
        let hr: Vec<HrSample> = (0..n_samples)
            .map(|i| HrSample {
                ts: i as f64,
                bpm: z2_bpm,
            })
            .collect();
        let gravity: Vec<GravityRow> = hr.iter().map(|s| make_gravity(s.ts, 0.30)).collect();
        let sessions = detect_exercise_sessions(&hr, &gravity, &profile);
        assert!(
            !sessions.is_empty(),
            "Expected a session when daily_hr_p10 is used as RHR"
        );
        assert_eq!(
            sessions[0].rhr_source, "daily_p10",
            "rhr_source should be 'daily_p10'"
        );
    }

    #[test]
    fn test_no_rhr_no_sessions() {
        let profile = ExerciseProfile {
            resting_hr: None,
            max_hr: Some(185.0),
            age: Some(30),
            sex: Some("male".to_string()),
            weight_kg: Some(75.0),
            height_cm: Some(175.0),
            daily_hr_p10: None,
        };
        let hr = vec![HrSample { ts: 0.0, bpm: 150 }];
        let gravity = vec![make_gravity(0.0, 0.05)];
        let sessions = detect_exercise_sessions(&hr, &gravity, &profile);
        assert_eq!(
            sessions.len(),
            0,
            "No RHR available → should return empty Vec"
        );
    }
}
