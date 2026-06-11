use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    BullError, BullResult,
    metrics::{
        AlgorithmRunResult, HrvInput, MetricComponent, SleepInput, StrainInput, StressInput,
    },
    store::{AlgorithmDefinitionRecord, AlgorithmRunRecord},
};

pub const REFERENCE_HRV_TIME_DOMAIN_ID: &str = "reference.hrv.time_domain.v1";
pub const REFERENCE_HRV_TIME_DOMAIN_VERSION: &str = "1.0.0";
pub const REFERENCE_HRV_PROVIDER: &str = "internal.hand_derived_time_domain";
pub const REFERENCE_SLEEP_ACTIGRAPHY_ID: &str = "reference.sleep.actigraphy_summary.v1";
pub const REFERENCE_SLEEP_ACTIGRAPHY_VERSION: &str = "1.0.0";
pub const REFERENCE_SLEEP_ACTIGRAPHY_PROVIDER: &str = "internal.pyactigraphy_style_window_summary";
pub const REFERENCE_STRAIN_EDWARDS_ID: &str = "reference.strain.edwards_zone_load.v1";
pub const REFERENCE_STRAIN_EDWARDS_VERSION: &str = "1.0.0";
pub const REFERENCE_STRAIN_EDWARDS_PROVIDER: &str = "internal.edwards_zone_load";
pub const REFERENCE_STRESS_HRV_HR_ID: &str = "reference.stress.hrv_hr_proxy.v1";
pub const REFERENCE_STRESS_HRV_HR_VERSION: &str = "1.0.0";
pub const REFERENCE_STRESS_HRV_HR_PROVIDER: &str = "internal.hrv_hr_stress_proxy";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HrvReferenceOutput {
    pub provider: String,
    pub provider_version: String,
    pub interval_count: usize,
    pub valid_interval_count: usize,
    pub invalid_interval_count: usize,
    pub mean_nn_ms: f64,
    pub rmssd_ms: f64,
    pub sdnn_sample_ms: f64,
    pub pnn50_fraction: f64,
    pub components: Vec<MetricComponent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SleepActigraphyReferenceOutput {
    pub provider: String,
    pub provider_version: String,
    pub time_in_bed_minutes: f64,
    pub sleep_minutes: f64,
    pub wake_minutes: f64,
    pub sleep_efficiency_fraction: f64,
    pub wake_after_sleep_onset_minutes: f64,
    pub disturbance_count: u32,
    pub fragmentation_index_per_hour: f64,
    pub components: Vec<MetricComponent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StrainEdwardsReferenceOutput {
    pub provider: String,
    pub provider_version: String,
    pub duration_minutes: f64,
    pub zone_minutes: Vec<f64>,
    pub zone_weights: Vec<f64>,
    pub edwards_load: f64,
    pub edwards_load_per_hour: f64,
    pub components: Vec<MetricComponent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StressHrvHrReferenceOutput {
    pub provider: String,
    pub provider_version: String,
    pub heart_rate_elevation_score: f64,
    pub hrv_suppression_score: f64,
    pub unadjusted_stress_score_0_to_100: f64,
    pub components: Vec<MetricComponent>,
}

pub fn reference_algorithm_definitions() -> Vec<AlgorithmDefinitionRecord> {
    vec![
        AlgorithmDefinitionRecord {
            algorithm_id: REFERENCE_HRV_TIME_DOMAIN_ID.to_string(),
            version: REFERENCE_HRV_TIME_DOMAIN_VERSION.to_string(),
            metric_family: "hrv".to_string(),
            display_name: "Reference HRV Time Domain".to_string(),
            implementation: "rust-reference".to_string(),
            license: "UNLICENSED".to_string(),
            input_schema: "bull.hrv-input.v1".to_string(),
            output_schema: "bull.hrv-reference-output.v1".to_string(),
            input_requirements_json: json!({
                "rr_intervals_ms": {
                    "unit": "ms",
                    "valid_range_inclusive": [300.0, 2000.0],
                    "minimum_to_compute": 2
                }
            })
            .to_string(),
            params_json: json!({
                "provider": REFERENCE_HRV_PROVIDER,
                "sdnn": "sample_standard_deviation",
                "pnn50_threshold_ms": 50.0,
                "invalid_rr_policy": "drop_and_flag"
            })
            .to_string(),
            quality_gates_json: json!([
                "at_least_2_valid_rr_intervals_to_compute",
                "drop_rr_intervals_outside_300_to_2000_ms"
            ])
            .to_string(),
            status: "benchmark-only".to_string(),
        },
        AlgorithmDefinitionRecord {
            algorithm_id: REFERENCE_SLEEP_ACTIGRAPHY_ID.to_string(),
            version: REFERENCE_SLEEP_ACTIGRAPHY_VERSION.to_string(),
            metric_family: "sleep".to_string(),
            display_name: "Reference Sleep Actigraphy Summary".to_string(),
            implementation: "rust-reference".to_string(),
            license: "UNLICENSED".to_string(),
            input_schema: "bull.sleep-input.v1".to_string(),
            output_schema: "bull.sleep-actigraphy-reference-output.v1".to_string(),
            input_requirements_json: json!({
                "time_in_bed_minutes": {"unit": "minutes", "minimum_to_compute": 1.0},
                "sleep_duration_minutes": {"unit": "minutes", "minimum_to_compute": 1.0},
                "disturbance_count": {"unit": "count"}
            })
            .to_string(),
            params_json: json!({
                "provider": REFERENCE_SLEEP_ACTIGRAPHY_PROVIDER,
                "outputs": [
                    "sleep_efficiency_fraction",
                    "wake_after_sleep_onset_minutes",
                    "fragmentation_index_per_hour"
                ],
                "intended_external_benchmarks": ["pyActigraphy", "GGIR"]
            })
            .to_string(),
            quality_gates_json: json!([
                "positive_time_in_bed",
                "positive_sleep_duration",
                "sleep_duration_not_greater_than_time_in_bed"
            ])
            .to_string(),
            status: "benchmark-only".to_string(),
        },
        AlgorithmDefinitionRecord {
            algorithm_id: REFERENCE_STRAIN_EDWARDS_ID.to_string(),
            version: REFERENCE_STRAIN_EDWARDS_VERSION.to_string(),
            metric_family: "strain".to_string(),
            display_name: "Reference Edwards Zone Load".to_string(),
            implementation: "rust-reference".to_string(),
            license: "UNLICENSED".to_string(),
            input_schema: "bull.strain-input.v1".to_string(),
            output_schema: "bull.edwards-zone-load-reference-output.v1".to_string(),
            input_requirements_json: json!({
                "hr_zone_minutes": {"unit": "minutes", "required_count": 5},
                "duration_minutes": {"unit": "minutes", "minimum_to_compute": 1.0}
            })
            .to_string(),
            params_json: json!({
                "provider": REFERENCE_STRAIN_EDWARDS_PROVIDER,
                "zone_weights": [1.0, 2.0, 3.0, 4.0, 5.0],
                "formula": "sum(zone_minutes[i] * zone_weight[i])"
            })
            .to_string(),
            quality_gates_json: json!([
                "five_hr_zones_required",
                "positive_duration",
                "zone_minutes_finite_non_negative",
                "zone_minutes_sum_close_to_duration"
            ])
            .to_string(),
            status: "benchmark-only".to_string(),
        },
        AlgorithmDefinitionRecord {
            algorithm_id: REFERENCE_STRESS_HRV_HR_ID.to_string(),
            version: REFERENCE_STRESS_HRV_HR_VERSION.to_string(),
            metric_family: "stress".to_string(),
            display_name: "Reference HRV/HR Stress Proxy".to_string(),
            implementation: "rust-reference".to_string(),
            license: "UNLICENSED".to_string(),
            input_schema: "bull.stress-input.v1".to_string(),
            output_schema: "bull.stress-hrv-hr-reference-output.v1".to_string(),
            input_requirements_json: json!({
                "heart_rate_bpm": {"unit": "bpm", "minimum_to_compute": 1.0},
                "resting_hr_bpm": {"unit": "bpm", "minimum_to_compute": 1.0},
                "hrv_rmssd_ms": {"unit": "ms_rmssd", "minimum_to_compute": 0.0},
                "hrv_baseline_rmssd_ms": {"unit": "ms_rmssd", "minimum_to_compute": 1.0}
            })
            .to_string(),
            params_json: json!({
                "provider": REFERENCE_STRESS_HRV_HR_PROVIDER,
                "formula": "0.6 * heart_rate_elevation_score + 0.4 * hrv_suppression_score",
                "motion_policy": "ignored_for_reference_proxy"
            })
            .to_string(),
            quality_gates_json: json!([
                "positive_heart_rate",
                "positive_resting_heart_rate",
                "non_negative_rmssd",
                "positive_baseline_rmssd",
                "motion_context_not_part_of_reference_proxy"
            ])
            .to_string(),
            status: "benchmark-only".to_string(),
        },
    ]
}

pub fn reference_hrv_time_domain(input: &HrvInput) -> AlgorithmRunResult<HrvReferenceOutput> {
    let mut quality_flags = Vec::new();
    let mut errors = Vec::new();
    let mut valid = Vec::new();
    let mut invalid_interval_count = 0usize;

    for interval in &input.rr_intervals_ms {
        if interval.is_finite() && (300.0..=2000.0).contains(interval) {
            valid.push(*interval);
        } else {
            invalid_interval_count += 1;
        }
    }

    if invalid_interval_count > 0 {
        quality_flags.push("invalid_rr_interval_dropped".to_string());
    }
    if valid.len() < 2 {
        errors.push("not_enough_valid_rr_intervals".to_string());
    }

    let output = if errors.is_empty() {
        let mean_nn_ms = mean(&valid);
        let rmssd_ms = rmssd(&valid);
        let sdnn_sample_ms = sample_sd(&valid, mean_nn_ms);
        let pnn50_fraction = pnn50(&valid);
        Some(HrvReferenceOutput {
            provider: REFERENCE_HRV_PROVIDER.to_string(),
            provider_version: REFERENCE_HRV_TIME_DOMAIN_VERSION.to_string(),
            interval_count: input.rr_intervals_ms.len(),
            valid_interval_count: valid.len(),
            invalid_interval_count,
            mean_nn_ms,
            rmssd_ms,
            sdnn_sample_ms,
            pnn50_fraction,
            components: vec![
                MetricComponent {
                    name: "mean_nn".to_string(),
                    value: mean_nn_ms,
                    unit: "ms".to_string(),
                },
                MetricComponent {
                    name: "rmssd".to_string(),
                    value: rmssd_ms,
                    unit: "ms".to_string(),
                },
                MetricComponent {
                    name: "sdnn_sample".to_string(),
                    value: sdnn_sample_ms,
                    unit: "ms".to_string(),
                },
                MetricComponent {
                    name: "pnn50".to_string(),
                    value: pnn50_fraction,
                    unit: "fraction".to_string(),
                },
            ],
        })
    } else {
        None
    };

    AlgorithmRunResult {
        algorithm_id: REFERENCE_HRV_TIME_DOMAIN_ID.to_string(),
        algorithm_version: REFERENCE_HRV_TIME_DOMAIN_VERSION.to_string(),
        family: "hrv".to_string(),
        start_time: input.start_time.clone(),
        end_time: input.end_time.clone(),
        output,
        quality_flags,
        errors,
        provenance: json!({
            "input_ids": input.input_ids,
            "input_interval_count": input.rr_intervals_ms.len(),
            "provider": REFERENCE_HRV_PROVIDER,
            "provider_kind": "internal_reference",
            "external_provider": null,
            "valid_rr_range_ms": [300.0, 2000.0],
            "expected_values_policy": "hand-derived-reference"
        }),
    }
}

pub fn reference_sleep_actigraphy_summary(
    input: &SleepInput,
) -> AlgorithmRunResult<SleepActigraphyReferenceOutput> {
    let mut quality_flags = Vec::new();
    let mut errors = Vec::new();

    require_finite_positive(
        "time_in_bed_minutes",
        input.time_in_bed_minutes,
        &mut errors,
    );
    require_finite_positive(
        "sleep_duration_minutes",
        input.sleep_duration_minutes,
        &mut errors,
    );
    if input.sleep_duration_minutes > input.time_in_bed_minutes {
        errors.push("sleep_duration_must_not_exceed_time_in_bed".to_string());
    }
    if input.time_in_bed_minutes < 180.0 {
        quality_flags.push("short_actigraphy_window".to_string());
    }

    let output = if errors.is_empty() {
        let wake_minutes = (input.time_in_bed_minutes - input.sleep_duration_minutes).max(0.0);
        let sleep_efficiency_fraction = input.sleep_duration_minutes / input.time_in_bed_minutes;
        let fragmentation_index_per_hour =
            input.disturbance_count as f64 / (input.sleep_duration_minutes / 60.0);
        Some(SleepActigraphyReferenceOutput {
            provider: REFERENCE_SLEEP_ACTIGRAPHY_PROVIDER.to_string(),
            provider_version: REFERENCE_SLEEP_ACTIGRAPHY_VERSION.to_string(),
            time_in_bed_minutes: input.time_in_bed_minutes,
            sleep_minutes: input.sleep_duration_minutes,
            wake_minutes,
            sleep_efficiency_fraction,
            wake_after_sleep_onset_minutes: wake_minutes,
            disturbance_count: input.disturbance_count,
            fragmentation_index_per_hour,
            components: vec![
                MetricComponent {
                    name: "sleep_efficiency".to_string(),
                    value: sleep_efficiency_fraction,
                    unit: "fraction".to_string(),
                },
                MetricComponent {
                    name: "wake_after_sleep_onset".to_string(),
                    value: wake_minutes,
                    unit: "minutes".to_string(),
                },
                MetricComponent {
                    name: "fragmentation_index".to_string(),
                    value: fragmentation_index_per_hour,
                    unit: "events_per_hour".to_string(),
                },
            ],
        })
    } else {
        None
    };

    AlgorithmRunResult {
        algorithm_id: REFERENCE_SLEEP_ACTIGRAPHY_ID.to_string(),
        algorithm_version: REFERENCE_SLEEP_ACTIGRAPHY_VERSION.to_string(),
        family: "sleep".to_string(),
        start_time: input.start_time.clone(),
        end_time: input.end_time.clone(),
        output,
        quality_flags,
        errors,
        provenance: json!({
            "input_ids": input.input_ids,
            "provider": REFERENCE_SLEEP_ACTIGRAPHY_PROVIDER,
            "provider_kind": "internal_reference",
            "external_provider": null,
            "intended_external_benchmarks": ["pyActigraphy", "GGIR"],
            "expected_values_policy": "hand-derived-reference"
        }),
    }
}

pub fn reference_strain_edwards_load(
    input: &StrainInput,
) -> AlgorithmRunResult<StrainEdwardsReferenceOutput> {
    let mut quality_flags = Vec::new();
    let mut errors = Vec::new();

    require_finite_positive("duration_minutes", input.duration_minutes, &mut errors);
    if input.hr_zone_minutes.len() != 5 {
        errors.push("five_hr_zones_required".to_string());
    }
    if input
        .hr_zone_minutes
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
    {
        errors.push("zone_minutes_must_be_finite_non_negative".to_string());
    }
    let zone_minutes_sum = input.hr_zone_minutes.iter().sum::<f64>();
    if (zone_minutes_sum - input.duration_minutes).abs() > 5.0 {
        quality_flags.push("zone_minutes_duration_mismatch".to_string());
    }

    let output = if errors.is_empty() {
        let zone_weights = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let edwards_load = input
            .hr_zone_minutes
            .iter()
            .zip(zone_weights.iter())
            .map(|(minutes, weight)| minutes * weight)
            .sum::<f64>();
        let edwards_load_per_hour = edwards_load / (input.duration_minutes / 60.0);
        let components = input
            .hr_zone_minutes
            .iter()
            .enumerate()
            .map(|(index, minutes)| MetricComponent {
                name: format!("zone_{}", index + 1),
                value: minutes * zone_weights[index],
                unit: "weighted_zone_minutes".to_string(),
            })
            .collect::<Vec<_>>();
        Some(StrainEdwardsReferenceOutput {
            provider: REFERENCE_STRAIN_EDWARDS_PROVIDER.to_string(),
            provider_version: REFERENCE_STRAIN_EDWARDS_VERSION.to_string(),
            duration_minutes: input.duration_minutes,
            zone_minutes: input.hr_zone_minutes.clone(),
            zone_weights,
            edwards_load,
            edwards_load_per_hour,
            components,
        })
    } else {
        None
    };

    AlgorithmRunResult {
        algorithm_id: REFERENCE_STRAIN_EDWARDS_ID.to_string(),
        algorithm_version: REFERENCE_STRAIN_EDWARDS_VERSION.to_string(),
        family: "strain".to_string(),
        start_time: input.start_time.clone(),
        end_time: input.end_time.clone(),
        output,
        quality_flags,
        errors,
        provenance: json!({
            "input_ids": input.input_ids,
            "provider": REFERENCE_STRAIN_EDWARDS_PROVIDER,
            "provider_kind": "internal_reference",
            "external_provider": null,
            "formula": "sum(zone_minutes[i] * zone_weight[i])",
            "zone_weights": [1.0, 2.0, 3.0, 4.0, 5.0],
            "expected_values_policy": "hand-derived-reference"
        }),
    }
}

pub fn reference_stress_hrv_hr_proxy(
    input: &StressInput,
) -> AlgorithmRunResult<StressHrvHrReferenceOutput> {
    let mut quality_flags = Vec::new();
    let mut errors = Vec::new();

    require_finite_positive("heart_rate_bpm", input.heart_rate_bpm, &mut errors);
    require_finite_positive("resting_hr_bpm", input.resting_hr_bpm, &mut errors);
    if !input.hrv_rmssd_ms.is_finite() || input.hrv_rmssd_ms < 0.0 {
        errors.push("hrv_rmssd_ms_must_be_finite_non_negative".to_string());
    }
    require_finite_positive(
        "hrv_baseline_rmssd_ms",
        input.hrv_baseline_rmssd_ms,
        &mut errors,
    );
    if input.motion_intensity_0_to_1 > 0.70 {
        quality_flags.push("high_motion_context_reference_ignores_motion".to_string());
    }
    if !(0.0..=1.0).contains(&input.motion_intensity_0_to_1) {
        quality_flags.push("motion_intensity_outside_reference_range".to_string());
    }

    let output = if errors.is_empty() {
        let heart_rate_elevation_score =
            clamp_0_100((input.heart_rate_bpm - input.resting_hr_bpm).max(0.0) / 60.0 * 100.0);
        let hrv_suppression_score =
            clamp_0_100((1.0 - input.hrv_rmssd_ms / input.hrv_baseline_rmssd_ms) * 100.0);
        let unadjusted_stress_score_0_to_100 =
            heart_rate_elevation_score * 0.60 + hrv_suppression_score * 0.40;
        Some(StressHrvHrReferenceOutput {
            provider: REFERENCE_STRESS_HRV_HR_PROVIDER.to_string(),
            provider_version: REFERENCE_STRESS_HRV_HR_VERSION.to_string(),
            heart_rate_elevation_score,
            hrv_suppression_score,
            unadjusted_stress_score_0_to_100,
            components: vec![
                MetricComponent {
                    name: "heart_rate_elevation".to_string(),
                    value: heart_rate_elevation_score,
                    unit: "score_0_to_100".to_string(),
                },
                MetricComponent {
                    name: "hrv_suppression".to_string(),
                    value: hrv_suppression_score,
                    unit: "score_0_to_100".to_string(),
                },
            ],
        })
    } else {
        None
    };

    AlgorithmRunResult {
        algorithm_id: REFERENCE_STRESS_HRV_HR_ID.to_string(),
        algorithm_version: REFERENCE_STRESS_HRV_HR_VERSION.to_string(),
        family: "stress".to_string(),
        start_time: input.start_time.clone(),
        end_time: input.end_time.clone(),
        output,
        quality_flags,
        errors,
        provenance: json!({
            "input_ids": input.input_ids,
            "provider": REFERENCE_STRESS_HRV_HR_PROVIDER,
            "provider_kind": "internal_reference",
            "external_provider": null,
            "formula": "0.6 * heart_rate_elevation_score + 0.4 * hrv_suppression_score",
            "motion_policy": "ignored_for_reference_proxy",
            "expected_values_policy": "hand-derived-reference"
        }),
    }
}

pub fn hrv_reference_run_record(
    run_id: &str,
    result: &AlgorithmRunResult<HrvReferenceOutput>,
) -> BullResult<AlgorithmRunRecord> {
    reference_run_record(run_id, result, "HRV reference")
}

pub fn sleep_reference_run_record(
    run_id: &str,
    result: &AlgorithmRunResult<SleepActigraphyReferenceOutput>,
) -> BullResult<AlgorithmRunRecord> {
    reference_run_record(run_id, result, "sleep reference")
}

pub fn strain_reference_run_record(
    run_id: &str,
    result: &AlgorithmRunResult<StrainEdwardsReferenceOutput>,
) -> BullResult<AlgorithmRunRecord> {
    reference_run_record(run_id, result, "strain reference")
}

pub fn stress_reference_run_record(
    run_id: &str,
    result: &AlgorithmRunResult<StressHrvHrReferenceOutput>,
) -> BullResult<AlgorithmRunRecord> {
    reference_run_record(run_id, result, "stress reference")
}

fn reference_run_record<T: Serialize>(
    run_id: &str,
    result: &AlgorithmRunResult<T>,
    label: &str,
) -> BullResult<AlgorithmRunRecord> {
    let output_json = serde_json::to_string(&result.output).map_err(|error| {
        BullError::message(format!("cannot serialize {label} output: {error}"))
    })?;
    let quality_flags_json = serde_json::to_string(&result.quality_flags).map_err(|error| {
        BullError::message(format!("cannot serialize {label} quality flags: {error}"))
    })?;
    let provenance_json = serde_json::to_string(&json!({
        "provenance": result.provenance,
        "errors": result.errors
    }))
    .map_err(|error| {
        BullError::message(format!("cannot serialize {label} provenance: {error}"))
    })?;

    Ok(AlgorithmRunRecord {
        run_id: run_id.to_string(),
        algorithm_id: result.algorithm_id.clone(),
        version: result.algorithm_version.clone(),
        start_time: result.start_time.clone(),
        end_time: result.end_time.clone(),
        output_json,
        quality_flags_json,
        provenance_json,
    })
}

fn require_finite_positive(name: &str, value: f64, errors: &mut Vec<String>) {
    if !value.is_finite() || value <= 0.0 {
        errors.push(format!("{name}_must_be_finite_positive"));
    }
}

fn clamp_0_100(value: f64) -> f64 {
    value.clamp(0.0, 100.0)
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn rmssd(values: &[f64]) -> f64 {
    let mean_square = values
        .windows(2)
        .map(|pair| {
            let diff = pair[1] - pair[0];
            diff * diff
        })
        .sum::<f64>()
        / (values.len() - 1) as f64;
    mean_square.sqrt()
}

fn sample_sd(values: &[f64], mean_value: f64) -> f64 {
    let sample_variance = values
        .iter()
        .map(|value| {
            let diff = value - mean_value;
            diff * diff
        })
        .sum::<f64>()
        / (values.len() - 1) as f64;
    sample_variance.sqrt()
}

fn pnn50(values: &[f64]) -> f64 {
    let above_threshold = values
        .windows(2)
        .filter(|pair| (pair[1] - pair[0]).abs() > 50.0)
        .count();
    above_threshold as f64 / (values.len() - 1) as f64
}
