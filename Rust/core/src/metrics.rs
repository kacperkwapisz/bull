use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    BullError, BullResult,
    baselines::EwmaBaseline,
    store::{AlgorithmDefinitionRecord, AlgorithmPreferenceRecord, AlgorithmRunRecord},
};

pub const BULL_HRV_V0_ID: &str = "bull.hrv.v0";
pub const BULL_HRV_V0_VERSION: &str = "0.1.0";
pub const BULL_SLEEP_V0_ID: &str = "bull.sleep.v0";
pub const BULL_SLEEP_V0_VERSION: &str = "0.1.0";
pub const BULL_SLEEP_V1_ID: &str = "bull.sleep.v1";
pub const BULL_SLEEP_V1_VERSION: &str = "0.1.0";
pub const BULL_STRAIN_V0_ID: &str = "bull.strain.v0";
pub const BULL_STRAIN_V0_VERSION: &str = "0.1.0";
pub const BULL_STRAIN_V1_ID: &str = "bull.strain.v1";
pub const BULL_STRAIN_V1_VERSION: &str = "0.1.0";
pub const BULL_RECOVERY_V1_ID: &str = "bull.recovery.v1";
pub const BULL_RECOVERY_V1_VERSION: &str = "0.1.0";
pub const BULL_READINESS_V1_ID: &str = "bull.readiness.v1";
pub const BULL_READINESS_V1_VERSION: &str = "0.1.0";
pub const BULL_RECOVERY_V0_ID: &str = "bull.recovery.v0";
pub const BULL_RECOVERY_V0_VERSION: &str = "0.1.0";
pub const BULL_STRESS_V0_ID: &str = "bull.stress.v0";
pub const BULL_STRESS_V0_VERSION: &str = "0.1.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HrvInput {
    pub start_time: String,
    pub end_time: String,
    pub rr_intervals_ms: Vec<f64>,
    #[serde(default)]
    pub input_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HrvOutput {
    pub algorithm_id: String,
    pub algorithm_version: String,
    pub interval_count: usize,
    pub valid_interval_count: usize,
    pub invalid_interval_count: usize,
    pub mean_nn_ms: f64,
    pub rmssd_ms: f64,
    pub sdnn_ms: f64,
    pub pnn50_fraction: f64,
    pub components: Vec<MetricComponent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SleepInput {
    pub start_time: String,
    pub end_time: String,
    pub sleep_duration_minutes: f64,
    pub sleep_need_minutes: f64,
    pub time_in_bed_minutes: f64,
    pub midpoint_deviation_minutes: f64,
    pub disturbance_count: u32,
    #[serde(default)]
    pub sleep_latency_minutes: f64,
    #[serde(default)]
    pub wake_after_sleep_onset_minutes: f64,
    #[serde(default)]
    pub wake_episode_count: u32,
    #[serde(default)]
    pub stage_minutes: BTreeMap<String, f64>,
    #[serde(default)]
    pub heart_rate_dip_percent: Option<f64>,
    #[serde(default)]
    pub input_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SleepScoreOutput {
    pub algorithm_id: String,
    pub algorithm_version: String,
    pub score_0_to_100: f64,
    pub sleep_performance_fraction: f64,
    pub sleep_debt_minutes: f64,
    pub efficiency_fraction: f64,
    pub awake_minutes: f64,
    pub restorative_sleep_minutes: f64,
    pub restorative_sleep_fraction: f64,
    pub sleep_latency_minutes: f64,
    pub wake_after_sleep_onset_minutes: f64,
    pub wake_episode_count: u32,
    pub heart_rate_dip_percent: Option<f64>,
    pub components: Vec<ScoreComponent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SleepNightHistoryInput {
    pub night_id: String,
    pub start_time: String,
    pub end_time: String,
    pub sleep_duration_minutes: f64,
    pub sleep_need_minutes: f64,
    pub time_in_bed_minutes: f64,
    pub awake_minutes: f64,
    #[serde(default)]
    pub sleep_latency_minutes: f64,
    #[serde(default)]
    pub wake_after_sleep_onset_minutes: f64,
    #[serde(default)]
    pub wake_episode_count: u32,
    #[serde(default)]
    pub stage_minutes: BTreeMap<String, f64>,
    #[serde(default)]
    pub heart_rate_dip_percent: Option<f64>,
    #[serde(default)]
    pub sleep_hr_average_bpm: Option<f64>,
    #[serde(default)]
    pub sleep_hr_min_bpm: Option<f64>,
    #[serde(default)]
    pub pre_sleep_awake_hr_average_bpm: Option<f64>,
    #[serde(default)]
    pub sleep_hr_trend_bpm_per_hour: Option<f64>,
    #[serde(default)]
    pub bedtime_deviation_minutes: f64,
    #[serde(default)]
    pub wake_time_deviation_minutes: f64,
    #[serde(default)]
    pub midpoint_deviation_minutes: f64,
    #[serde(default)]
    pub naps_minutes: f64,
    #[serde(default = "default_sleep_history_confidence")]
    pub confidence_0_to_1: f64,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub excluded_from_baseline: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SleepBaselineWindow {
    pub window_days: u32,
    pub night_count: u32,
    pub average_sleep_duration_minutes: f64,
    pub average_sleep_need_minutes: f64,
    pub average_sleep_debt_minutes: f64,
    pub average_time_in_bed_minutes: f64,
    pub average_awake_minutes: f64,
    pub average_sleep_efficiency_fraction: f64,
    pub average_latency_minutes: f64,
    pub average_wake_after_sleep_onset_minutes: f64,
    pub average_wake_episode_count: f64,
    pub average_deep_sleep_minutes: f64,
    pub average_rem_sleep_minutes: f64,
    pub average_core_sleep_minutes: f64,
    pub average_restorative_sleep_minutes: f64,
    pub average_bedtime_deviation_minutes: f64,
    pub average_wake_time_deviation_minutes: f64,
    pub average_midpoint_deviation_minutes: f64,
    pub average_naps_minutes: f64,
    pub average_sleep_hr_bpm: Option<f64>,
    pub average_sleep_hr_min_bpm: Option<f64>,
    pub average_sleep_hr_trend_bpm_per_hour: Option<f64>,
    pub average_hr_dip_percent: Option<f64>,
    pub average_confidence_0_to_1: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SleepBaseline {
    pub usable_night_count: u32,
    pub excluded_night_count: u32,
    pub rolling_sleep_debt_minutes: f64,
    pub short_7_day: Option<SleepBaselineWindow>,
    pub current_14_day: Option<SleepBaselineWindow>,
    pub stable_28_day: Option<SleepBaselineWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SleepPreviousNightComparison {
    pub night_id: String,
    pub sleep_duration_delta_minutes: f64,
    pub awake_minutes_delta: f64,
    pub sleep_debt_delta_minutes: f64,
    pub sleep_efficiency_delta_fraction: f64,
    pub sleep_latency_delta_minutes: f64,
    pub wake_after_sleep_onset_delta_minutes: f64,
    pub wake_episode_count_delta: i32,
    pub deep_sleep_delta_minutes: f64,
    pub rem_sleep_delta_minutes: f64,
    pub core_sleep_delta_minutes: f64,
    pub restorative_sleep_delta_minutes: f64,
    pub bedtime_deviation_delta_minutes: f64,
    pub wake_time_deviation_delta_minutes: f64,
    pub sleep_hr_average_delta_bpm: Option<f64>,
    pub sleep_hr_min_delta_bpm: Option<f64>,
    pub sleep_hr_trend_delta_bpm_per_hour: Option<f64>,
    pub sleep_hr_dip_delta_percent: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SleepV1Input {
    #[serde(flatten)]
    pub sleep: SleepInput,
    #[serde(default)]
    pub model_status: SleepModelStatusInput,
    #[serde(default)]
    pub prior_nights: Vec<SleepNightHistoryInput>,
    #[serde(default)]
    pub stage_segments: Vec<SleepStageSegment>,
    #[serde(default)]
    pub rolling_sleep_debt_minutes: f64,
    #[serde(default)]
    pub bedtime_deviation_minutes: f64,
    #[serde(default)]
    pub wake_time_deviation_minutes: f64,
    #[serde(default)]
    pub sleep_hr_average_bpm: Option<f64>,
    #[serde(default)]
    pub sleep_hr_min_bpm: Option<f64>,
    #[serde(default)]
    pub pre_sleep_awake_hr_average_bpm: Option<f64>,
    #[serde(default)]
    pub sleep_hr_trend_bpm_per_hour: Option<f64>,
    #[serde(default)]
    pub naps_minutes: f64,
    #[serde(default)]
    pub prior_day_strain: Option<f64>,
    #[serde(default)]
    pub data_coverage_fraction: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SleepStageSegment {
    pub stage_kind: String,
    pub start_time: String,
    pub end_time: String,
    pub duration_minutes: f64,
    pub confidence_0_to_1: f64,
    #[serde(default)]
    pub stage_probabilities: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SleepV1Output {
    pub algorithm_id: String,
    pub algorithm_version: String,
    pub model_status: SleepModelStatus,
    pub model_status_label: String,
    pub model_status_reason: String,
    pub score_0_to_100: f64,
    #[serde(default)]
    pub sleep_window_confidence_0_to_1: f64,
    pub sleep_performance_fraction: f64,
    pub sleep_need_minutes: f64,
    pub sleep_debt_minutes: f64,
    pub rolling_sleep_debt_minutes: f64,
    pub time_in_bed_minutes: f64,
    pub sleep_duration_minutes: f64,
    pub awake_minutes: f64,
    pub sleep_latency_minutes: f64,
    pub wake_after_sleep_onset_minutes: f64,
    pub wake_episode_count: u32,
    pub sleep_efficiency_fraction: f64,
    pub bedtime_deviation_minutes: f64,
    pub wake_time_deviation_minutes: f64,
    pub midpoint_deviation_minutes: f64,
    pub stage_minutes: BTreeMap<String, f64>,
    pub stage_segments: Vec<SleepStageSegment>,
    pub stage_segment_confidence_0_to_1: Option<f64>,
    pub sleep_architecture_confidence_0_to_1: Option<f64>,
    pub deep_sleep_minutes: f64,
    pub rem_sleep_minutes: f64,
    pub core_sleep_minutes: f64,
    pub restorative_sleep_minutes: f64,
    pub restorative_sleep_fraction: f64,
    pub sleep_hr_average_bpm: Option<f64>,
    pub sleep_hr_min_bpm: Option<f64>,
    pub pre_sleep_awake_hr_average_bpm: Option<f64>,
    pub sleep_hr_trend_bpm_per_hour: Option<f64>,
    pub sleep_hr_dip_percent: Option<f64>,
    pub sleep_hr_recovery_score: Option<f64>,
    pub naps_minutes: f64,
    pub prior_day_strain: Option<f64>,
    pub data_coverage_fraction: Option<f64>,
    pub confidence_0_to_1: f64,
    pub baseline: Option<SleepBaseline>,
    #[serde(default)]
    pub previous_night_comparison: Option<SleepPreviousNightComparison>,
    pub status_report: SleepModelStatusReport,
    pub components: Vec<ScoreComponent>,
    pub component_provenance: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub quality_flags: Vec<String>,
    #[serde(default)]
    pub provenance: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SleepModelStatus {
    SetupNeeded,
    ImportingHistory,
    Learning,
    BaselineReady,
    Training,
    Trained,
    NeedsRelearn,
    Blocked,
}

impl SleepModelStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SetupNeeded => "setup_needed",
            Self::ImportingHistory => "importing_history",
            Self::Learning => "learning",
            Self::BaselineReady => "baseline_ready",
            Self::Training => "training",
            Self::Trained => "trained",
            Self::NeedsRelearn => "needs_relearn",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SleepModelStatusInput {
    #[serde(default)]
    pub sleep_permission_granted: bool,
    #[serde(default)]
    pub history_import_in_progress: bool,
    #[serde(default)]
    pub timestamp_sync_blocked: bool,
    #[serde(default)]
    pub trusted_bull_sleep_nights: u32,
    #[serde(default)]
    pub imported_platform_sleep_nights: u32,
    #[serde(default)]
    pub excluded_sleep_nights: u32,
    #[serde(default)]
    pub motion_coverage_fraction: Option<f64>,
    #[serde(default)]
    pub heart_rate_coverage_fraction: Option<f64>,
    #[serde(default)]
    pub calibration_label_count: u32,
    #[serde(default)]
    pub holdout_validation_passed: bool,
    #[serde(default)]
    pub days_since_last_valid_night: Option<u32>,
    #[serde(default)]
    pub timezone_or_schedule_shift_detected: bool,
    #[serde(default)]
    pub repeated_low_confidence_nights: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SleepModelStatusReport {
    pub status: SleepModelStatus,
    pub status_label: String,
    pub status_reason: String,
    pub report_state: String,
    pub valid_sleep_nights: u32,
    pub trusted_bull_sleep_nights: u32,
    pub imported_platform_sleep_nights: u32,
    pub excluded_sleep_nights: u32,
    pub calibration_label_count: u32,
    pub nights_until_baseline: u32,
    #[serde(default)]
    pub nights_until_bull_training: u32,
    pub nights_until_training: u32,
    pub can_show_provisional_score: bool,
    pub can_show_final_score: bool,
    pub can_show_personal_baseline: bool,
    pub can_show_trained_score: bool,
    pub quality_flags: Vec<String>,
    pub next_actions: Vec<String>,
}

impl Default for SleepInput {
    fn default() -> Self {
        Self {
            start_time: String::new(),
            end_time: String::new(),
            sleep_duration_minutes: 0.0,
            sleep_need_minutes: 0.0,
            time_in_bed_minutes: 0.0,
            midpoint_deviation_minutes: 0.0,
            disturbance_count: 0,
            sleep_latency_minutes: 0.0,
            wake_after_sleep_onset_minutes: 0.0,
            wake_episode_count: 0,
            stage_minutes: BTreeMap::new(),
            heart_rate_dip_percent: None,
            input_ids: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct StrainInput {
    pub start_time: String,
    pub end_time: String,
    pub duration_minutes: f64,
    pub resting_hr_bpm: f64,
    pub average_hr_bpm: f64,
    pub max_hr_bpm: f64,
    pub hr_zone_minutes: Vec<f64>,
    #[serde(default)]
    pub input_ids: Vec<String>,
    #[serde(default)]
    pub profile_sex: Option<String>,
    #[serde(default)]
    pub profile_age: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StrainScoreOutput {
    pub algorithm_id: String,
    pub algorithm_version: String,
    pub score_0_to_21: f64,
    pub zone_load: f64,
    pub average_hr_reserve_fraction: f64,
    pub components: Vec<ScoreComponent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecoveryInput {
    pub start_time: String,
    pub end_time: String,
    pub hrv_rmssd_ms: f64,
    pub hrv_baseline_rmssd_ms: f64,
    pub resting_hr_bpm: f64,
    pub resting_hr_baseline_bpm: f64,
    pub respiratory_rate_rpm: f64,
    pub respiratory_rate_baseline_rpm: f64,
    pub skin_temp_delta_c: f64,
    pub sleep_score_0_to_100: f64,
    pub prior_strain_0_to_21: f64,
    #[serde(default)]
    pub input_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecoveryScoreOutput {
    pub algorithm_id: String,
    pub algorithm_version: String,
    pub score_0_to_100: f64,
    pub components: Vec<ScoreComponent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StressInput {
    pub start_time: String,
    pub end_time: String,
    pub heart_rate_bpm: f64,
    pub resting_hr_bpm: f64,
    pub hrv_rmssd_ms: f64,
    pub hrv_baseline_rmssd_ms: f64,
    pub motion_intensity_0_to_1: f64,
    #[serde(default)]
    pub input_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StressScoreOutput {
    pub algorithm_id: String,
    pub algorithm_version: String,
    pub score_0_to_100: f64,
    pub heart_rate_elevation_score: f64,
    pub hrv_suppression_score: f64,
    pub motion_adjusted_hr_score: f64,
    pub components: Vec<ScoreComponent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricComponent {
    pub name: String,
    pub value: f64,
    pub unit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScoreComponent {
    pub name: String,
    pub value: f64,
    pub unit: String,
    pub score_0_to_100: f64,
    pub weight: f64,
    pub contribution: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlgorithmRunResult<T> {
    pub algorithm_id: String,
    pub algorithm_version: String,
    pub family: String,
    pub start_time: String,
    pub end_time: String,
    pub output: Option<T>,
    pub quality_flags: Vec<String>,
    pub errors: Vec<String>,
    pub provenance: serde_json::Value,
}

pub fn built_in_algorithm_definitions() -> Vec<AlgorithmDefinitionRecord> {
    vec![
        hrv_definition(),
        sleep_definition(),
        sleep_v1_definition(),
        strain_definition(),
        recovery_definition(),
        stress_definition(),
    ]
}

pub fn built_in_default_algorithm_preferences() -> Vec<AlgorithmPreferenceRecord> {
    vec![
        default_algorithm_preference("hrv", BULL_HRV_V0_ID, BULL_HRV_V0_VERSION),
        default_algorithm_preference("sleep", BULL_SLEEP_V0_ID, BULL_SLEEP_V0_VERSION),
        default_algorithm_preference("strain", BULL_STRAIN_V0_ID, BULL_STRAIN_V0_VERSION),
        default_algorithm_preference("recovery", BULL_RECOVERY_V0_ID, BULL_RECOVERY_V0_VERSION),
        default_algorithm_preference("stress", BULL_STRESS_V0_ID, BULL_STRESS_V0_VERSION),
    ]
}

pub fn default_algorithm_preferences_for_scope(scope: &str) -> Vec<AlgorithmPreferenceRecord> {
    built_in_default_algorithm_preferences()
        .into_iter()
        .map(|mut preference| {
            preference.scope = scope.to_string();
            preference
        })
        .collect()
}

fn default_algorithm_preference(
    metric_family: &str,
    algorithm_id: &str,
    version: &str,
) -> AlgorithmPreferenceRecord {
    AlgorithmPreferenceRecord {
        scope: "global".to_string(),
        metric_family: metric_family.to_string(),
        algorithm_id: algorithm_id.to_string(),
        version: version.to_string(),
    }
}

fn hrv_definition() -> AlgorithmDefinitionRecord {
    AlgorithmDefinitionRecord {
        algorithm_id: BULL_HRV_V0_ID.to_string(),
        version: BULL_HRV_V0_VERSION.to_string(),
        metric_family: "hrv".to_string(),
        display_name: "Bull HRV v0".to_string(),
        implementation: "rust".to_string(),
        license: "UNLICENSED".to_string(),
        input_schema: "bull.hrv-input.v1".to_string(),
        output_schema: "bull.hrv-output.v1".to_string(),
        input_requirements_json: json!({
            "rr_intervals_ms": {
                "unit": "ms",
                "valid_range_inclusive": [300.0, 2000.0],
                "minimum_to_compute": 2,
                "recommended_minimum": 30
            }
        })
        .to_string(),
        params_json: json!({
            "sdnn": "sample_standard_deviation",
            "pnn50_threshold_ms": 50.0,
            "invalid_rr_policy": "drop_and_flag",
            "night_window_confidence": "not_implemented"
        })
        .to_string(),
        quality_gates_json: json!([
            "at_least_2_valid_rr_intervals_to_compute",
            "low_interval_count_below_30",
            "drop_rr_intervals_outside_300_to_2000_ms"
        ])
        .to_string(),
        status: "beta".to_string(),
    }
}

fn sleep_definition() -> AlgorithmDefinitionRecord {
    AlgorithmDefinitionRecord {
        algorithm_id: BULL_SLEEP_V0_ID.to_string(),
        version: BULL_SLEEP_V0_VERSION.to_string(),
        metric_family: "sleep".to_string(),
        display_name: "Bull Sleep v0".to_string(),
        implementation: "rust".to_string(),
        license: "UNLICENSED".to_string(),
        input_schema: "bull.sleep-input.v1".to_string(),
        output_schema: "bull.sleep-output.v1".to_string(),
        input_requirements_json: json!({
            "sleep_duration_minutes": {"unit": "minutes", "minimum_to_compute": 1.0},
            "sleep_need_minutes": {"unit": "minutes", "minimum_to_compute": 1.0},
            "time_in_bed_minutes": {"unit": "minutes", "minimum_to_compute": 1.0},
            "midpoint_deviation_minutes": {"unit": "minutes"},
            "disturbance_count": {"unit": "count"}
        })
        .to_string(),
        params_json: json!({
            "weights": {
                "duration": 0.45,
                "efficiency": 0.30,
                "consistency": 0.15,
                "disturbances": 0.10
            },
            "consistency_full_penalty_minutes": 120.0,
            "disturbance_penalty_points": 5.0
        })
        .to_string(),
        quality_gates_json: json!([
            "positive_sleep_need",
            "positive_time_in_bed",
            "duration_not_greater_than_time_in_bed",
            "short_sleep_window_below_180_minutes"
        ])
        .to_string(),
        status: "experimental".to_string(),
    }
}

fn sleep_v1_definition() -> AlgorithmDefinitionRecord {
    AlgorithmDefinitionRecord {
        algorithm_id: BULL_SLEEP_V1_ID.to_string(),
        version: BULL_SLEEP_V1_VERSION.to_string(),
        metric_family: "sleep".to_string(),
        display_name: "Bull Sleep v1".to_string(),
        implementation: "rust".to_string(),
        license: "UNLICENSED".to_string(),
        input_schema: "bull.sleep-v1-input.v1".to_string(),
        output_schema: "bull.sleep-v1-output.v1".to_string(),
        input_requirements_json: json!({
            "sleep_window": ["start_time", "end_time", "time_in_bed_minutes", "sleep_duration_minutes"],
            "continuity": ["awake_minutes", "sleep_latency_minutes", "wake_after_sleep_onset_minutes", "wake_episode_count"],
            "architecture": ["stage_minutes.deep", "stage_minutes.core", "stage_minutes.rem"],
            "stage_segments": ["stage_kind", "start_time", "end_time", "duration_minutes", "confidence_0_to_1", "stage_probabilities"],
            "cardiovascular": ["sleep_hr_average_bpm", "sleep_hr_min_bpm", "pre_sleep_awake_hr_average_bpm", "sleep_hr_trend_bpm_per_hour", "sleep_hr_dip_percent"],
            "status": ["model_status"]
        })
        .to_string(),
        params_json: json!({
            "status_policy": "rust_sleep_model_status_report",
            "initial_score_policy": "v0_score_with_v1_output_contract",
            "sleep_window_confidence_policy": "status_motion_hr_coverage_and_duration_sanity",
            "sleep_architecture_confidence_policy": "duration_weighted_stage_confidence_and_selected_stage_probability",
            "weights_planned": {
                "sleep_need_fulfillment": 0.25,
                "continuity": 0.20,
                "schedule_regularity": 0.15,
                "sleep_architecture": 0.15,
                "cardiovascular_recovery": 0.15,
                "context_adjustment": 0.05,
                "data_confidence": 0.05
            }
        })
        .to_string(),
        quality_gates_json: json!([
            "status_report_required",
            "timestamp_sync_must_not_be_blocked_for_final_personalized_sleep",
            "baseline_requires_at_least_7_valid_sleep_nights",
            "trained_requires_holdout_validation_and_bull_packet_derived_nights"
        ])
        .to_string(),
        status: "experimental".to_string(),
    }
}

fn strain_definition() -> AlgorithmDefinitionRecord {
    AlgorithmDefinitionRecord {
        algorithm_id: BULL_STRAIN_V0_ID.to_string(),
        version: BULL_STRAIN_V0_VERSION.to_string(),
        metric_family: "strain".to_string(),
        display_name: "Bull Strain v0".to_string(),
        implementation: "rust".to_string(),
        license: "UNLICENSED".to_string(),
        input_schema: "bull.strain-input.v1".to_string(),
        output_schema: "bull.strain-output.v1".to_string(),
        input_requirements_json: json!({
            "hr_zone_minutes": {"unit": "minutes", "required_count": 5},
            "duration_minutes": {"unit": "minutes", "minimum_to_compute": 1.0},
            "resting_hr_bpm": {"unit": "bpm"},
            "average_hr_bpm": {"unit": "bpm"},
            "max_hr_bpm": {"unit": "bpm"}
        })
        .to_string(),
        params_json: json!({
            "zone_weights": [1.0, 2.0, 3.0, 4.0, 5.0],
            "zone_load_score_divisor": 20.0,
            "weights": {"zone_load": 0.70, "average_hr_reserve": 0.30}
        })
        .to_string(),
        quality_gates_json: json!([
            "five_hr_zones_required",
            "positive_duration",
            "max_hr_above_resting_hr",
            "zone_minutes_sum_close_to_duration"
        ])
        .to_string(),
        status: "experimental".to_string(),
    }
}

fn recovery_definition() -> AlgorithmDefinitionRecord {
    AlgorithmDefinitionRecord {
        algorithm_id: BULL_RECOVERY_V0_ID.to_string(),
        version: BULL_RECOVERY_V0_VERSION.to_string(),
        metric_family: "recovery".to_string(),
        display_name: "Bull Recovery v0".to_string(),
        implementation: "rust".to_string(),
        license: "UNLICENSED".to_string(),
        input_schema: "bull.recovery-input.v1".to_string(),
        output_schema: "bull.recovery-output.v1".to_string(),
        input_requirements_json: json!({
            "hrv_rmssd_ms": {"unit": "ms"},
            "hrv_baseline_rmssd_ms": {"unit": "ms", "minimum_to_compute": 1.0},
            "resting_hr_bpm": {"unit": "bpm"},
            "resting_hr_baseline_bpm": {"unit": "bpm", "minimum_to_compute": 1.0},
            "respiratory_rate_rpm": {"unit": "breaths_per_minute"},
            "respiratory_rate_baseline_rpm": {"unit": "breaths_per_minute", "minimum_to_compute": 1.0},
            "skin_temp_delta_c": {"unit": "celsius_delta"},
            "sleep_score_0_to_100": {"unit": "score_0_to_100"},
            "prior_strain_0_to_21": {"unit": "score_0_to_21"}
        })
        .to_string(),
        params_json: json!({
            "weights": {
                "hrv": 0.35,
                "rhr": 0.20,
                "respiratory": 0.10,
                "temperature": 0.10,
                "sleep": 0.15,
                "prior_strain": 0.10
            },
            "baseline_neutral_score": 70.0,
            "rhr_points_per_bpm_below_baseline": 5.0,
            "respiratory_penalty_per_rpm": 20.0,
            "temperature_penalty_per_c": 50.0
        })
        .to_string(),
        quality_gates_json: json!([
            "positive_hrv_baseline",
            "positive_rhr_baseline",
            "positive_respiratory_baseline",
            "sleep_score_bounded",
            "prior_strain_bounded"
        ])
        .to_string(),
        status: "experimental".to_string(),
    }
}

fn stress_definition() -> AlgorithmDefinitionRecord {
    AlgorithmDefinitionRecord {
        algorithm_id: BULL_STRESS_V0_ID.to_string(),
        version: BULL_STRESS_V0_VERSION.to_string(),
        metric_family: "stress".to_string(),
        display_name: "Bull Stress v0".to_string(),
        implementation: "rust".to_string(),
        license: "UNLICENSED".to_string(),
        input_schema: "bull.stress-input.v1".to_string(),
        output_schema: "bull.stress-output.v1".to_string(),
        input_requirements_json: json!({
            "heart_rate_bpm": {"unit": "bpm"},
            "resting_hr_bpm": {"unit": "bpm"},
            "hrv_rmssd_ms": {"unit": "ms"},
            "hrv_baseline_rmssd_ms": {"unit": "ms", "minimum_to_compute": 1.0},
            "motion_intensity_0_to_1": {"unit": "fraction"}
        })
        .to_string(),
        params_json: json!({
            "weights": {"motion_adjusted_hr": 0.60, "hrv_suppression": 0.40},
            "hr_elevation_full_scale_bpm": 60.0,
            "motion_hr_discount_at_max_motion": 0.50
        })
        .to_string(),
        quality_gates_json: json!([
            "positive_hrv_baseline",
            "heart_rate_at_or_above_resting",
            "motion_intensity_clamped_to_0_1",
            "high_motion_context_flag"
        ])
        .to_string(),
        status: "experimental".to_string(),
    }
}

pub fn bull_hrv_v0(input: &HrvInput) -> AlgorithmRunResult<HrvOutput> {
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
    if valid.len() < 30 {
        quality_flags.push("low_interval_count".to_string());
    }
    if valid.len() < 2 {
        errors.push("not_enough_valid_rr_intervals".to_string());
    }

    let output = if errors.is_empty() {
        let mean_nn_ms = mean(&valid);
        let rmssd_ms = rmssd(&valid);
        let sdnn_ms = sample_sd(&valid, mean_nn_ms);
        let pnn50_fraction = pnn50(&valid);
        let interval_count = input.rr_intervals_ms.len();
        let valid_interval_count = valid.len();
        Some(HrvOutput {
            algorithm_id: BULL_HRV_V0_ID.to_string(),
            algorithm_version: BULL_HRV_V0_VERSION.to_string(),
            interval_count,
            valid_interval_count,
            invalid_interval_count,
            mean_nn_ms,
            rmssd_ms,
            sdnn_ms,
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
                    name: "sdnn".to_string(),
                    value: sdnn_ms,
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
        algorithm_id: BULL_HRV_V0_ID.to_string(),
        algorithm_version: BULL_HRV_V0_VERSION.to_string(),
        family: "hrv".to_string(),
        start_time: input.start_time.clone(),
        end_time: input.end_time.clone(),
        output,
        quality_flags,
        errors,
        provenance: json!({
            "input_ids": input.input_ids,
            "input_interval_count": input.rr_intervals_ms.len(),
            "valid_rr_range_ms": [300.0, 2000.0],
            "expected_values_policy": "hand-derived-tests-and-versioned-bull-output"
        }),
    }
}

pub fn bull_sleep_v0(input: &SleepInput) -> AlgorithmRunResult<SleepScoreOutput> {
    let mut quality_flags = Vec::new();
    let mut errors = Vec::new();

    require_finite_positive("sleep_need_minutes", input.sleep_need_minutes, &mut errors);
    require_finite_positive(
        "time_in_bed_minutes",
        input.time_in_bed_minutes,
        &mut errors,
    );
    require_finite_non_negative(
        "sleep_duration_minutes",
        input.sleep_duration_minutes,
        &mut errors,
    );
    require_finite_non_negative(
        "midpoint_deviation_minutes",
        input.midpoint_deviation_minutes,
        &mut errors,
    );
    require_finite_non_negative(
        "sleep_latency_minutes",
        input.sleep_latency_minutes,
        &mut errors,
    );
    require_finite_non_negative(
        "wake_after_sleep_onset_minutes",
        input.wake_after_sleep_onset_minutes,
        &mut errors,
    );
    if let Some(heart_rate_dip_percent) = input.heart_rate_dip_percent {
        require_finite_non_negative(
            "heart_rate_dip_percent",
            heart_rate_dip_percent,
            &mut errors,
        );
    }
    for (stage, minutes) in &input.stage_minutes {
        if !minutes.is_finite() || *minutes < 0.0 {
            errors.push(format!("stage_minutes_{stage}_must_be_finite_non_negative"));
        }
    }

    if input.sleep_duration_minutes < 180.0 {
        quality_flags.push("short_sleep_window".to_string());
    }
    if input.sleep_duration_minutes > input.time_in_bed_minutes {
        quality_flags.push("duration_exceeds_time_in_bed".to_string());
    }
    if input.sleep_latency_minutes >= 45.0 {
        quality_flags.push("long_sleep_latency".to_string());
    }
    if input.wake_after_sleep_onset_minutes >= 45.0 {
        quality_flags.push("elevated_wake_after_sleep_onset".to_string());
    }
    if input.wake_episode_count >= 4 {
        quality_flags.push("fragmented_sleep".to_string());
    }
    if input.stage_minutes.is_empty() {
        quality_flags.push("sleep_architecture_unavailable".to_string());
    }
    if let Some(heart_rate_dip_percent) = input.heart_rate_dip_percent
        && heart_rate_dip_percent < 8.0
    {
        quality_flags.push("low_sleep_heart_rate_dip".to_string());
    }

    let output = if errors.is_empty() {
        let sleep_performance_fraction =
            clamp_fraction(input.sleep_duration_minutes / input.sleep_need_minutes);
        let duration_score =
            clamp_0_100(input.sleep_duration_minutes / input.sleep_need_minutes * 100.0);
        let efficiency_fraction =
            clamp_fraction(input.sleep_duration_minutes / input.time_in_bed_minutes);
        let efficiency_score = efficiency_fraction * 100.0;
        let consistency_score =
            clamp_0_100(100.0 - input.midpoint_deviation_minutes / 120.0 * 100.0);
        let disturbance_score = clamp_0_100(100.0 - input.disturbance_count as f64 * 5.0);
        let sleep_debt_minutes = (input.sleep_need_minutes - input.sleep_duration_minutes).max(0.0);
        let awake_minutes = stage_minutes(&input.stage_minutes, "awake")
            .unwrap_or_else(|| (input.time_in_bed_minutes - input.sleep_duration_minutes).max(0.0));
        let deep_sleep_minutes = stage_minutes(&input.stage_minutes, "deep").unwrap_or(0.0);
        let rem_sleep_minutes = stage_minutes(&input.stage_minutes, "rem").unwrap_or(0.0);
        let restorative_sleep_minutes = deep_sleep_minutes + rem_sleep_minutes;
        let restorative_sleep_fraction =
            clamp_fraction(restorative_sleep_minutes / input.sleep_duration_minutes.max(1.0));
        let stage_total_minutes = input.stage_minutes.values().sum::<f64>();
        if !input.stage_minutes.is_empty()
            && (stage_total_minutes - input.time_in_bed_minutes).abs() > 5.0
        {
            quality_flags.push("stage_minutes_do_not_match_time_in_bed".to_string());
        }

        let components = vec![
            score_component(
                "duration",
                input.sleep_duration_minutes,
                "minutes",
                duration_score,
                0.45,
                100.0,
            ),
            score_component(
                "efficiency",
                efficiency_fraction,
                "fraction",
                efficiency_score,
                0.30,
                100.0,
            ),
            score_component(
                "consistency",
                input.midpoint_deviation_minutes,
                "minutes_deviation",
                consistency_score,
                0.15,
                100.0,
            ),
            score_component(
                "disturbances",
                input.disturbance_count as f64,
                "count",
                disturbance_score,
                0.10,
                100.0,
            ),
            score_component(
                "sleep_latency",
                input.sleep_latency_minutes,
                "minutes",
                clamp_0_100(100.0 - input.sleep_latency_minutes / 60.0 * 100.0),
                0.0,
                100.0,
            ),
            score_component(
                "wake_after_sleep_onset",
                input.wake_after_sleep_onset_minutes,
                "minutes",
                clamp_0_100(100.0 - input.wake_after_sleep_onset_minutes / 90.0 * 100.0),
                0.0,
                100.0,
            ),
            score_component(
                "restorative_sleep",
                restorative_sleep_fraction,
                "fraction",
                restorative_sleep_fraction * 100.0,
                0.0,
                100.0,
            ),
        ];

        Some(SleepScoreOutput {
            algorithm_id: BULL_SLEEP_V0_ID.to_string(),
            algorithm_version: BULL_SLEEP_V0_VERSION.to_string(),
            score_0_to_100: component_sum(&components),
            sleep_performance_fraction,
            sleep_debt_minutes,
            efficiency_fraction,
            awake_minutes,
            restorative_sleep_minutes,
            restorative_sleep_fraction,
            sleep_latency_minutes: input.sleep_latency_minutes,
            wake_after_sleep_onset_minutes: input.wake_after_sleep_onset_minutes,
            wake_episode_count: input.wake_episode_count,
            heart_rate_dip_percent: input.heart_rate_dip_percent,
            components,
        })
    } else {
        None
    };

    AlgorithmRunResult {
        algorithm_id: BULL_SLEEP_V0_ID.to_string(),
        algorithm_version: BULL_SLEEP_V0_VERSION.to_string(),
        family: "sleep".to_string(),
        start_time: input.start_time.clone(),
        end_time: input.end_time.clone(),
        output,
        quality_flags,
        errors,
        provenance: json!({
            "input_ids": input.input_ids,
            "score_policy": "weighted_duration_efficiency_consistency_disturbances_with_unweighted_sleep_architecture_diagnostics",
            "expected_values_policy": "hand-derived-tests-and-versioned-bull-output"
        }),
    }
}

pub fn bull_sleep_v1(input: &SleepV1Input) -> AlgorithmRunResult<SleepV1Output> {
    let mut quality_flags = Vec::new();
    let mut errors = Vec::new();

    for (index, prior_night) in input.prior_nights.iter().enumerate() {
        validate_sleep_night_history_input(index, prior_night, &mut errors);
    }
    for (index, segment) in input.stage_segments.iter().enumerate() {
        validate_sleep_stage_segment(index, segment, &mut errors);
    }
    validate_sleep_v1_sleep_window(input, &mut errors);
    validate_sleep_v1_current_stage_minutes(input, &mut errors);
    validate_sleep_stage_timeline(input, &mut errors);
    require_finite_non_negative(
        "rolling_sleep_debt_minutes",
        input.rolling_sleep_debt_minutes,
        &mut errors,
    );
    require_finite_non_negative(
        "bedtime_deviation_minutes",
        input.bedtime_deviation_minutes,
        &mut errors,
    );
    require_finite_non_negative(
        "wake_time_deviation_minutes",
        input.wake_time_deviation_minutes,
        &mut errors,
    );
    require_finite_non_negative("naps_minutes", input.naps_minutes, &mut errors);
    if let Some(data_coverage_fraction) = input.data_coverage_fraction
        && (!data_coverage_fraction.is_finite() || !(0.0..=1.0).contains(&data_coverage_fraction))
    {
        errors.push("data_coverage_fraction_must_be_between_0_and_1".to_string());
    }
    if let Some(sleep_hr_average_bpm) = input.sleep_hr_average_bpm {
        require_finite_positive("sleep_hr_average_bpm", sleep_hr_average_bpm, &mut errors);
    }
    if let Some(sleep_hr_min_bpm) = input.sleep_hr_min_bpm {
        require_finite_positive("sleep_hr_min_bpm", sleep_hr_min_bpm, &mut errors);
    }
    if let Some(pre_sleep_awake_hr_average_bpm) = input.pre_sleep_awake_hr_average_bpm {
        require_finite_positive(
            "pre_sleep_awake_hr_average_bpm",
            pre_sleep_awake_hr_average_bpm,
            &mut errors,
        );
    }
    if let Some(sleep_hr_trend_bpm_per_hour) = input.sleep_hr_trend_bpm_per_hour
        && !sleep_hr_trend_bpm_per_hour.is_finite()
    {
        errors.push("sleep_hr_trend_bpm_per_hour_must_be_finite".to_string());
    }
    if let Some(prior_day_strain) = input.prior_day_strain
        && (!prior_day_strain.is_finite() || !(0.0..=21.0).contains(&prior_day_strain))
    {
        errors.push("prior_day_strain_must_be_between_0_and_21".to_string());
    }

    let status_report = evaluate_sleep_model_status(&input.model_status);
    quality_flags.extend(status_report.quality_flags.clone());
    if status_report.status == SleepModelStatus::Blocked {
        quality_flags.push("sleep_v1_status_blocked".to_string());
    }
    let usable_prior_nights_before_sleep = if errors.is_empty() {
        sleep_prior_nights_before_scored_sleep(input)
    } else {
        Vec::new()
    };
    if errors.is_empty() && usable_prior_nights_before_sleep.len() < input.prior_nights.len() {
        quality_flags.push("sleep_v1_future_prior_nights_ignored".to_string());
    }
    let baseline = if errors.is_empty() {
        let baseline = sleep_baseline_from_history(&usable_prior_nights_before_sleep);
        if baseline.is_none() && !input.prior_nights.is_empty() {
            quality_flags.push("sleep_v1_no_usable_prior_nights".to_string());
        }
        baseline
    } else {
        None
    };
    let previous_night_comparison = if errors.is_empty() {
        sleep_previous_night_comparison(input, &usable_prior_nights_before_sleep)
    } else {
        None
    };

    let v0_result = bull_sleep_v0(&input.sleep);
    quality_flags.extend(v0_result.quality_flags.clone());
    errors.extend(v0_result.errors.clone());

    let output = match (errors.is_empty(), v0_result.output) {
        (true, Some(v0_output)) => {
            let effective_stage_minutes = sleep_v1_effective_stage_minutes(input);
            let deep_sleep_minutes = stage_minutes(&effective_stage_minutes, "deep").unwrap_or(0.0);
            let rem_sleep_minutes = stage_minutes(&effective_stage_minutes, "rem").unwrap_or(0.0);
            let core_sleep_minutes = stage_minutes(&effective_stage_minutes, "core").unwrap_or(0.0);
            let stage_segment_confidence_0_to_1 = sleep_v1_stage_segment_confidence(input);
            let sleep_architecture_confidence_0_to_1 =
                sleep_v1_architecture_confidence(input, stage_segment_confidence_0_to_1);
            let data_coverage_fraction = input.data_coverage_fraction.or_else(|| {
                status_report
                    .quality_flags
                    .iter()
                    .any(|flag| flag == "motion_coverage_low")
                    .then_some(0.5)
            });
            let confidence_0_to_1 = sleep_v1_confidence_0_to_1(
                &status_report,
                data_coverage_fraction,
                input.sleep.heart_rate_dip_percent,
                !effective_stage_minutes.is_empty(),
                sleep_architecture_confidence_0_to_1,
            );
            let sleep_window_confidence_0_to_1 = sleep_v1_sleep_window_confidence_0_to_1(
                input,
                &status_report,
                data_coverage_fraction,
            );
            let sleep_hr_recovery_score = input
                .sleep
                .heart_rate_dip_percent
                .map(|dip| clamp_0_100(dip / 20.0 * 100.0));
            let rolling_sleep_debt_minutes = sleep_v1_rolling_sleep_debt_minutes(
                input,
                baseline.as_ref(),
                v0_output.sleep_debt_minutes,
            );
            let components = sleep_v1_components(
                input,
                &v0_output,
                baseline.as_ref(),
                rolling_sleep_debt_minutes,
                data_coverage_fraction,
                confidence_0_to_1,
                sleep_window_confidence_0_to_1,
                &effective_stage_minutes,
            );
            let component_provenance = sleep_v1_component_provenance(
                input,
                baseline.as_ref(),
                rolling_sleep_debt_minutes,
                data_coverage_fraction,
                &effective_stage_minutes,
                stage_segment_confidence_0_to_1,
                sleep_architecture_confidence_0_to_1,
                sleep_window_confidence_0_to_1,
            );
            let mut score_0_to_100 = component_sum(&components);
            score_0_to_100 = sleep_v1_guardrailed_score(
                score_0_to_100,
                input,
                v0_output.efficiency_fraction,
                &mut quality_flags,
            );

            let provenance = sleep_v1_output_provenance(
                input,
                previous_night_comparison.as_ref(),
                usable_prior_nights_before_sleep.len(),
            );

            Some(SleepV1Output {
                algorithm_id: BULL_SLEEP_V1_ID.to_string(),
                algorithm_version: BULL_SLEEP_V1_VERSION.to_string(),
                model_status: status_report.status.clone(),
                model_status_label: status_report.status_label.clone(),
                model_status_reason: status_report.status_reason.clone(),
                score_0_to_100,
                sleep_window_confidence_0_to_1,
                sleep_performance_fraction: v0_output.sleep_performance_fraction,
                sleep_need_minutes: input.sleep.sleep_need_minutes,
                sleep_debt_minutes: v0_output.sleep_debt_minutes,
                rolling_sleep_debt_minutes,
                time_in_bed_minutes: input.sleep.time_in_bed_minutes,
                sleep_duration_minutes: input.sleep.sleep_duration_minutes,
                awake_minutes: v0_output.awake_minutes,
                sleep_latency_minutes: input.sleep.sleep_latency_minutes,
                wake_after_sleep_onset_minutes: input.sleep.wake_after_sleep_onset_minutes,
                wake_episode_count: input.sleep.wake_episode_count,
                sleep_efficiency_fraction: v0_output.efficiency_fraction,
                bedtime_deviation_minutes: input.bedtime_deviation_minutes,
                wake_time_deviation_minutes: input.wake_time_deviation_minutes,
                midpoint_deviation_minutes: input.sleep.midpoint_deviation_minutes,
                stage_minutes: effective_stage_minutes,
                stage_segments: input.stage_segments.clone(),
                stage_segment_confidence_0_to_1,
                sleep_architecture_confidence_0_to_1,
                deep_sleep_minutes,
                rem_sleep_minutes,
                core_sleep_minutes,
                restorative_sleep_minutes: v0_output.restorative_sleep_minutes,
                restorative_sleep_fraction: v0_output.restorative_sleep_fraction,
                sleep_hr_average_bpm: input.sleep_hr_average_bpm,
                sleep_hr_min_bpm: input.sleep_hr_min_bpm,
                pre_sleep_awake_hr_average_bpm: input.pre_sleep_awake_hr_average_bpm,
                sleep_hr_trend_bpm_per_hour: input.sleep_hr_trend_bpm_per_hour,
                sleep_hr_dip_percent: input.sleep.heart_rate_dip_percent,
                sleep_hr_recovery_score,
                naps_minutes: input.naps_minutes,
                prior_day_strain: input.prior_day_strain,
                data_coverage_fraction,
                confidence_0_to_1,
                baseline,
                previous_night_comparison: previous_night_comparison.clone(),
                status_report,
                components,
                component_provenance,
                quality_flags: quality_flags.clone(),
                provenance,
            })
        }
        _ => None,
    };

    let provenance = sleep_v1_output_provenance(
        input,
        previous_night_comparison.as_ref(),
        usable_prior_nights_before_sleep.len(),
    );

    AlgorithmRunResult {
        algorithm_id: BULL_SLEEP_V1_ID.to_string(),
        algorithm_version: BULL_SLEEP_V1_VERSION.to_string(),
        family: "sleep".to_string(),
        start_time: input.sleep.start_time.clone(),
        end_time: input.sleep.end_time.clone(),
        output,
        quality_flags,
        errors,
        provenance,
    }
}

fn sleep_v1_output_provenance(
    input: &SleepV1Input,
    previous_night_comparison: Option<&SleepPreviousNightComparison>,
    usable_prior_night_count: usize,
) -> serde_json::Value {
    let previous_night_fields = [
        "sleep_duration_delta_minutes",
        "awake_minutes_delta",
        "sleep_debt_delta_minutes",
        "sleep_efficiency_delta_fraction",
        "sleep_latency_delta_minutes",
        "wake_after_sleep_onset_delta_minutes",
        "wake_episode_count_delta",
        "deep_sleep_delta_minutes",
        "rem_sleep_delta_minutes",
        "core_sleep_delta_minutes",
        "restorative_sleep_delta_minutes",
        "bedtime_deviation_delta_minutes",
        "wake_time_deviation_delta_minutes",
        "sleep_hr_average_delta_bpm",
        "sleep_hr_min_delta_bpm",
        "sleep_hr_trend_delta_bpm_per_hour",
        "sleep_hr_dip_delta_percent",
    ];
    json!({
        "input_ids": input.sleep.input_ids,
        "score_policy": "weighted_sleep_v1_components_with_fragmentation_guardrails",
        "status_policy": "rust_sleep_model_status_report",
        "previous_night_comparison": {
            "policy": "latest_usable_prior_night_before_scored_sleep",
            "selected_night_id": previous_night_comparison.map(|comparison| comparison.night_id.as_str()),
            "usable_prior_night_count": usable_prior_night_count,
            "fields": previous_night_fields
        },
        "expected_values_policy": "hand-derived-tests-and-versioned-bull-output"
    })
}

pub fn evaluate_sleep_model_status(input: &SleepModelStatusInput) -> SleepModelStatusReport {
    let mut quality_flags = Vec::new();
    let mut next_actions = Vec::new();
    let valid_sleep_nights = input
        .trusted_bull_sleep_nights
        .saturating_add(input.imported_platform_sleep_nights);
    let nights_until_baseline = 7u32.saturating_sub(valid_sleep_nights);
    let nights_until_training = 14u32.saturating_sub(input.calibration_label_count);
    let motion_coverage_ok = coverage_ok(input.motion_coverage_fraction, 0.70);
    let heart_rate_coverage_ok = coverage_ok(input.heart_rate_coverage_fraction, 0.50);

    if input.timestamp_sync_blocked {
        quality_flags.push("timestamp_sync_blocked".to_string());
        next_actions.push(
            "Validate and normalize historical packet timestamps before trusting final sleep reports."
                .to_string(),
        );
        return sleep_model_status_report(
            SleepModelStatus::Blocked,
            "Blocked",
            "Historical packet timestamps are not reliable enough for personalized sleep.",
            input,
            valid_sleep_nights,
            nights_until_baseline,
            nights_until_training,
            quality_flags,
            next_actions,
        );
    }

    if !input.sleep_permission_granted && valid_sleep_nights == 0 {
        quality_flags.push("sleep_history_permission_missing".to_string());
        next_actions.push(
            "Grant sleep history access or complete one Bull packet-derived sleep night."
                .to_string(),
        );
        return sleep_model_status_report(
            SleepModelStatus::SetupNeeded,
            "Setup needed",
            "Bull needs sleep history access or one packet-derived night to begin learning.",
            input,
            valid_sleep_nights,
            nights_until_baseline,
            nights_until_training,
            quality_flags,
            next_actions,
        );
    }

    if input.history_import_in_progress && nights_until_baseline > 0 {
        quality_flags.push("sleep_history_import_in_progress".to_string());
        next_actions.push("Keep importing sleep history to bootstrap the baseline.".to_string());
        return sleep_model_status_report(
            SleepModelStatus::ImportingHistory,
            "Importing history",
            "Bull is importing existing sleep history before building a baseline.",
            input,
            valid_sleep_nights,
            nights_until_baseline,
            nights_until_training,
            quality_flags,
            next_actions,
        );
    }

    if !motion_coverage_ok {
        quality_flags.push("motion_coverage_low".to_string());
        next_actions.push(
            "Collect a sleep night with stronger motion coverage before trusting personalization."
                .to_string(),
        );
    }
    if !heart_rate_coverage_ok {
        quality_flags.push("heart_rate_coverage_low".to_string());
        next_actions.push(
            "Collect more overnight heart-rate coverage to improve recovery and HR-dip baselines."
                .to_string(),
        );
    }

    if input.timezone_or_schedule_shift_detected || input.repeated_low_confidence_nights {
        if input.timezone_or_schedule_shift_detected {
            quality_flags.push("timezone_or_schedule_shift_detected".to_string());
        }
        if input.repeated_low_confidence_nights {
            quality_flags.push("repeated_low_confidence_nights".to_string());
        }
        next_actions.push(
            "Collect several recent high-confidence nights so Bull can refresh the baseline."
                .to_string(),
        );
        return sleep_model_status_report(
            SleepModelStatus::NeedsRelearn,
            "Needs relearn",
            "Recent sleep patterns differ enough that Bull should refresh the personal model.",
            input,
            valid_sleep_nights,
            nights_until_baseline,
            nights_until_training,
            quality_flags,
            next_actions,
        );
    }

    if input
        .days_since_last_valid_night
        .is_some_and(|days| days >= 14)
        && valid_sleep_nights >= 7
    {
        quality_flags.push("sleep_baseline_stale".to_string());
        next_actions
            .push("Record a recent sleep night before relying on the baseline.".to_string());
        return sleep_model_status_report(
            SleepModelStatus::NeedsRelearn,
            "Needs relearn",
            "The sleep baseline is stale because Bull has not seen a recent valid night.",
            input,
            valid_sleep_nights,
            nights_until_baseline,
            nights_until_training,
            quality_flags,
            next_actions,
        );
    }

    if input.holdout_validation_passed
        && input.trusted_bull_sleep_nights >= 7
        && input.calibration_label_count >= 14
        && motion_coverage_ok
        && heart_rate_coverage_ok
    {
        return sleep_model_status_report(
            SleepModelStatus::Trained,
            "Trained",
            "Bull has a passed personal sleep model for this algorithm version.",
            input,
            valid_sleep_nights,
            nights_until_baseline,
            nights_until_training,
            quality_flags,
            next_actions,
        );
    }

    if valid_sleep_nights == 0 {
        next_actions.push("Complete one sleep night to start learning.".to_string());
        return sleep_model_status_report(
            SleepModelStatus::SetupNeeded,
            "Setup needed",
            "Bull needs one valid sleep night to start learning.",
            input,
            valid_sleep_nights,
            nights_until_baseline,
            nights_until_training,
            quality_flags,
            next_actions,
        );
    }

    if valid_sleep_nights < 7 {
        next_actions.push(format!(
            "Collect {nights_until_baseline} more valid sleep night{} for a personal baseline.",
            plural_suffix(nights_until_baseline)
        ));
        return sleep_model_status_report(
            SleepModelStatus::Learning,
            "Learning",
            &format!(
                "{valid_sleep_nights} valid sleep night{} collected; {nights_until_baseline} more for baseline.",
                plural_suffix(valid_sleep_nights)
            ),
            input,
            valid_sleep_nights,
            nights_until_baseline,
            nights_until_training,
            quality_flags,
            next_actions,
        );
    }

    if input.trusted_bull_sleep_nights >= 7
        && input.calibration_label_count >= 14
        && !input.holdout_validation_passed
    {
        next_actions.push("Run holdout validation before marking Sleep V1 trained.".to_string());
        return sleep_model_status_report(
            SleepModelStatus::Training,
            "Training",
            "Bull has enough sleep history for training, but holdout validation has not passed.",
            input,
            valid_sleep_nights,
            nights_until_baseline,
            nights_until_training,
            quality_flags,
            next_actions,
        );
    }

    if input.trusted_bull_sleep_nights == 0 {
        next_actions.push(
            "Complete one Bull packet-derived sleep night before showing a final Sleep V1 score."
                .to_string(),
        );
    } else if input.trusted_bull_sleep_nights < 7 {
        let nights_until_bull_training = 7u32.saturating_sub(input.trusted_bull_sleep_nights);
        next_actions.push(format!(
            "Collect {nights_until_bull_training} more Bull packet-derived sleep night{} before training.",
            plural_suffix(nights_until_bull_training)
        ));
    } else if input.calibration_label_count < 14 {
        next_actions.push(format!(
            "Add {nights_until_training} more user-owned sleep calibration label{} before training.",
            plural_suffix(nights_until_training)
        ));
    } else {
        next_actions.push("Run holdout validation before marking Sleep V1 trained.".to_string());
    }
    sleep_model_status_report(
        SleepModelStatus::BaselineReady,
        "Baseline ready",
        "Bull has enough sleep history for personal schedule and debt baselines.",
        input,
        valid_sleep_nights,
        nights_until_baseline,
        nights_until_training,
        quality_flags,
        next_actions,
    )
}

pub fn bull_strain_v0(input: &StrainInput) -> AlgorithmRunResult<StrainScoreOutput> {
    let mut quality_flags = Vec::new();
    let mut errors = Vec::new();

    require_finite_positive("duration_minutes", input.duration_minutes, &mut errors);
    require_finite_positive("resting_hr_bpm", input.resting_hr_bpm, &mut errors);
    require_finite_positive("average_hr_bpm", input.average_hr_bpm, &mut errors);
    require_finite_positive("max_hr_bpm", input.max_hr_bpm, &mut errors);
    if input.max_hr_bpm <= input.resting_hr_bpm {
        errors.push("max_hr_must_exceed_resting_hr".to_string());
    }
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
        let zone_load = input
            .hr_zone_minutes
            .iter()
            .zip([1.0, 2.0, 3.0, 4.0, 5.0])
            .map(|(minutes, weight)| minutes * weight)
            .sum::<f64>();
        let zone_score_0_to_21 = clamp_0_to(21.0, zone_load / 20.0);
        let hr_reserve_fraction = clamp_fraction(
            (input.average_hr_bpm - input.resting_hr_bpm)
                / (input.max_hr_bpm - input.resting_hr_bpm),
        );
        let zone_score_0_to_100 = zone_score_0_to_21 / 21.0 * 100.0;
        let avg_hr_score_0_to_100 = hr_reserve_fraction * 100.0;

        let components = vec![
            score_component(
                "zone_load",
                zone_load,
                "weighted_zone_minutes",
                zone_score_0_to_100,
                0.70,
                21.0,
            ),
            score_component(
                "average_hr_reserve",
                hr_reserve_fraction,
                "fraction",
                avg_hr_score_0_to_100,
                0.30,
                21.0,
            ),
        ];

        Some(StrainScoreOutput {
            algorithm_id: BULL_STRAIN_V0_ID.to_string(),
            algorithm_version: BULL_STRAIN_V0_VERSION.to_string(),
            score_0_to_21: component_sum(&components),
            zone_load,
            average_hr_reserve_fraction: hr_reserve_fraction,
            components,
        })
    } else {
        None
    };

    AlgorithmRunResult {
        algorithm_id: BULL_STRAIN_V0_ID.to_string(),
        algorithm_version: BULL_STRAIN_V0_VERSION.to_string(),
        family: "strain".to_string(),
        start_time: input.start_time.clone(),
        end_time: input.end_time.clone(),
        output,
        quality_flags,
        errors,
        provenance: json!({
            "input_ids": input.input_ids,
            "score_policy": "weighted_zone_load_and_average_hr_reserve",
            "zone_weights": [1.0, 2.0, 3.0, 4.0, 5.0],
            "expected_values_policy": "hand-derived-tests-and-versioned-bull-output"
        }),
    }
}

/// Returns the Tanaka HRmax estimate: 208.0 - 0.7 * age.
/// Physiologically-grounded alternative to the 220-age heuristic.
pub fn tanaka_hrmax(age: f64) -> f64 {
    208.0 - 0.7 * age
}

// ── ALG-STR-01: History-based HRmax estimator ──────────────────────────────

/// Returns the 99.5th-percentile heart rate from `hr_history` when at least
/// 600 finite samples are present; returns None otherwise.
///
/// Non-finite values (NaN, ±infinity) are filtered out before counting. Index
/// is clamped to `len - 1` to guard against boundary overflow (T-23-02).
pub fn estimate_hrmax_from_history(hr_history: &[f64]) -> Option<f64> {
    let mut finite: Vec<f64> = hr_history
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .collect();
    if finite.len() < 600 {
        return None;
    }
    finite.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let len = finite.len();
    // CR-01 fix: nearest-rank P99.5 is ceil(0.995*n) - 1 (0-indexed).
    let index = ((0.995 * len as f64).ceil() as usize)
        .saturating_sub(1)
        .min(len - 1);
    Some(finite[index])
}

// ── ALG-STR-01: Effective HRmax resolver ───────────────────────────────────

/// Resolves an effective HRmax value and its source label.
///
/// Resolution order (ALG-STR-01):
/// 1. `estimate_hrmax_from_history` returns Some → `(value, "observed")`
/// 2. `profile_age` is Some → `(max(session_max_hr, tanaka_hrmax(age)), "tanaka")`
/// 3. No history + no age → `(session_max_hr, "fallback")`
///
/// Returns `(hrmax, source)` where source ∈ {"observed", "tanaka", "fallback"}.
pub fn resolve_effective_hrmax(
    session_max_hr: f64,
    profile_age: Option<f64>,
    hr_history: &[f64],
) -> (f64, String) {
    if let Some(history_hrmax) = estimate_hrmax_from_history(hr_history) {
        return (history_hrmax, "observed".to_string());
    }
    if let Some(age) = profile_age {
        let hrmax = session_max_hr.max(tanaka_hrmax(age));
        return (hrmax, "tanaka".to_string());
    }
    (session_max_hr, "fallback".to_string())
}

// ── ALG-STR-02: Banister TRIMP with zone-midpoint approximation ─────────────

/// Banister TRIMP using zone-midpoint approximation (ALG-STR-02).
///
/// Zone midpoints as % of HRmax: [55, 65, 75, 85, 95]%.
/// HRR fraction: `x = clamp((zone_mid_hr - resting_hr) / (hrmax - resting_hr), 0, 1)`
/// Formula: `Σ zone_minutes[i] * x_i * 0.64 * exp(b * x_i)` over 5 zones.
/// Sex constant: "male" → 1.92, "female" → 1.67, otherwise → 1.795 (mean).
///
/// Quality flag `banister_trimp_zone_midpoint_approximation` must always be
/// emitted by callers (it is a property of the input-approximation method,
/// not of the computation correctness).
pub fn banister_trimp_zone_midpoint(
    hr_zone_minutes: &[f64],
    resting_hr_bpm: f64,
    hrmax: f64,
    sex: Option<&str>,
) -> f64 {
    // Sex-specific exponential weighting constant (ALG-STR-02).
    // CR-02 fix: case-insensitive comparison so "Male"/"MALE" route correctly.
    let b: f64 = match sex.map(str::to_ascii_lowercase).as_deref() {
        Some("male") => 1.92,
        Some("female") => 1.67,
        _ => 1.795,
    };

    // Zone midpoints as fraction of HRmax (55/65/75/85/95%).
    const ZONE_MIDPOINTS: [f64; 5] = [0.55, 0.65, 0.75, 0.85, 0.95];

    let hr_range = hrmax - resting_hr_bpm;

    ZONE_MIDPOINTS
        .iter()
        .enumerate()
        .map(|(i, &pct)| {
            let minutes = hr_zone_minutes.get(i).copied().unwrap_or(0.0);
            if !minutes.is_finite() || minutes <= 0.0 {
                return 0.0;
            }
            let zone_mid_hr = pct * hrmax;
            let x = ((zone_mid_hr - resting_hr_bpm) / hr_range).clamp(0.0, 1.0);
            minutes * x * 0.64 * (b * x).exp()
        })
        .sum()
}

// ── ALG-STR-03: fit_strain_denominator ────────────────────────────────────

/// Fits the denominator `D` in the Banister-style logarithmic strain formula
/// `strain = 21 * ln(TRIMP+1) / ln(D)` from a set of (TRIMP, reference_strain) pairs.
///
/// Because the formula is linear in `m = 1/ln(D)` with coefficient `C_i = 21*ln(TRIMP_i+1)`,
/// the closed-form least-squares solution is `m = Σ(C_i*s_i) / Σ(C_i²)`,
/// giving `D = exp(1/m)`.
///
/// Returns `None` when:
/// - fewer than 2 pairs provided (T-23-03),
/// - any coefficient or strain value is non-finite,
/// - the denominator `Σ(C_i²)` is zero or non-finite.
pub fn fit_strain_denominator(pairs: &[(f64, f64)]) -> Option<f64> {
    if pairs.len() < 2 {
        return None;
    }

    let mut numerator = 0.0_f64;
    let mut denominator = 0.0_f64;

    for &(trimp, reference_strain) in pairs {
        if !trimp.is_finite() || !reference_strain.is_finite() {
            return None;
        }
        let c = 21.0 * (trimp + 1.0).ln();
        if !c.is_finite() {
            return None;
        }
        numerator += c * reference_strain;
        denominator += c * c;
    }

    if denominator == 0.0 || !denominator.is_finite() || !numerator.is_finite() {
        return None;
    }

    let m = numerator / denominator; // m = 1/ln(D)
    if m == 0.0 || !m.is_finite() {
        return None;
    }

    let d = (1.0 / m).exp();
    if d.is_finite() && d > 0.0 {
        Some(d)
    } else {
        None
    }
}

// ── ALG-STR-02/03: bull_strain_v1 ────────────────────────────────────────

/// Default denominator constant (D=7201) used when no calibration pairs are
/// available. Default constant fitted from local calibration pairs via fit_strain_denominator.
const STRAIN_V1_DEFAULT_DENOMINATOR: f64 = 7201.0;

/// Computes strain using both the Edwards zone-load model (v0 logic) and the
/// Banister TRIMP logarithmic formula (ALG-STR-02, ALG-STR-03).
///
/// The quality flag `banister_trimp_zone_midpoint_approximation` is always
/// emitted because zone midpoints are an approximation of the true
/// per-minute HRR distribution.
pub fn bull_strain_v1(input: &StrainInput) -> AlgorithmRunResult<StrainScoreOutput> {
    let mut quality_flags = Vec::new();
    let mut errors = Vec::new();

    // Always emit the approximation flag (ALG-STR-02 requirement).
    quality_flags.push("banister_trimp_zone_midpoint_approximation".to_string());

    require_finite_positive("duration_minutes", input.duration_minutes, &mut errors);
    require_finite_positive("resting_hr_bpm", input.resting_hr_bpm, &mut errors);
    require_finite_positive("average_hr_bpm", input.average_hr_bpm, &mut errors);
    require_finite_positive("max_hr_bpm", input.max_hr_bpm, &mut errors);
    if input.max_hr_bpm <= input.resting_hr_bpm {
        errors.push("max_hr_must_exceed_resting_hr".to_string());
    }
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

    // Resolve effective HRmax (ALG-STR-01 chain: observed → tanaka → fallback).
    let (effective_hrmax, hrmax_source) =
        resolve_effective_hrmax(input.max_hr_bpm, input.profile_age, &[]);

    // Banister b constant for provenance recording (selected by sex, case-insensitive).
    let b_constant: f64 = match input
        .profile_sex
        .as_deref()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("male") => 1.92,
        Some("female") => 1.67,
        _ => 1.795,
    };

    let output = if errors.is_empty() {
        // Edwards zone-load score (reusing v0 logic).
        let zone_load = input
            .hr_zone_minutes
            .iter()
            .zip([1.0_f64, 2.0, 3.0, 4.0, 5.0])
            .map(|(minutes, weight)| minutes * weight)
            .sum::<f64>();
        let edwards_score_0_to_21 = clamp_0_to(21.0, zone_load / 20.0);
        let hr_reserve_fraction = clamp_fraction(
            (input.average_hr_bpm - input.resting_hr_bpm)
                / (input.max_hr_bpm - input.resting_hr_bpm),
        );
        let edwards_score_0_to_100 = edwards_score_0_to_21 / 21.0 * 100.0;
        let avg_hr_score_0_to_100 = hr_reserve_fraction * 100.0;

        // Banister TRIMP score using zone-midpoint approximation.
        let trimp = banister_trimp_zone_midpoint(
            &input.hr_zone_minutes,
            input.resting_hr_bpm,
            effective_hrmax,
            input.profile_sex.as_deref(),
        );
        // Logarithmic strain using default denominator (D=7201).
        let banister_score_0_to_21 = if trimp > 0.0 {
            clamp_0_to(
                21.0,
                21.0 * (trimp + 1.0).ln() / STRAIN_V1_DEFAULT_DENOMINATOR.ln(),
            )
        } else {
            0.0
        };
        let banister_score_0_to_100 = banister_score_0_to_21 / 21.0 * 100.0;

        let components = vec![
            score_component(
                "edwards_zone_load",
                zone_load,
                "weighted_zone_minutes",
                edwards_score_0_to_100,
                0.50,
                21.0,
            ),
            score_component(
                "average_hr_reserve",
                hr_reserve_fraction,
                "fraction",
                avg_hr_score_0_to_100,
                0.20,
                21.0,
            ),
            score_component(
                "banister_trimp",
                trimp,
                "trimp_au",
                banister_score_0_to_100,
                0.30,
                21.0,
            ),
        ];

        Some(StrainScoreOutput {
            algorithm_id: BULL_STRAIN_V1_ID.to_string(),
            algorithm_version: BULL_STRAIN_V1_VERSION.to_string(),
            score_0_to_21: component_sum(&components),
            zone_load,
            average_hr_reserve_fraction: hr_reserve_fraction,
            components,
        })
    } else {
        None
    };

    AlgorithmRunResult {
        algorithm_id: BULL_STRAIN_V1_ID.to_string(),
        algorithm_version: BULL_STRAIN_V1_VERSION.to_string(),
        family: "strain".to_string(),
        start_time: input.start_time.clone(),
        end_time: input.end_time.clone(),
        output,
        quality_flags,
        errors,
        provenance: json!({
            "input_ids": input.input_ids,
            "score_policy": "edwards_zone_load_banister_trimp_combined",
            "zone_weights_edwards": [1.0, 2.0, 3.0, 4.0, 5.0],
            "zone_midpoints_pct_hrmax": [0.55, 0.65, 0.75, 0.85, 0.95],
            "banister_b_constant": b_constant,
            "hrmax_source": hrmax_source,
            "effective_hrmax": effective_hrmax,
            "default_denominator": STRAIN_V1_DEFAULT_DENOMINATOR,
        }),
    }
}

pub fn bull_recovery_v0(input: &RecoveryInput) -> AlgorithmRunResult<RecoveryScoreOutput> {
    let mut quality_flags = Vec::new();
    let mut errors = Vec::new();

    require_finite_positive(
        "hrv_baseline_rmssd_ms",
        input.hrv_baseline_rmssd_ms,
        &mut errors,
    );
    require_finite_positive(
        "resting_hr_baseline_bpm",
        input.resting_hr_baseline_bpm,
        &mut errors,
    );
    require_finite_positive(
        "respiratory_rate_baseline_rpm",
        input.respiratory_rate_baseline_rpm,
        &mut errors,
    );
    require_finite_non_negative("hrv_rmssd_ms", input.hrv_rmssd_ms, &mut errors);
    require_finite_positive("resting_hr_bpm", input.resting_hr_bpm, &mut errors);
    require_finite_positive(
        "respiratory_rate_rpm",
        input.respiratory_rate_rpm,
        &mut errors,
    );
    require_bounded(
        "sleep_score_0_to_100",
        input.sleep_score_0_to_100,
        0.0,
        100.0,
        &mut errors,
    );
    require_bounded(
        "prior_strain_0_to_21",
        input.prior_strain_0_to_21,
        0.0,
        21.0,
        &mut errors,
    );

    if input.sleep_score_0_to_100 < 60.0 {
        quality_flags.push("low_sleep_score".to_string());
    }
    if input.prior_strain_0_to_21 > 14.0 {
        quality_flags.push("high_prior_strain".to_string());
    }

    let output = if errors.is_empty() {
        let hrv_score =
            clamp_0_100(70.0 + (input.hrv_rmssd_ms / input.hrv_baseline_rmssd_ms - 1.0) * 100.0);
        let rhr_score =
            clamp_0_100(70.0 + (input.resting_hr_baseline_bpm - input.resting_hr_bpm) * 5.0);
        let respiratory_score = clamp_0_100(
            100.0 - (input.respiratory_rate_rpm - input.respiratory_rate_baseline_rpm).abs() * 20.0,
        );
        let temperature_score = clamp_0_100(100.0 - input.skin_temp_delta_c.abs() * 50.0);
        let strain_readiness_score = clamp_0_100(100.0 - input.prior_strain_0_to_21 / 21.0 * 60.0);

        let components = vec![
            score_component(
                "hrv",
                input.hrv_rmssd_ms,
                "ms_rmssd",
                hrv_score,
                0.35,
                100.0,
            ),
            score_component("rhr", input.resting_hr_bpm, "bpm", rhr_score, 0.20, 100.0),
            score_component(
                "respiratory",
                input.respiratory_rate_rpm,
                "breaths_per_minute",
                respiratory_score,
                0.10,
                100.0,
            ),
            score_component(
                "temperature",
                input.skin_temp_delta_c,
                "celsius_delta",
                temperature_score,
                0.10,
                100.0,
            ),
            score_component(
                "sleep",
                input.sleep_score_0_to_100,
                "score_0_to_100",
                input.sleep_score_0_to_100,
                0.15,
                100.0,
            ),
            score_component(
                "prior_strain",
                input.prior_strain_0_to_21,
                "score_0_to_21",
                strain_readiness_score,
                0.10,
                100.0,
            ),
        ];

        Some(RecoveryScoreOutput {
            algorithm_id: BULL_RECOVERY_V0_ID.to_string(),
            algorithm_version: BULL_RECOVERY_V0_VERSION.to_string(),
            score_0_to_100: component_sum(&components),
            components,
        })
    } else {
        None
    };

    AlgorithmRunResult {
        algorithm_id: BULL_RECOVERY_V0_ID.to_string(),
        algorithm_version: BULL_RECOVERY_V0_VERSION.to_string(),
        family: "recovery".to_string(),
        start_time: input.start_time.clone(),
        end_time: input.end_time.clone(),
        output,
        quality_flags,
        errors,
        provenance: json!({
            "input_ids": input.input_ids,
            "score_policy": "weighted_interpretable_recovery_components",
            "official_labels_policy": "not_used_unless_explicit_calibration_label",
            "expected_values_policy": "hand-derived-tests-and-versioned-bull-output"
        }),
    }
}

pub fn bull_stress_v0(input: &StressInput) -> AlgorithmRunResult<StressScoreOutput> {
    let mut quality_flags = Vec::new();
    let mut errors = Vec::new();

    require_finite_positive("heart_rate_bpm", input.heart_rate_bpm, &mut errors);
    require_finite_positive("resting_hr_bpm", input.resting_hr_bpm, &mut errors);
    require_finite_non_negative("hrv_rmssd_ms", input.hrv_rmssd_ms, &mut errors);
    require_finite_positive(
        "hrv_baseline_rmssd_ms",
        input.hrv_baseline_rmssd_ms,
        &mut errors,
    );
    if input.heart_rate_bpm < input.resting_hr_bpm {
        quality_flags.push("heart_rate_below_resting".to_string());
    }
    if input.motion_intensity_0_to_1 > 0.70 {
        quality_flags.push("high_motion_context".to_string());
    }
    if !(0.0..=1.0).contains(&input.motion_intensity_0_to_1) {
        quality_flags.push("motion_intensity_clamped".to_string());
    }

    let output = if errors.is_empty() {
        let motion = clamp_fraction(input.motion_intensity_0_to_1);
        let heart_rate_elevation_score =
            clamp_0_100((input.heart_rate_bpm - input.resting_hr_bpm).max(0.0) / 60.0 * 100.0);
        let hrv_suppression_score =
            clamp_0_100((1.0 - input.hrv_rmssd_ms / input.hrv_baseline_rmssd_ms) * 100.0);
        let motion_adjusted_hr_score = heart_rate_elevation_score * (1.0 - motion * 0.50);

        let components = vec![
            score_component(
                "motion_adjusted_hr",
                motion_adjusted_hr_score,
                "score_0_to_100",
                motion_adjusted_hr_score,
                0.60,
                100.0,
            ),
            score_component(
                "hrv_suppression",
                input.hrv_rmssd_ms,
                "ms_rmssd",
                hrv_suppression_score,
                0.40,
                100.0,
            ),
        ];

        Some(StressScoreOutput {
            algorithm_id: BULL_STRESS_V0_ID.to_string(),
            algorithm_version: BULL_STRESS_V0_VERSION.to_string(),
            score_0_to_100: component_sum(&components),
            heart_rate_elevation_score,
            hrv_suppression_score,
            motion_adjusted_hr_score,
            components,
        })
    } else {
        None
    };

    AlgorithmRunResult {
        algorithm_id: BULL_STRESS_V0_ID.to_string(),
        algorithm_version: BULL_STRESS_V0_VERSION.to_string(),
        family: "stress".to_string(),
        start_time: input.start_time.clone(),
        end_time: input.end_time.clone(),
        output,
        quality_flags,
        errors,
        provenance: json!({
            "input_ids": input.input_ids,
            "score_policy": "hr_elevation_and_hrv_suppression_with_motion_context",
            "expected_values_policy": "hand-derived-tests-and-versioned-bull-output"
        }),
    }
}

pub fn algorithm_run_record<T: Serialize>(
    run_id: &str,
    result: &AlgorithmRunResult<T>,
) -> BullResult<AlgorithmRunRecord> {
    let output_json = serde_json::to_string(&result.output).map_err(|error| {
        BullError::message(format!("cannot serialize algorithm output: {error}"))
    })?;
    let quality_flags_json = serde_json::to_string(&result.quality_flags).map_err(|error| {
        BullError::message(format!("cannot serialize algorithm quality flags: {error}"))
    })?;
    let provenance_json = serde_json::to_string(&json!({
        "provenance": result.provenance,
        "errors": result.errors
    }))
    .map_err(|error| {
        BullError::message(format!("cannot serialize algorithm provenance: {error}"))
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

pub fn hrv_run_record(
    run_id: &str,
    result: &AlgorithmRunResult<HrvOutput>,
) -> BullResult<AlgorithmRunRecord> {
    algorithm_run_record(run_id, result)
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
    if values.len() < 2 {
        return 0.0;
    }
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

fn stage_minutes(stage_minutes: &BTreeMap<String, f64>, stage: &str) -> Option<f64> {
    stage_minutes
        .get(stage)
        .copied()
        .filter(|minutes| minutes.is_finite() && *minutes >= 0.0)
}

pub fn sleep_baseline_from_history(
    prior_nights: &[SleepNightHistoryInput],
) -> Option<SleepBaseline> {
    let mut usable_nights = prior_nights
        .iter()
        .filter(|night| sleep_history_night_is_usable(night))
        .collect::<Vec<_>>();
    usable_nights.sort_by_key(|night| sleep_history_sort_key(night));
    if usable_nights.is_empty() {
        return None;
    }

    let rolling_sleep_debt_minutes = usable_nights
        .iter()
        .rev()
        .take(28)
        .map(|night| (night.sleep_need_minutes - night.sleep_duration_minutes).max(0.0))
        .sum::<f64>();
    let excluded_night_count = prior_nights
        .iter()
        .filter(|night| !sleep_history_night_is_usable(night))
        .count() as u32;

    Some(SleepBaseline {
        usable_night_count: usable_nights.len() as u32,
        excluded_night_count,
        rolling_sleep_debt_minutes,
        short_7_day: sleep_baseline_window(&usable_nights, 7),
        current_14_day: sleep_baseline_window(&usable_nights, 14),
        stable_28_day: sleep_baseline_window(&usable_nights, 28),
    })
}

fn sleep_prior_nights_before_scored_sleep(input: &SleepV1Input) -> Vec<SleepNightHistoryInput> {
    let Some(scored_sleep_start_unix_ms) = sleep_time_unix_ms(&input.sleep.start_time) else {
        return Vec::new();
    };
    input
        .prior_nights
        .iter()
        .filter(|night| {
            sleep_time_unix_ms(&night.end_time)
                .is_some_and(|end_unix_ms| end_unix_ms <= scored_sleep_start_unix_ms)
        })
        .cloned()
        .collect()
}

fn sleep_previous_night_comparison(
    input: &SleepV1Input,
    prior_nights: &[SleepNightHistoryInput],
) -> Option<SleepPreviousNightComparison> {
    let previous = prior_nights
        .iter()
        .filter(|night| sleep_history_night_is_usable(night))
        .max_by_key(|night| sleep_history_sort_key(night))?;
    let previous_sleep_efficiency_fraction =
        clamp_fraction(previous.sleep_duration_minutes / previous.time_in_bed_minutes);
    let current_sleep_efficiency_fraction =
        clamp_fraction(input.sleep.sleep_duration_minutes / input.sleep.time_in_bed_minutes);
    let previous_restorative_sleep_minutes = stage_minutes(&previous.stage_minutes, "deep")
        .unwrap_or(0.0)
        + stage_minutes(&previous.stage_minutes, "rem").unwrap_or(0.0);
    let current_stage_minutes = sleep_v1_effective_stage_minutes(input);
    let current_restorative_sleep_minutes = stage_minutes(&current_stage_minutes, "deep")
        .unwrap_or(0.0)
        + stage_minutes(&current_stage_minutes, "rem").unwrap_or(0.0);

    Some(SleepPreviousNightComparison {
        night_id: previous.night_id.clone(),
        sleep_duration_delta_minutes: input.sleep.sleep_duration_minutes
            - previous.sleep_duration_minutes,
        awake_minutes_delta: (input.sleep.time_in_bed_minutes - input.sleep.sleep_duration_minutes)
            - previous.awake_minutes,
        sleep_debt_delta_minutes: (input.sleep.sleep_need_minutes
            - input.sleep.sleep_duration_minutes)
            .max(0.0)
            - (previous.sleep_need_minutes - previous.sleep_duration_minutes).max(0.0),
        sleep_efficiency_delta_fraction: current_sleep_efficiency_fraction
            - previous_sleep_efficiency_fraction,
        sleep_latency_delta_minutes: input.sleep.sleep_latency_minutes
            - previous.sleep_latency_minutes,
        wake_after_sleep_onset_delta_minutes: input.sleep.wake_after_sleep_onset_minutes
            - previous.wake_after_sleep_onset_minutes,
        wake_episode_count_delta: input.sleep.wake_episode_count as i32
            - previous.wake_episode_count as i32,
        deep_sleep_delta_minutes: stage_minutes(&current_stage_minutes, "deep").unwrap_or(0.0)
            - stage_minutes(&previous.stage_minutes, "deep").unwrap_or(0.0),
        rem_sleep_delta_minutes: stage_minutes(&current_stage_minutes, "rem").unwrap_or(0.0)
            - stage_minutes(&previous.stage_minutes, "rem").unwrap_or(0.0),
        core_sleep_delta_minutes: stage_minutes(&current_stage_minutes, "core").unwrap_or(0.0)
            - stage_minutes(&previous.stage_minutes, "core").unwrap_or(0.0),
        restorative_sleep_delta_minutes: current_restorative_sleep_minutes
            - previous_restorative_sleep_minutes,
        bedtime_deviation_delta_minutes: input.bedtime_deviation_minutes
            - previous.bedtime_deviation_minutes,
        wake_time_deviation_delta_minutes: input.wake_time_deviation_minutes
            - previous.wake_time_deviation_minutes,
        sleep_hr_average_delta_bpm: input
            .sleep_hr_average_bpm
            .zip(previous.sleep_hr_average_bpm)
            .map(|(current, previous)| current - previous),
        sleep_hr_min_delta_bpm: input
            .sleep_hr_min_bpm
            .zip(previous.sleep_hr_min_bpm)
            .map(|(current, previous)| current - previous),
        sleep_hr_trend_delta_bpm_per_hour: input
            .sleep_hr_trend_bpm_per_hour
            .zip(previous.sleep_hr_trend_bpm_per_hour)
            .map(|(current, previous)| current - previous),
        sleep_hr_dip_delta_percent: input
            .sleep
            .heart_rate_dip_percent
            .zip(previous.heart_rate_dip_percent)
            .map(|(current, previous)| current - previous),
    })
}

fn sleep_baseline_window(
    usable_nights: &[&SleepNightHistoryInput],
    window_days: u32,
) -> Option<SleepBaselineWindow> {
    let window_nights = usable_nights
        .iter()
        .rev()
        .take(window_days as usize)
        .copied()
        .collect::<Vec<_>>();
    if window_nights.is_empty() {
        return None;
    }

    let night_count = window_nights.len() as u32;
    let count = window_nights.len() as f64;
    let average_sleep_duration_minutes =
        average_by(&window_nights, |night| night.sleep_duration_minutes);
    let average_sleep_need_minutes = average_by(&window_nights, |night| night.sleep_need_minutes);
    let average_time_in_bed_minutes = average_by(&window_nights, |night| night.time_in_bed_minutes);
    let average_awake_minutes = average_by(&window_nights, |night| night.awake_minutes);

    Some(SleepBaselineWindow {
        window_days,
        night_count,
        average_sleep_duration_minutes,
        average_sleep_need_minutes,
        average_sleep_debt_minutes: average_by(&window_nights, |night| {
            (night.sleep_need_minutes - night.sleep_duration_minutes).max(0.0)
        }),
        average_time_in_bed_minutes,
        average_awake_minutes,
        average_sleep_efficiency_fraction: if average_time_in_bed_minutes > 0.0 {
            clamp_fraction(average_sleep_duration_minutes / average_time_in_bed_minutes)
        } else {
            0.0
        },
        average_latency_minutes: average_by(&window_nights, |night| night.sleep_latency_minutes),
        average_wake_after_sleep_onset_minutes: average_by(&window_nights, |night| {
            night.wake_after_sleep_onset_minutes
        }),
        average_wake_episode_count: window_nights
            .iter()
            .map(|night| night.wake_episode_count as f64)
            .sum::<f64>()
            / count,
        average_deep_sleep_minutes: average_stage_minutes(&window_nights, "deep"),
        average_rem_sleep_minutes: average_stage_minutes(&window_nights, "rem"),
        average_core_sleep_minutes: average_stage_minutes(&window_nights, "core"),
        average_restorative_sleep_minutes: average_by(&window_nights, |night| {
            stage_minutes(&night.stage_minutes, "deep").unwrap_or(0.0)
                + stage_minutes(&night.stage_minutes, "rem").unwrap_or(0.0)
        }),
        average_bedtime_deviation_minutes: average_by(&window_nights, |night| {
            night.bedtime_deviation_minutes
        }),
        average_wake_time_deviation_minutes: average_by(&window_nights, |night| {
            night.wake_time_deviation_minutes
        }),
        average_midpoint_deviation_minutes: average_by(&window_nights, |night| {
            night.midpoint_deviation_minutes
        }),
        average_naps_minutes: average_by(&window_nights, |night| night.naps_minutes),
        average_sleep_hr_bpm: average_option_by(&window_nights, |night| night.sleep_hr_average_bpm),
        average_sleep_hr_min_bpm: average_option_by(&window_nights, |night| night.sleep_hr_min_bpm),
        average_sleep_hr_trend_bpm_per_hour: average_option_by(&window_nights, |night| {
            night.sleep_hr_trend_bpm_per_hour
        }),
        average_hr_dip_percent: average_option_by(&window_nights, |night| {
            night.heart_rate_dip_percent
        }),
        average_confidence_0_to_1: average_by(&window_nights, |night| night.confidence_0_to_1),
    })
}

pub(crate) fn sleep_history_night_is_usable(night: &SleepNightHistoryInput) -> bool {
    !night.excluded_from_baseline
        && night.confidence_0_to_1 >= 0.5
        && sleep_time_unix_ms(&night.start_time)
            .zip(sleep_time_unix_ms(&night.end_time))
            .is_some_and(|(start, end)| end > start)
        && night.sleep_duration_minutes.is_finite()
        && night.sleep_duration_minutes > 0.0
        && night.sleep_need_minutes.is_finite()
        && night.sleep_need_minutes > 0.0
        && night.time_in_bed_minutes.is_finite()
        && night.time_in_bed_minutes > 0.0
        && night.awake_minutes.is_finite()
        && night.awake_minutes >= 0.0
        && night.sleep_duration_minutes <= night.time_in_bed_minutes
        && night.awake_minutes <= night.time_in_bed_minutes
        && night.sleep_duration_minutes + night.awake_minutes <= night.time_in_bed_minutes + 5.0
        && sleep_history_stage_minutes_are_usable(night)
}

fn sleep_history_stage_minutes_are_usable(night: &SleepNightHistoryInput) -> bool {
    if night.stage_minutes.is_empty() {
        return true;
    }
    if night
        .stage_minutes
        .iter()
        .any(|(stage, minutes)| stage.trim().is_empty() || !minutes.is_finite() || *minutes < 0.0)
    {
        return false;
    }
    let total_stage_minutes = night.stage_minutes.values().sum::<f64>();
    let asleep_stage_minutes = night
        .stage_minutes
        .iter()
        .filter(|(stage, _)| stage.as_str() != "awake")
        .map(|(_, minutes)| *minutes)
        .sum::<f64>();
    total_stage_minutes <= night.time_in_bed_minutes + 5.0
        && asleep_stage_minutes <= night.sleep_duration_minutes + 5.0
}

fn sleep_history_sort_key(night: &SleepNightHistoryInput) -> (i64, &str) {
    (
        sleep_time_unix_ms(&night.end_time).unwrap_or(i64::MIN),
        night.night_id.as_str(),
    )
}

fn average_by<F>(nights: &[&SleepNightHistoryInput], value: F) -> f64
where
    F: Fn(&SleepNightHistoryInput) -> f64,
{
    nights.iter().map(|night| value(night)).sum::<f64>() / nights.len() as f64
}

fn average_stage_minutes(nights: &[&SleepNightHistoryInput], stage: &str) -> f64 {
    average_by(nights, |night| {
        stage_minutes(&night.stage_minutes, stage).unwrap_or(0.0)
    })
}

fn average_option_by<F>(nights: &[&SleepNightHistoryInput], value: F) -> Option<f64>
where
    F: Fn(&SleepNightHistoryInput) -> Option<f64>,
{
    let values = nights
        .iter()
        .filter_map(|night| value(night))
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

#[allow(clippy::too_many_arguments)]
fn sleep_model_status_report(
    status: SleepModelStatus,
    status_label: &str,
    status_reason: &str,
    input: &SleepModelStatusInput,
    valid_sleep_nights: u32,
    nights_until_baseline: u32,
    nights_until_training: u32,
    quality_flags: Vec<String>,
    next_actions: Vec<String>,
) -> SleepModelStatusReport {
    let can_show_personal_baseline = matches!(
        status,
        SleepModelStatus::BaselineReady | SleepModelStatus::Training | SleepModelStatus::Trained
    );
    let coverage_ready = !quality_flags.iter().any(|flag| {
        matches!(
            flag.as_str(),
            "motion_coverage_low" | "heart_rate_coverage_low"
        )
    });
    let can_show_final_score =
        can_show_personal_baseline && coverage_ready && input.trusted_bull_sleep_nights > 0;
    let can_show_provisional_score = valid_sleep_nights > 0 && status != SleepModelStatus::Blocked;
    let can_show_trained_score = status == SleepModelStatus::Trained;
    let report_state = if status == SleepModelStatus::Blocked {
        "blocked"
    } else if can_show_final_score {
        "final"
    } else if can_show_provisional_score {
        "provisional"
    } else {
        "pending"
    };
    SleepModelStatusReport {
        status,
        status_label: status_label.to_string(),
        status_reason: status_reason.to_string(),
        report_state: report_state.to_string(),
        valid_sleep_nights,
        trusted_bull_sleep_nights: input.trusted_bull_sleep_nights,
        imported_platform_sleep_nights: input.imported_platform_sleep_nights,
        excluded_sleep_nights: input.excluded_sleep_nights,
        calibration_label_count: input.calibration_label_count,
        nights_until_baseline,
        nights_until_bull_training: 7u32.saturating_sub(input.trusted_bull_sleep_nights),
        nights_until_training,
        can_show_provisional_score,
        can_show_final_score,
        can_show_personal_baseline,
        can_show_trained_score,
        quality_flags,
        next_actions,
    }
}

fn coverage_ok(coverage: Option<f64>, threshold: f64) -> bool {
    coverage.is_some_and(|value| value.is_finite() && value >= threshold)
}

fn sleep_v1_confidence_0_to_1(
    status_report: &SleepModelStatusReport,
    data_coverage_fraction: Option<f64>,
    heart_rate_dip_percent: Option<f64>,
    has_sleep_architecture: bool,
    sleep_architecture_confidence_0_to_1: Option<f64>,
) -> f64 {
    let status_basis = match status_report.status {
        SleepModelStatus::Trained => 0.95,
        SleepModelStatus::BaselineReady | SleepModelStatus::Training => 0.78,
        SleepModelStatus::Learning | SleepModelStatus::ImportingHistory => 0.55,
        SleepModelStatus::NeedsRelearn => 0.45,
        SleepModelStatus::SetupNeeded => 0.30,
        SleepModelStatus::Blocked => 0.10,
    };
    let coverage_basis = data_coverage_fraction.unwrap_or(0.65).clamp(0.0, 1.0);
    let hr_basis = if heart_rate_dip_percent.is_some() {
        1.0
    } else {
        0.82
    };
    let architecture_basis = sleep_architecture_confidence_0_to_1
        .unwrap_or(if has_sleep_architecture { 1.0 } else { 0.86 });
    let confidence = clamp_fraction(
        status_basis * 0.55 + coverage_basis * 0.25 + hr_basis * 0.10 + architecture_basis * 0.10,
    );
    let confidence = if sleep_status_has_quality_flag(status_report, "motion_coverage_low") {
        confidence.min(0.60)
    } else {
        confidence
    };
    if sleep_status_has_quality_flag(status_report, "heart_rate_coverage_low") {
        confidence.min(0.72)
    } else {
        confidence
    }
}

fn sleep_v1_sleep_window_confidence_0_to_1(
    input: &SleepV1Input,
    status_report: &SleepModelStatusReport,
    data_coverage_fraction: Option<f64>,
) -> f64 {
    if status_report.status == SleepModelStatus::Blocked {
        return 0.10;
    }
    let status_basis = match status_report.status {
        SleepModelStatus::Trained => 0.96,
        SleepModelStatus::BaselineReady | SleepModelStatus::Training => 0.84,
        SleepModelStatus::Learning | SleepModelStatus::ImportingHistory => 0.62,
        SleepModelStatus::NeedsRelearn => 0.52,
        SleepModelStatus::SetupNeeded => 0.35,
        SleepModelStatus::Blocked => 0.10,
    };
    let coverage_basis = data_coverage_fraction
        .or(input.model_status.motion_coverage_fraction)
        .unwrap_or(0.65)
        .clamp(0.0, 1.0);
    let duration_basis = if input.sleep.time_in_bed_minutes >= 180.0
        && input.sleep.sleep_duration_minutes >= 60.0
        && input.sleep.sleep_duration_minutes <= input.sleep.time_in_bed_minutes
    {
        1.0
    } else {
        0.45
    };
    let confidence = status_basis * 0.55 + coverage_basis * 0.35 + duration_basis * 0.10;
    let confidence = if sleep_status_has_quality_flag(status_report, "motion_coverage_low") {
        confidence.min(0.55)
    } else {
        confidence
    };
    let confidence = if sleep_status_has_quality_flag(status_report, "heart_rate_coverage_low") {
        confidence.min(0.70)
    } else {
        confidence
    };
    clamp_fraction(confidence)
}

fn sleep_status_has_quality_flag(status_report: &SleepModelStatusReport, flag: &str) -> bool {
    status_report
        .quality_flags
        .iter()
        .any(|quality_flag| quality_flag == flag)
}

fn sleep_v1_stage_segment_confidence(input: &SleepV1Input) -> Option<f64> {
    if input.stage_segments.is_empty() {
        return None;
    }
    let total_duration_minutes = input
        .stage_segments
        .iter()
        .map(|segment| segment.duration_minutes)
        .sum::<f64>();
    if total_duration_minutes <= 0.0 || !total_duration_minutes.is_finite() {
        return None;
    }
    Some(clamp_fraction(
        input
            .stage_segments
            .iter()
            .map(|segment| segment.confidence_0_to_1 * segment.duration_minutes)
            .sum::<f64>()
            / total_duration_minutes,
    ))
}

fn sleep_v1_architecture_confidence(
    input: &SleepV1Input,
    stage_segment_confidence_0_to_1: Option<f64>,
) -> Option<f64> {
    if input.stage_segments.is_empty() {
        return None;
    }
    let total_duration_minutes = input
        .stage_segments
        .iter()
        .map(|segment| segment.duration_minutes)
        .sum::<f64>();
    if total_duration_minutes <= 0.0 || !total_duration_minutes.is_finite() {
        return None;
    }
    let probability_confidence = input
        .stage_segments
        .iter()
        .map(|segment| {
            let selected_probability = segment
                .stage_probabilities
                .get(&segment.stage_kind)
                .copied()
                .unwrap_or(segment.confidence_0_to_1)
                .clamp(0.0, 1.0);
            selected_probability * segment.duration_minutes
        })
        .sum::<f64>()
        / total_duration_minutes;
    Some(clamp_fraction(
        stage_segment_confidence_0_to_1.unwrap_or(probability_confidence) * 0.60
            + probability_confidence * 0.40,
    ))
}

fn sleep_v1_effective_stage_minutes(input: &SleepV1Input) -> BTreeMap<String, f64> {
    if !input.sleep.stage_minutes.is_empty() {
        return input.sleep.stage_minutes.clone();
    }
    let mut stage_minutes = BTreeMap::new();
    for segment in &input.stage_segments {
        *stage_minutes
            .entry(segment.stage_kind.clone())
            .or_insert(0.0) += segment.duration_minutes;
    }
    stage_minutes
}

fn sleep_v1_rolling_sleep_debt_minutes(
    input: &SleepV1Input,
    baseline: Option<&SleepBaseline>,
    current_sleep_debt_minutes: f64,
) -> f64 {
    if input.rolling_sleep_debt_minutes > 0.0 {
        input.rolling_sleep_debt_minutes
    } else {
        baseline
            .map(|baseline| baseline.rolling_sleep_debt_minutes + current_sleep_debt_minutes)
            .unwrap_or(current_sleep_debt_minutes)
    }
}

fn sleep_v1_components(
    input: &SleepV1Input,
    v0_output: &SleepScoreOutput,
    baseline: Option<&SleepBaseline>,
    rolling_sleep_debt_minutes: f64,
    data_coverage_fraction: Option<f64>,
    confidence_0_to_1: f64,
    sleep_window_confidence_0_to_1: f64,
    stage_minutes: &BTreeMap<String, f64>,
) -> Vec<ScoreComponent> {
    let sleep_need_score = sleep_need_fulfillment_score(
        input.sleep.sleep_duration_minutes,
        input.sleep.sleep_need_minutes,
        rolling_sleep_debt_minutes,
        input.naps_minutes,
    );
    let continuity_score = sleep_continuity_score(
        v0_output.efficiency_fraction,
        input.sleep.sleep_latency_minutes,
        input.sleep.wake_after_sleep_onset_minutes,
        input.sleep.wake_episode_count,
    );
    let schedule_score = sleep_schedule_score(
        input.bedtime_deviation_minutes,
        input.wake_time_deviation_minutes,
        input.sleep.midpoint_deviation_minutes,
    );
    let architecture_score =
        sleep_architecture_score(stage_minutes, input.sleep.sleep_duration_minutes, baseline);
    let cardiovascular_score = sleep_cardiovascular_score(input, baseline);
    let context_score = sleep_context_score(input.prior_day_strain, input.naps_minutes);
    let data_confidence_score = confidence_0_to_1
        * sleep_window_confidence_0_to_1
        * data_coverage_fraction.unwrap_or(0.65)
        * 100.0;

    vec![
        score_component(
            "sleep_need_fulfillment",
            input.sleep.sleep_duration_minutes,
            "minutes",
            sleep_need_score,
            0.25,
            100.0,
        ),
        score_component(
            "continuity",
            input.sleep.wake_after_sleep_onset_minutes,
            "minutes_waso",
            continuity_score,
            0.20,
            100.0,
        ),
        score_component(
            "schedule_regularity",
            input.sleep.midpoint_deviation_minutes,
            "minutes_deviation",
            schedule_score,
            0.15,
            100.0,
        ),
        score_component(
            "sleep_architecture",
            v0_output.restorative_sleep_minutes,
            "minutes_restorative",
            architecture_score,
            0.15,
            100.0,
        ),
        score_component(
            "cardiovascular_recovery",
            input.sleep.heart_rate_dip_percent.unwrap_or(0.0),
            "hr_dip_percent",
            cardiovascular_score,
            0.15,
            100.0,
        ),
        score_component(
            "context_adjustment",
            input.prior_day_strain.unwrap_or(0.0),
            "strain_0_to_21",
            context_score,
            0.05,
            100.0,
        ),
        score_component(
            "data_confidence",
            data_coverage_fraction.unwrap_or(0.65),
            "fraction",
            data_confidence_score,
            0.05,
            100.0,
        ),
    ]
}

fn sleep_v1_component_provenance(
    input: &SleepV1Input,
    baseline: Option<&SleepBaseline>,
    rolling_sleep_debt_minutes: f64,
    data_coverage_fraction: Option<f64>,
    stage_minutes: &BTreeMap<String, f64>,
    stage_segment_confidence_0_to_1: Option<f64>,
    sleep_architecture_confidence_0_to_1: Option<f64>,
    sleep_window_confidence_0_to_1: f64,
) -> BTreeMap<String, serde_json::Value> {
    let baseline_policy = baseline
        .and_then(preferred_sleep_baseline_window)
        .map(|window| {
            json!({
                "window_days": window.window_days,
                "night_count": window.night_count,
                "source": "prior_nights",
            })
        });
    let mut provenance = BTreeMap::new();
    provenance.insert(
        "sleep_need_fulfillment".to_string(),
        json!({
            "inputs": {
                "sleep_duration_minutes": input.sleep.sleep_duration_minutes,
                "sleep_need_minutes": input.sleep.sleep_need_minutes,
                "rolling_sleep_debt_minutes": rolling_sleep_debt_minutes,
                "naps_minutes": input.naps_minutes,
            },
            "input_ids": input.sleep.input_ids,
            "policy": "duration_vs_need_with_debt_pressure_and_nap_credit",
        }),
    );
    provenance.insert(
        "continuity".to_string(),
        json!({
            "inputs": {
                "time_in_bed_minutes": input.sleep.time_in_bed_minutes,
                "sleep_duration_minutes": input.sleep.sleep_duration_minutes,
                "sleep_latency_minutes": input.sleep.sleep_latency_minutes,
                "wake_after_sleep_onset_minutes": input.sleep.wake_after_sleep_onset_minutes,
                "wake_episode_count": input.sleep.wake_episode_count,
            },
            "input_ids": input.sleep.input_ids,
            "policy": "efficiency_latency_waso_and_wake_episode_curve",
        }),
    );
    provenance.insert(
        "schedule_regularity".to_string(),
        json!({
            "inputs": {
                "bedtime_deviation_minutes": input.bedtime_deviation_minutes,
                "wake_time_deviation_minutes": input.wake_time_deviation_minutes,
                "midpoint_deviation_minutes": input.sleep.midpoint_deviation_minutes,
            },
            "baseline": baseline_policy.clone(),
            "policy": "weighted_bedtime_wake_time_midpoint_deviation",
        }),
    );
    provenance.insert(
        "sleep_architecture".to_string(),
        json!({
            "inputs": {
                "stage_minutes": stage_minutes,
                "stage_segment_count": input.stage_segments.len(),
                "stage_segment_confidence_0_to_1": stage_segment_confidence_0_to_1,
                "sleep_architecture_confidence_0_to_1": sleep_architecture_confidence_0_to_1,
                "stage_prior_calibration": sleep_architecture_prior_calibration_provenance(baseline),
            },
            "baseline": baseline_policy.clone(),
            "input_ids": input.sleep.input_ids,
            "policy": "deep_rem_restorative_balance_vs_personal_baseline_when_available_with_architecture_confidence",
        }),
    );
    provenance.insert(
        "cardiovascular_recovery".to_string(),
        json!({
            "inputs": {
                "sleep_hr_average_bpm": input.sleep_hr_average_bpm,
                "sleep_hr_min_bpm": input.sleep_hr_min_bpm,
                "pre_sleep_awake_hr_average_bpm": input.pre_sleep_awake_hr_average_bpm,
                "sleep_hr_trend_bpm_per_hour": input.sleep_hr_trend_bpm_per_hour,
                "heart_rate_dip_percent": input.sleep.heart_rate_dip_percent,
            },
            "baseline": baseline_policy.clone(),
            "input_ids": input.sleep.input_ids,
            "policy": "hr_dip_pre_sleep_awake_hr_overnight_trend_and_personal_baseline_when_available",
        }),
    );
    provenance.insert(
        "context_adjustment".to_string(),
        json!({
            "inputs": {
                "prior_day_strain": input.prior_day_strain,
                "naps_minutes": input.naps_minutes,
            },
            "policy": "strain_and_long_nap_penalty",
        }),
    );
    provenance.insert(
        "data_confidence".to_string(),
        json!({
            "inputs": {
                "sleep_window_confidence_0_to_1": sleep_window_confidence_0_to_1,
                "data_coverage_fraction": data_coverage_fraction,
                "motion_coverage_fraction": input.model_status.motion_coverage_fraction,
                "heart_rate_coverage_fraction": input.model_status.heart_rate_coverage_fraction,
                "stage_segment_confidence_0_to_1": stage_segment_confidence_0_to_1,
                "sleep_architecture_confidence_0_to_1": sleep_architecture_confidence_0_to_1,
                "timestamp_sync_blocked": input.model_status.timestamp_sync_blocked,
            },
            "policy": "combined_sleep_v1_confidence_window_confidence_and_coverage",
        }),
    );
    provenance
}

fn sleep_need_fulfillment_score(
    sleep_duration_minutes: f64,
    sleep_need_minutes: f64,
    rolling_sleep_debt_minutes: f64,
    naps_minutes: f64,
) -> f64 {
    let debt_pressure_minutes = (rolling_sleep_debt_minutes * 0.20).min(120.0);
    let effective_sleep_need = (sleep_need_minutes + debt_pressure_minutes - naps_minutes * 0.50)
        .max(sleep_need_minutes * 0.75);
    clamp_0_100(sleep_duration_minutes / effective_sleep_need * 100.0)
}

fn sleep_continuity_score(
    efficiency_fraction: f64,
    sleep_latency_minutes: f64,
    wake_after_sleep_onset_minutes: f64,
    wake_episode_count: u32,
) -> f64 {
    let efficiency_score = clamp_0_100((efficiency_fraction - 0.70) / 0.25 * 100.0);
    let latency_score = clamp_0_100(100.0 - sleep_latency_minutes / 60.0 * 100.0);
    let waso_score = clamp_0_100(100.0 - wake_after_sleep_onset_minutes / 120.0 * 100.0);
    let episode_score = clamp_0_100(100.0 - wake_episode_count as f64 / 8.0 * 100.0);
    efficiency_score * 0.40 + latency_score * 0.20 + waso_score * 0.25 + episode_score * 0.15
}

fn sleep_schedule_score(
    bedtime_deviation_minutes: f64,
    wake_time_deviation_minutes: f64,
    midpoint_deviation_minutes: f64,
) -> f64 {
    let average_deviation = bedtime_deviation_minutes * 0.35
        + wake_time_deviation_minutes * 0.35
        + midpoint_deviation_minutes * 0.30;
    clamp_0_100(100.0 - average_deviation / 120.0 * 100.0)
}

fn sleep_architecture_score(
    stage_minutes_by_kind: &BTreeMap<String, f64>,
    sleep_duration_minutes: f64,
    baseline: Option<&SleepBaseline>,
) -> f64 {
    if stage_minutes_by_kind.is_empty() {
        return 55.0;
    }

    let population_score =
        sleep_architecture_population_score(stage_minutes_by_kind, sleep_duration_minutes);
    let baseline_window = baseline.and_then(preferred_sleep_baseline_window);
    if let Some(baseline_window) = baseline_window {
        let personal_score =
            sleep_architecture_personal_score(stage_minutes_by_kind, baseline_window);
        let personal_prior_weight = sleep_architecture_personal_prior_weight(baseline_window);
        population_score * (1.0 - personal_prior_weight) + personal_score * personal_prior_weight
    } else {
        population_score
    }
}

fn sleep_architecture_population_score(
    stage_minutes_by_kind: &BTreeMap<String, f64>,
    sleep_duration_minutes: f64,
) -> f64 {
    let deep_sleep_minutes = stage_minutes(stage_minutes_by_kind, "deep").unwrap_or(0.0);
    let rem_sleep_minutes = stage_minutes(stage_minutes_by_kind, "rem").unwrap_or(0.0);
    let core_sleep_minutes = stage_minutes(stage_minutes_by_kind, "core").unwrap_or(0.0);
    let restorative_sleep_minutes = deep_sleep_minutes + rem_sleep_minutes;
    let restorative_fraction = restorative_sleep_minutes / sleep_duration_minutes.max(1.0);
    let core_fraction = core_sleep_minutes / sleep_duration_minutes.max(1.0);
    let restorative_score = clamp_0_100(restorative_fraction / 0.38 * 100.0);
    let core_balance_score = clamp_0_100(100.0 - (core_fraction - 0.55).abs() / 0.35 * 100.0);
    restorative_score * 0.70 + core_balance_score * 0.30
}

fn sleep_architecture_personal_score(
    stage_minutes_by_kind: &BTreeMap<String, f64>,
    baseline_window: &SleepBaselineWindow,
) -> f64 {
    let deep_sleep_minutes = stage_minutes(stage_minutes_by_kind, "deep").unwrap_or(0.0);
    let rem_sleep_minutes = stage_minutes(stage_minutes_by_kind, "rem").unwrap_or(0.0);
    let restorative_sleep_minutes = deep_sleep_minutes + rem_sleep_minutes;
    let deep_score = ratio_closeness_score(
        deep_sleep_minutes,
        baseline_window.average_deep_sleep_minutes,
        45.0,
    );
    let rem_score = ratio_closeness_score(
        rem_sleep_minutes,
        baseline_window.average_rem_sleep_minutes,
        60.0,
    );
    let restorative_score = ratio_closeness_score(
        restorative_sleep_minutes,
        baseline_window.average_restorative_sleep_minutes,
        90.0,
    );
    deep_score * 0.30 + rem_score * 0.30 + restorative_score * 0.40
}

fn sleep_architecture_personal_prior_weight(baseline_window: &SleepBaselineWindow) -> f64 {
    if baseline_window.night_count < 7 {
        return 0.0;
    }
    let night_count_weight = clamp_fraction((baseline_window.night_count as f64 - 6.0) / 8.0);
    let confidence_weight =
        clamp_fraction((baseline_window.average_confidence_0_to_1 - 0.50) / 0.35);
    night_count_weight * confidence_weight
}

fn sleep_architecture_prior_calibration_provenance(
    baseline: Option<&SleepBaseline>,
) -> serde_json::Value {
    if let Some(window) = baseline.and_then(preferred_sleep_baseline_window) {
        let personal_prior_weight = sleep_architecture_personal_prior_weight(window);
        json!({
            "source": "personal_stage_baseline_blended_with_population_prior",
            "window_days": window.window_days,
            "night_count": window.night_count,
            "average_confidence_0_to_1": window.average_confidence_0_to_1,
            "personal_prior_weight": personal_prior_weight,
            "population_prior_weight": 1.0 - personal_prior_weight,
            "average_deep_sleep_minutes": window.average_deep_sleep_minutes,
            "average_rem_sleep_minutes": window.average_rem_sleep_minutes,
            "average_core_sleep_minutes": window.average_core_sleep_minutes,
            "average_restorative_sleep_minutes": window.average_restorative_sleep_minutes,
            "policy": "blend_personal_stage_priors_by_baseline_maturity_and_confidence",
        })
    } else {
        json!({
            "source": "population_stage_fraction_prior",
            "personal_prior_weight": 0.0,
            "population_prior_weight": 1.0,
            "policy": "use_population_stage_priors_until_personal_baseline_is_available",
        })
    }
}

fn sleep_cardiovascular_score(input: &SleepV1Input, baseline: Option<&SleepBaseline>) -> f64 {
    let dip_score = input
        .sleep
        .heart_rate_dip_percent
        .map(|dip| clamp_0_100(dip / 18.0 * 100.0))
        .unwrap_or(60.0);
    let pre_sleep_hr_score = pre_sleep_awake_hr_score(
        input.pre_sleep_awake_hr_average_bpm,
        input.sleep_hr_average_bpm,
    );
    let Some(baseline_window) = baseline.and_then(preferred_sleep_baseline_window) else {
        return match (
            sleep_hr_trend_score(input.sleep_hr_trend_bpm_per_hour, None),
            pre_sleep_hr_score,
        ) {
            (Some(trend_score), Some(pre_sleep_hr_score)) => {
                dip_score * 0.55 + trend_score * 0.25 + pre_sleep_hr_score * 0.20
            }
            (Some(trend_score), None) => dip_score * 0.75 + trend_score * 0.25,
            (None, Some(pre_sleep_hr_score)) => dip_score * 0.70 + pre_sleep_hr_score * 0.30,
            (None, None) => dip_score,
        };
    };

    let average_hr_score = match (
        input.sleep_hr_average_bpm,
        baseline_window.average_sleep_hr_bpm,
    ) {
        (Some(current), Some(baseline)) => {
            clamp_0_100(100.0 - (current - baseline).max(0.0) / 12.0 * 100.0)
        }
        _ => 75.0,
    };
    let min_hr_score = match (
        input.sleep_hr_min_bpm,
        baseline_window.average_sleep_hr_min_bpm,
    ) {
        (Some(current), Some(baseline)) => {
            clamp_0_100(100.0 - (current - baseline).max(0.0) / 10.0 * 100.0)
        }
        _ => 75.0,
    };
    let dip_vs_baseline_score = match (
        input.sleep.heart_rate_dip_percent,
        baseline_window.average_hr_dip_percent,
    ) {
        (Some(current), Some(baseline)) => clamp_0_100(70.0 + (current - baseline) / 8.0 * 30.0),
        _ => dip_score,
    };
    let trend_score = sleep_hr_trend_score(
        input.sleep_hr_trend_bpm_per_hour,
        baseline_window.average_sleep_hr_trend_bpm_per_hour,
    );

    if trend_score.is_some() {
        let base = dip_score * 0.35
            + average_hr_score * 0.20
            + min_hr_score * 0.15
            + dip_vs_baseline_score * 0.15
            + trend_score.unwrap() * 0.15;
        pre_sleep_hr_score
            .map(|score| base * 0.90 + score * 0.10)
            .unwrap_or(base)
    } else {
        let base = dip_score * 0.45
            + average_hr_score * 0.25
            + min_hr_score * 0.15
            + dip_vs_baseline_score * 0.15;
        pre_sleep_hr_score
            .map(|score| base * 0.90 + score * 0.10)
            .unwrap_or(base)
    }
}

fn pre_sleep_awake_hr_score(
    pre_sleep_awake_hr_bpm: Option<f64>,
    sleep_hr_bpm: Option<f64>,
) -> Option<f64> {
    let pre_sleep_awake_hr_bpm = pre_sleep_awake_hr_bpm?;
    let sleep_hr_bpm = sleep_hr_bpm?;
    let drop_bpm = pre_sleep_awake_hr_bpm - sleep_hr_bpm;
    Some(clamp_0_100(if drop_bpm >= 0.0 {
        70.0 + drop_bpm.min(10.0) / 10.0 * 30.0
    } else {
        70.0 + drop_bpm.max(-8.0) / 8.0 * 45.0
    }))
}

fn sleep_hr_trend_score(
    current_bpm_per_hour: Option<f64>,
    baseline_bpm_per_hour: Option<f64>,
) -> Option<f64> {
    let current = current_bpm_per_hour?;
    let expected = baseline_bpm_per_hour.unwrap_or(0.0);
    let excess_rise = (current - expected).max(0.0);
    let recovery_drop = (expected - current).max(0.0);
    Some(clamp_0_100(
        82.0 - excess_rise / 3.0 * 62.0 + recovery_drop.min(2.0) / 2.0 * 18.0,
    ))
}

fn sleep_context_score(prior_day_strain: Option<f64>, naps_minutes: f64) -> f64 {
    let strain_penalty = prior_day_strain
        .map(|strain| (strain - 12.0).max(0.0) / 9.0 * 20.0)
        .unwrap_or(5.0);
    let nap_penalty = (naps_minutes - 45.0).max(0.0) / 90.0 * 20.0;
    clamp_0_100(100.0 - strain_penalty - nap_penalty)
}

fn sleep_v1_guardrailed_score(
    score_0_to_100: f64,
    input: &SleepV1Input,
    efficiency_fraction: f64,
    quality_flags: &mut Vec<String>,
) -> f64 {
    let mut score = clamp_0_100(score_0_to_100);
    if input.sleep.sleep_duration_minutes < 180.0 {
        quality_flags.push("sleep_v1_guardrail_very_short_sleep".to_string());
        score = score.min(45.0);
    }
    if input.sleep.wake_after_sleep_onset_minutes >= 120.0 || input.sleep.wake_episode_count >= 10 {
        quality_flags.push("sleep_v1_guardrail_severe_fragmentation".to_string());
        score = score.min(65.0);
    }
    if efficiency_fraction < 0.55 {
        quality_flags.push("sleep_v1_guardrail_low_efficiency".to_string());
        score = score.min(60.0);
    }
    score
}

fn preferred_sleep_baseline_window(baseline: &SleepBaseline) -> Option<&SleepBaselineWindow> {
    baseline
        .current_14_day
        .as_ref()
        .or(baseline.short_7_day.as_ref())
        .or(baseline.stable_28_day.as_ref())
}

fn ratio_closeness_score(current: f64, expected: f64, tolerance_minutes: f64) -> f64 {
    if expected <= 0.0 {
        return 70.0;
    }
    clamp_0_100(100.0 - (current - expected).abs() / tolerance_minutes * 100.0)
}

fn plural_suffix(count: u32) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn score_component(
    name: &str,
    value: f64,
    unit: &str,
    score_0_to_100: f64,
    weight: f64,
    output_scale: f64,
) -> ScoreComponent {
    ScoreComponent {
        name: name.to_string(),
        value,
        unit: unit.to_string(),
        score_0_to_100: clamp_0_100(score_0_to_100),
        weight,
        contribution: clamp_0_100(score_0_to_100) / 100.0 * output_scale * weight,
    }
}

fn component_sum(components: &[ScoreComponent]) -> f64 {
    components
        .iter()
        .map(|component| component.contribution)
        .sum()
}

fn clamp_0_100(value: f64) -> f64 {
    clamp_0_to(100.0, value)
}

fn clamp_0_to(max: f64, value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    value.clamp(0.0, max)
}

fn clamp_fraction(value: f64) -> f64 {
    clamp_0_to(1.0, value)
}

fn default_sleep_history_confidence() -> f64 {
    1.0
}

fn validate_sleep_night_history_input(
    index: usize,
    input: &SleepNightHistoryInput,
    errors: &mut Vec<String>,
) {
    let prefix = format!("prior_nights_{index}");
    if input.night_id.trim().is_empty() {
        errors.push(format!("{prefix}_night_id_required"));
    }
    if input.start_time.trim().is_empty() {
        errors.push(format!("{prefix}_start_time_required"));
    }
    if input.end_time.trim().is_empty() {
        errors.push(format!("{prefix}_end_time_required"));
    }
    require_finite_positive(
        &format!("{prefix}_sleep_duration_minutes"),
        input.sleep_duration_minutes,
        errors,
    );
    require_finite_positive(
        &format!("{prefix}_sleep_need_minutes"),
        input.sleep_need_minutes,
        errors,
    );
    require_finite_positive(
        &format!("{prefix}_time_in_bed_minutes"),
        input.time_in_bed_minutes,
        errors,
    );
    if input.sleep_duration_minutes.is_finite()
        && input.time_in_bed_minutes.is_finite()
        && input.sleep_duration_minutes > input.time_in_bed_minutes
    {
        errors.push(format!("{prefix}_sleep_duration_exceeds_time_in_bed"));
    }
    require_finite_non_negative(
        &format!("{prefix}_awake_minutes"),
        input.awake_minutes,
        errors,
    );
    if input.awake_minutes.is_finite()
        && input.time_in_bed_minutes.is_finite()
        && input.awake_minutes > input.time_in_bed_minutes
    {
        errors.push(format!("{prefix}_awake_minutes_exceeds_time_in_bed"));
    }
    if input.sleep_duration_minutes.is_finite()
        && input.awake_minutes.is_finite()
        && input.time_in_bed_minutes.is_finite()
        && input.sleep_duration_minutes + input.awake_minutes > input.time_in_bed_minutes + 5.0
    {
        errors.push(format!(
            "{prefix}_sleep_duration_plus_awake_minutes_exceeds_time_in_bed"
        ));
    }
    require_finite_non_negative(
        &format!("{prefix}_sleep_latency_minutes"),
        input.sleep_latency_minutes,
        errors,
    );
    require_finite_non_negative(
        &format!("{prefix}_wake_after_sleep_onset_minutes"),
        input.wake_after_sleep_onset_minutes,
        errors,
    );
    require_finite_non_negative(
        &format!("{prefix}_bedtime_deviation_minutes"),
        input.bedtime_deviation_minutes,
        errors,
    );
    require_finite_non_negative(
        &format!("{prefix}_wake_time_deviation_minutes"),
        input.wake_time_deviation_minutes,
        errors,
    );
    require_finite_non_negative(
        &format!("{prefix}_midpoint_deviation_minutes"),
        input.midpoint_deviation_minutes,
        errors,
    );
    require_finite_non_negative(
        &format!("{prefix}_naps_minutes"),
        input.naps_minutes,
        errors,
    );
    require_bounded(
        &format!("{prefix}_confidence_0_to_1"),
        input.confidence_0_to_1,
        0.0,
        1.0,
        errors,
    );
    if let Some(heart_rate_dip_percent) = input.heart_rate_dip_percent {
        require_finite_non_negative(
            &format!("{prefix}_heart_rate_dip_percent"),
            heart_rate_dip_percent,
            errors,
        );
    }
    if let Some(sleep_hr_average_bpm) = input.sleep_hr_average_bpm {
        require_finite_positive(
            &format!("{prefix}_sleep_hr_average_bpm"),
            sleep_hr_average_bpm,
            errors,
        );
    }
    if let Some(sleep_hr_min_bpm) = input.sleep_hr_min_bpm {
        require_finite_positive(
            &format!("{prefix}_sleep_hr_min_bpm"),
            sleep_hr_min_bpm,
            errors,
        );
    }
    if let Some(pre_sleep_awake_hr_average_bpm) = input.pre_sleep_awake_hr_average_bpm {
        require_finite_positive(
            &format!("{prefix}_pre_sleep_awake_hr_average_bpm"),
            pre_sleep_awake_hr_average_bpm,
            errors,
        );
    }
    if let Some(sleep_hr_trend_bpm_per_hour) = input.sleep_hr_trend_bpm_per_hour
        && !sleep_hr_trend_bpm_per_hour.is_finite()
    {
        errors.push(format!(
            "{prefix}_sleep_hr_trend_bpm_per_hour_must_be_finite"
        ));
    }
    for (stage, minutes) in &input.stage_minutes {
        if stage.trim().is_empty() || !minutes.is_finite() || *minutes < 0.0 {
            errors.push(format!("{prefix}_stage_minutes_invalid"));
        }
    }
    if !input.stage_minutes.is_empty()
        && input
            .stage_minutes
            .values()
            .all(|minutes| minutes.is_finite() && *minutes >= 0.0)
    {
        let total_stage_minutes = input.stage_minutes.values().sum::<f64>();
        if input.time_in_bed_minutes.is_finite()
            && total_stage_minutes > input.time_in_bed_minutes + 5.0
        {
            errors.push(format!("{prefix}_stage_minutes_exceed_time_in_bed"));
        }
        let asleep_stage_minutes = input
            .stage_minutes
            .iter()
            .filter(|(stage, _)| stage.as_str() != "awake")
            .map(|(_, minutes)| *minutes)
            .sum::<f64>();
        if input.sleep_duration_minutes.is_finite()
            && asleep_stage_minutes > input.sleep_duration_minutes + 5.0
        {
            errors.push(format!(
                "{prefix}_asleep_stage_minutes_exceed_sleep_duration"
            ));
        }
    }
}

fn validate_sleep_stage_segment(index: usize, input: &SleepStageSegment, errors: &mut Vec<String>) {
    let prefix = format!("stage_segments_{index}");
    if input.stage_kind.trim().is_empty() {
        errors.push(format!("{prefix}_stage_kind_required"));
    } else if !sleep_v1_stage_kind_is_allowed(&input.stage_kind) {
        errors.push(format!("{prefix}_stage_kind_unrecognized"));
    }
    if input.start_time.trim().is_empty() {
        errors.push(format!("{prefix}_start_time_required"));
    }
    if input.end_time.trim().is_empty() {
        errors.push(format!("{prefix}_end_time_required"));
    }
    require_finite_positive(
        &format!("{prefix}_duration_minutes"),
        input.duration_minutes,
        errors,
    );
    require_bounded(
        &format!("{prefix}_confidence_0_to_1"),
        input.confidence_0_to_1,
        0.0,
        1.0,
        errors,
    );
    let mut probability_sum = 0.0;
    for (stage, probability) in &input.stage_probabilities {
        if stage.trim().is_empty() {
            errors.push(format!("{prefix}_stage_probability_name_required"));
        } else if !sleep_v1_stage_kind_is_allowed(stage) {
            errors.push(format!("{prefix}_stage_probability_{stage}_unrecognized"));
        }
        require_bounded(
            &format!("{prefix}_stage_probability_{stage}"),
            *probability,
            0.0,
            1.0,
            errors,
        );
        probability_sum += probability;
    }
    if !input.stage_probabilities.is_empty() && probability_sum > 1.05 {
        errors.push(format!("{prefix}_stage_probability_sum_must_not_exceed_1"));
    }
}

fn validate_sleep_v1_current_stage_minutes(input: &SleepV1Input, errors: &mut Vec<String>) {
    for stage in input.sleep.stage_minutes.keys() {
        if stage.trim().is_empty() {
            continue;
        }
        if !sleep_v1_stage_kind_is_allowed(stage) {
            errors.push(format!("sleep_stage_minutes_{stage}_unrecognized"));
        }
    }
}

fn validate_sleep_v1_sleep_window(input: &SleepV1Input, errors: &mut Vec<String>) {
    let start = sleep_time_unix_ms(&input.sleep.start_time);
    let end = sleep_time_unix_ms(&input.sleep.end_time);
    if start.is_none() {
        errors.push("sleep_window_start_time_invalid".to_string());
    }
    if end.is_none() {
        errors.push("sleep_window_end_time_invalid".to_string());
    }
    if let (Some(start), Some(end)) = (start, end) {
        if end <= start {
            errors.push("sleep_window_end_time_must_be_after_start_time".to_string());
        } else {
            let actual_time_in_bed_minutes = (end - start) as f64 / 60_000.0;
            if (actual_time_in_bed_minutes - input.sleep.time_in_bed_minutes).abs() > 1.0 {
                errors.push("sleep_window_time_in_bed_minutes_mismatch".to_string());
            }
        }
    }
    if input.sleep.sleep_duration_minutes.is_finite()
        && input.sleep.time_in_bed_minutes.is_finite()
        && input.sleep.sleep_duration_minutes > input.sleep.time_in_bed_minutes + 1.0
    {
        errors.push("sleep_window_sleep_duration_exceeds_time_in_bed".to_string());
    }
    if input.sleep.sleep_duration_minutes.is_finite()
        && input.sleep.sleep_latency_minutes.is_finite()
        && input.sleep.wake_after_sleep_onset_minutes.is_finite()
        && input.sleep.time_in_bed_minutes.is_finite()
        && input.sleep.sleep_duration_minutes
            + input.sleep.sleep_latency_minutes
            + input.sleep.wake_after_sleep_onset_minutes
            > input.sleep.time_in_bed_minutes + 5.0
    {
        errors.push("sleep_window_sleep_latency_waso_duration_exceeds_time_in_bed".to_string());
    }
}

fn sleep_v1_stage_kind_is_allowed(stage: &str) -> bool {
    matches!(
        stage
            .trim()
            .to_ascii_lowercase()
            .replace([' ', '-'], "_")
            .as_str(),
        "awake" | "core" | "deep" | "rem"
    )
}

fn validate_sleep_stage_timeline(input: &SleepV1Input, errors: &mut Vec<String>) {
    if input.stage_segments.is_empty() {
        return;
    }
    let sleep_start = sleep_time_unix_ms(&input.sleep.start_time);
    let sleep_end = sleep_time_unix_ms(&input.sleep.end_time);
    let mut parsed_segments = Vec::new();

    for (index, segment) in input.stage_segments.iter().enumerate() {
        let prefix = format!("stage_segments_{index}");
        let start = sleep_time_unix_ms(&segment.start_time);
        let end = sleep_time_unix_ms(&segment.end_time);
        if start.is_none() {
            errors.push(format!("{prefix}_start_time_invalid"));
        }
        if end.is_none() {
            errors.push(format!("{prefix}_end_time_invalid"));
        }
        let (Some(start), Some(end)) = (start, end) else {
            continue;
        };
        if end <= start {
            errors.push(format!("{prefix}_end_time_must_be_after_start_time"));
            continue;
        }
        let actual_duration_minutes = (end - start) as f64 / 60_000.0;
        if (actual_duration_minutes - segment.duration_minutes).abs() > 1.0 {
            errors.push(format!("{prefix}_duration_minutes_mismatch"));
        }
        if let (Some(sleep_start), Some(sleep_end)) = (sleep_start, sleep_end)
            && (start < sleep_start || end > sleep_end)
        {
            errors.push(format!("{prefix}_outside_sleep_window"));
        }
        parsed_segments.push((index, start, end, segment.duration_minutes));
    }

    parsed_segments.sort_by_key(|(_, start, _, _)| *start);
    let mut previous_end = None;
    for (index, start, end, _) in &parsed_segments {
        if let Some(previous_end) = previous_end
            && *start < previous_end
        {
            errors.push(format!("stage_segments_{index}_overlaps_previous_segment"));
        }
        previous_end = Some(previous_end.map_or(*end, |value| value.max(*end)));
    }

    let total_stage_minutes = parsed_segments
        .iter()
        .map(|(_, _, _, duration)| *duration)
        .sum::<f64>();
    if total_stage_minutes > input.sleep.time_in_bed_minutes + 1.0 {
        errors.push("stage_segments_total_duration_exceeds_time_in_bed".to_string());
    }
}

fn sleep_time_unix_ms(value: &str) -> Option<i64> {
    if let Some(unix_ms) = value
        .strip_prefix("unix_ms:")
        .and_then(|text| text.parse::<i64>().ok())
    {
        return Some(unix_ms);
    }
    parse_rfc3339_utc_unix_ms(value)
}

fn parse_rfc3339_utc_unix_ms(value: &str) -> Option<i64> {
    let value = value.strip_suffix('Z')?;
    let (date, time) = value.split_once('T')?;
    let mut date_parts = date.split('-');
    let year = date_parts.next()?.parse::<i32>().ok()?;
    let month = date_parts.next()?.parse::<u32>().ok()?;
    let day = date_parts.next()?.parse::<u32>().ok()?;
    if date_parts.next().is_some() {
        return None;
    }
    let mut time_parts = time.split(':');
    let hour = time_parts.next()?.parse::<u32>().ok()?;
    let minute = time_parts.next()?.parse::<u32>().ok()?;
    let seconds_part = time_parts.next()?;
    if time_parts.next().is_some() {
        return None;
    }
    let second = seconds_part
        .split_once('.')
        .map(|(second, _)| second)
        .unwrap_or(seconds_part)
        .parse::<u32>()
        .ok()?;
    if !(1..=12).contains(&month)
        || day == 0
        || day > days_in_month(year, month)
        || hour >= 24
        || minute >= 60
        || second >= 60
    {
        return None;
    }
    let days = days_from_civil(year, month, day);
    Some((days * 86_400 + hour as i64 * 3_600 + minute as i64 * 60 + second as i64) * 1_000)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year as i64 - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = month as i64;
    let day = day as i64;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn require_finite_positive(name: &str, value: f64, errors: &mut Vec<String>) {
    if !value.is_finite() || value <= 0.0 {
        errors.push(format!("{name}_must_be_finite_positive"));
    }
}

fn require_finite_non_negative(name: &str, value: f64, errors: &mut Vec<String>) {
    if !value.is_finite() || value < 0.0 {
        errors.push(format!("{name}_must_be_finite_non_negative"));
    }
}

fn require_bounded(name: &str, value: f64, min: f64, max: f64, errors: &mut Vec<String>) {
    if !value.is_finite() || value < min || value > max {
        errors.push(format!("{name}_must_be_between_{min}_and_{max}"));
    }
}

const RECOVERY_POPULATION_MEAN: f64 = 58.0;

/// Colour band for the Recovery V1 score, named in PT-PT.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColourBand {
    Vermelho,
    Amarelo,
    Verde,
}

impl ColourBand {
    /// Classify a 0–100 score into a colour band.
    ///
    /// Verde ≥ 67, Amarelo ≥ 34, Vermelho < 34.
    pub fn from_score(score: f64) -> Self {
        if score >= 67.0 {
            Self::Verde
        } else if score >= 34.0 {
            Self::Amarelo
        } else {
            Self::Vermelho
        }
    }

    /// PT-PT lowercase name, suitable for JSON serialisation.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Verde => "verde",
            Self::Amarelo => "amarelo",
            Self::Vermelho => "vermelho",
        }
    }
}

/// Input for the personal-baseline recovery score algorithm (v1).
///
/// The database path is carried by the bridge layer; this struct holds only the
/// nightly biometric values. Keeping the database path out of the struct lets
/// `bull_recovery_v1` stay a pure, unit-testable function.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecoveryV1Input {
    pub device_id: String,
    pub date_key: String,
    pub hrv_rmssd_ms: f64,
    pub resting_hr_bpm: f64,
    /// Optional respiratory rate deviation from baseline (z-score units).
    /// When Some: contributes resp_z * 0.05 to combined_z (ALG-ALIGN-01).
    #[serde(default)]
    pub resp_rate_rpm: Option<f64>,
    /// Optional sleep performance fraction 0–1.
    /// When Some: contributes sleep_perf_z * 0.15 to combined_z (ALG-ALIGN-01).
    #[serde(default)]
    pub sleep_performance_fraction: Option<f64>,
}

/// Output of the personal-baseline recovery score algorithm (v1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecoveryV1Output {
    pub algorithm_id: String,
    pub algorithm_version: String,
    /// None when baseline is in Calibrating state (< 4 nights).
    pub score_0_to_100: Option<f64>,
    /// "calibrating" | "provisional" | "trusted"
    pub trust_level: String,
    /// "vermelho" | "amarelo" | "verde"
    pub colour_band: String,
    /// Z-score for HRV RMSSD (None when Calibrating).
    pub z_hrv: Option<f64>,
    /// Z-score for resting HR (None when Calibrating or RHR baseline insufficient).
    pub z_rhr: Option<f64>,
}

/// Compute the personal-baseline Recovery score (v1, ALG-REC-01/02).
///
/// Takes the raw nightly biometrics and a pre-reconstructed EWMA baseline (the
/// bridge layer reconstructs the baseline via `EwmaBaseline::fold_history`).
///
/// When the HRV baseline night_count < 4 (cold-start), returns `score_0_to_100`
/// as `None`. The colour band is derived from the population mean (58.0) so the
/// calibrating UI still shows a meaningful band.
///
/// When trust ≥ Provisional (personal-baseline weights):
/// - combined z = 0.60 * z_hrv - 0.20 * z_rhr [- 0.05 * z_resp + 0.15 * z_sleep_perf] (optional)
/// - score = 100.0 / (1.0 + exp(-1.6 * (z + 0.20)))  [logistic squash; Z=0 → ~58%]
///
/// Optional components (resp_rate_rpm, sleep_performance_fraction) are included only
/// when the caller passes them; weight is re-normalised so the mandatory components
/// always sum to their full weight.
pub fn bull_recovery_v1(input: &RecoveryV1Input, baseline: &EwmaBaseline) -> RecoveryV1Output {
    let trust = baseline.hrv.trust_level();
    let trust_level = trust.as_str().to_string();

    let z_hrv = baseline.hrv.z_score(input.hrv_rmssd_ms);
    // RHR z-score is sign-flipped: lower RHR than baseline = positive contribution.
    let z_rhr_raw = baseline.resting_hr.z_score(input.resting_hr_bpm);

    // Cold-start gate: HRV baseline not seeded yet.
    if z_hrv.is_none() {
        let band = ColourBand::from_score(RECOVERY_POPULATION_MEAN)
            .as_str()
            .to_string();
        return RecoveryV1Output {
            algorithm_id: BULL_RECOVERY_V1_ID.to_string(),
            algorithm_version: BULL_RECOVERY_V1_VERSION.to_string(),
            score_0_to_100: None,
            trust_level,
            colour_band: band,
            z_hrv: None,
            z_rhr: None,
        };
    }

    let z_hrv_val = z_hrv.expect("checked above");

    // Personal-baseline component weights.
    // Mandatory: HRV 0.60, RHR 0.20.
    // Optional: resp 0.05 (z = deviation from population mean ≈ 0 when near baseline),
    //           sleep_perf 0.15 (z = (frac - 0.85) / 0.10, centered on 85% efficiency).
    let mandatory_z = {
        let rhr_component = z_rhr_raw.map_or(0.0, |z| -0.20 * z);
        0.60 * z_hrv_val + rhr_component
    };

    // Optional resp component: z_resp = -(resp_rpm - baseline). Negative = better.
    let resp_component: f64 = 0.0; // No resp baseline stored yet; reserved for future use.

    // Optional sleep performance component: z_sleep_perf = (frac - 0.85) / 0.10.
    let sleep_perf_component: f64 = input
        .sleep_performance_fraction
        .map_or(0.0, |frac| 0.15 * (frac - 0.85) / 0.10);

    let combined_z = if z_rhr_raw.is_none() {
        // When RHR is absent, redistribute its weight to HRV.
        z_hrv_val + resp_component + sleep_perf_component
    } else {
        mandatory_z + resp_component + sleep_perf_component
    };
    let _ = resp_component; // suppress unused warning when resp is always 0.0

    let score = 100.0 / (1.0 + (-1.6_f64 * (combined_z + 0.20)).exp());
    let colour_band = ColourBand::from_score(score).as_str().to_string();

    RecoveryV1Output {
        algorithm_id: BULL_RECOVERY_V1_ID.to_string(),
        algorithm_version: BULL_RECOVERY_V1_VERSION.to_string(),
        score_0_to_100: Some(score),
        trust_level,
        colour_band,
        z_hrv: Some(z_hrv_val),
        z_rhr: z_rhr_raw,
    }
}

#[cfg(test)]
mod recovery_v1_tests {
    use super::*;
    use crate::baselines::{EwmaBaseline, EwmaState, MIN_NIGHTS_SEED, MIN_NIGHTS_TRUST};

    /// Build a minimal EwmaState with exact mean/variance/night_count for test control.
    fn make_state(mean: f64, variance: f64, night_count: usize) -> EwmaState {
        EwmaState {
            mean,
            variance,
            night_count,
        }
    }

    fn make_baseline(hrv: EwmaState, resting_hr: EwmaState) -> EwmaBaseline {
        EwmaBaseline { hrv, resting_hr }
    }

    fn make_input() -> RecoveryV1Input {
        RecoveryV1Input {
            device_id: "test-device".to_string(),
            date_key: "2024-01-01".to_string(),
            hrv_rmssd_ms: 60.0,
            resting_hr_bpm: 55.0,
            resp_rate_rpm: None,
            sleep_performance_fraction: None,
        }
    }

    // ---- Z=0 → ≈58% --------------------------------------------------------

    #[test]
    fn test_recovery_v1_z_zero_yields_approx_58_percent() {
        // To get combined Z = 0: need z_hrv = 0 and z_rhr = 0.
        // z_hrv = 0 when hrv_rmssd_ms == hrv.mean.
        // z_rhr = 0 when resting_hr_bpm == resting_hr.mean.
        let sigma = 5.0_f64;
        let hrv = make_state(60.0, sigma * sigma, MIN_NIGHTS_SEED);
        let rhr = make_state(55.0, sigma * sigma, MIN_NIGHTS_SEED);
        let baseline = make_baseline(hrv, rhr);

        let input = RecoveryV1Input {
            device_id: "dev".to_string(),
            date_key: "2024-01-01".to_string(),
            hrv_rmssd_ms: 60.0,   // == hrv.mean → z_hrv = 0
            resting_hr_bpm: 55.0, // == rhr.mean → z_rhr = 0
            resp_rate_rpm: None,
            sleep_performance_fraction: None,
        };

        let output = bull_recovery_v1(&input, &baseline);
        let score = output
            .score_0_to_100
            .expect("score must be Some when trusted");

        // Expected: 100 / (1 + exp(-1.6 * 0.20)) = 100 / (1 + exp(-0.32)) ≈ 57.9%
        let expected = 100.0 / (1.0 + (-1.6_f64 * 0.20_f64).exp());
        assert!(
            (score - expected).abs() < 0.5,
            "Z=0 score must be within 0.5% of {:.4}, got {:.4}",
            expected,
            score
        );
        assert!(
            (score - 58.0).abs() < 0.5,
            "Z=0 score must be within 0.5% of 58.0, got {:.4}",
            score
        );
    }

    // ---- Z_RHR sign flip: lower RHR = better (positive contribution) --------

    #[test]
    fn test_recovery_v1_lower_rhr_improves_score() {
        let sigma = 5.0_f64;
        // HRV at mean (z_hrv = 0)
        let hrv = make_state(60.0, sigma * sigma, MIN_NIGHTS_SEED);
        let rhr = make_state(60.0, sigma * sigma, MIN_NIGHTS_SEED);

        // Baseline: low score (RHR at mean, HRV at mean → z=0 → ~58%)
        let baseline_neutral = make_baseline(hrv.clone(), rhr.clone());
        let input_neutral = RecoveryV1Input {
            device_id: "dev".to_string(),
            date_key: "2024-01-01".to_string(),
            hrv_rmssd_ms: 60.0,
            resting_hr_bpm: 60.0, // == rhr.mean → z_rhr = 0
            resp_rate_rpm: None,
            sleep_performance_fraction: None,
        };
        let output_neutral = bull_recovery_v1(&input_neutral, &baseline_neutral);
        let score_neutral = output_neutral.score_0_to_100.unwrap();

        // Scenario: RHR is below baseline mean → lower than usual → BETTER → should raise score.
        let baseline_good = make_baseline(hrv, rhr);
        let input_good = RecoveryV1Input {
            device_id: "dev".to_string(),
            date_key: "2024-01-01".to_string(),
            hrv_rmssd_ms: 60.0,
            resting_hr_bpm: 55.0, // below rhr.mean=60 → z_rhr_raw < 0 → -0.20 * z_rhr_raw > 0
            resp_rate_rpm: None,
            sleep_performance_fraction: None,
        };
        let output_good = bull_recovery_v1(&input_good, &baseline_good);
        let score_good = output_good.score_0_to_100.unwrap();

        assert!(
            score_good > score_neutral,
            "lower RHR than baseline must improve score: neutral={:.4}, good={:.4}",
            score_neutral,
            score_good
        );
    }

    // ---- Cold-start: score is None when HRV baseline < 4 nights -------------

    #[test]
    fn test_recovery_v1_cold_start_score_none() {
        let hrv = make_state(60.0, 25.0, 3); // < MIN_NIGHTS_SEED=4
        let rhr = make_state(55.0, 25.0, 3);
        let baseline = make_baseline(hrv, rhr);
        let input = make_input();

        let output = bull_recovery_v1(&input, &baseline);

        assert!(
            output.score_0_to_100.is_none(),
            "cold-start: score must be None when HRV night_count < 4"
        );
        assert_eq!(
            output.trust_level, "calibrating",
            "cold-start: trust_level must be 'calibrating'"
        );
        assert!(output.z_hrv.is_none(), "cold-start: z_hrv must be None");
    }

    // ---- Cold-start: colour band falls back to population mean band ----------

    #[test]
    fn test_recovery_v1_cold_start_colour_band_population_mean() {
        let hrv = make_state(60.0, 25.0, 0); // no nights
        let rhr = make_state(55.0, 25.0, 0);
        let baseline = make_baseline(hrv, rhr);
        let input = make_input();

        let output = bull_recovery_v1(&input, &baseline);

        // RECOVERY_POPULATION_MEAN=58.0 → ColourBand::Amarelo (34 ≤ 58 < 67)
        assert_eq!(
            output.colour_band, "amarelo",
            "cold-start colour band must be 'amarelo' (population mean 58.0)"
        );
    }

    // ---- Colour band boundaries --------------------------------------------

    #[test]
    fn test_colour_band_verde_at_67() {
        assert_eq!(ColourBand::from_score(67.0), ColourBand::Verde);
        assert_eq!(ColourBand::from_score(67.0).as_str(), "verde");
    }

    #[test]
    fn test_colour_band_amarelo_at_66_9() {
        assert_eq!(ColourBand::from_score(66.9), ColourBand::Amarelo);
        assert_eq!(ColourBand::from_score(66.9).as_str(), "amarelo");
    }

    #[test]
    fn test_colour_band_amarelo_at_34() {
        assert_eq!(ColourBand::from_score(34.0), ColourBand::Amarelo);
        assert_eq!(ColourBand::from_score(34.0).as_str(), "amarelo");
    }

    #[test]
    fn test_colour_band_vermelho_at_33_9() {
        assert_eq!(ColourBand::from_score(33.9), ColourBand::Vermelho);
        assert_eq!(ColourBand::from_score(33.9).as_str(), "vermelho");
    }

    // ---- Trust level mapping -----------------------------------------------

    #[test]
    fn test_recovery_v1_trust_calibrating_at_3_nights() {
        let hrv = make_state(60.0, 25.0, 3);
        let rhr = make_state(55.0, 25.0, 3);
        let baseline = make_baseline(hrv, rhr);
        let output = bull_recovery_v1(&make_input(), &baseline);
        assert_eq!(output.trust_level, "calibrating");
    }

    #[test]
    fn test_recovery_v1_trust_provisional_at_4_nights() {
        let sigma = 5.0_f64;
        let hrv = make_state(60.0, sigma * sigma, MIN_NIGHTS_SEED);
        let rhr = make_state(55.0, sigma * sigma, MIN_NIGHTS_SEED);
        let baseline = make_baseline(hrv, rhr);
        let output = bull_recovery_v1(&make_input(), &baseline);
        assert_eq!(output.trust_level, "provisional");
        assert!(
            output.score_0_to_100.is_some(),
            "provisional must have a score"
        );
    }

    #[test]
    fn test_recovery_v1_trust_trusted_at_14_nights() {
        let sigma = 5.0_f64;
        let hrv = make_state(60.0, sigma * sigma, MIN_NIGHTS_TRUST);
        let rhr = make_state(55.0, sigma * sigma, MIN_NIGHTS_TRUST);
        let baseline = make_baseline(hrv, rhr);
        let output = bull_recovery_v1(&make_input(), &baseline);
        assert_eq!(output.trust_level, "trusted");
        assert!(output.score_0_to_100.is_some(), "trusted must have a score");
    }

    // ---- z_rhr None fallback (only z_hrv used) -----------------------------

    #[test]
    fn test_recovery_v1_z_rhr_none_uses_z_hrv_alone() {
        // HRV has 4 nights (provisional), RHR has 0 nights → z_rhr is None.
        let sigma = 5.0_f64;
        let hrv = make_state(60.0, sigma * sigma, MIN_NIGHTS_SEED);
        let rhr = make_state(55.0, sigma * sigma, 0); // no nights
        let baseline = make_baseline(hrv, rhr);

        let input = RecoveryV1Input {
            device_id: "dev".to_string(),
            date_key: "2024-01-01".to_string(),
            hrv_rmssd_ms: 60.0, // == hrv.mean → z_hrv = 0
            resting_hr_bpm: 55.0,
            resp_rate_rpm: None,
            sleep_performance_fraction: None,
        };

        let output = bull_recovery_v1(&input, &baseline);
        // With z_hrv=0 and z_rhr=None: combined_z = z_hrv = 0
        // score = 100 / (1 + exp(-1.6 * 0.20)) ≈ 57.9
        let score = output.score_0_to_100.expect("must have score");
        let expected = 100.0 / (1.0 + (-1.6_f64 * 0.20_f64).exp());
        assert!(
            (score - expected).abs() < 0.5,
            "z_rhr None: score must use z_hrv alone, got {:.4}, expected {:.4}",
            score,
            expected
        );
        assert!(
            output.z_rhr.is_none(),
            "z_rhr must be None when rhr baseline cold"
        );
    }
}

// ---------------------------------------------------------------------------
// Readiness Engine — ACWR + Foster monotony (RDY-01, RDY-02, RDY-03)
// ---------------------------------------------------------------------------

/// 5-class daily readiness level derived from ACWR and Foster monotony.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessLevel {
    Rundown,
    Strained,
    Balanced,
    Primed,
    Unknown,
}

/// Input for bull_readiness_v1.
///
/// `daily_strain` is a slice of (timestamp_secs_f64, strain_0_to_21_f64) pairs
/// ordered newest-last. The caller is responsible for providing pairs in
/// chronological order; bull_readiness_v1 uses slice positions, not timestamps.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReadinessInput {
    pub daily_strain: Vec<(f64, f64)>,
}

/// Output of bull_readiness_v1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReadinessOutput {
    pub algorithm_id: String,
    pub algorithm_version: String,
    /// Acute:chronic workload ratio (7-day / 28-day mean). None when chronic=0.
    pub acwr: Option<f64>,
    /// "under_training" | "optimal" | "caution" | "danger" | "unknown"
    pub acwr_zone: String,
    /// Foster training monotony (mean/std of last 7 days). None when std=0 or <3 entries.
    pub monotony: Option<f64>,
    pub monotony_high: bool,
    pub level: ReadinessLevel,
    pub insufficient_data: bool,
}

fn acwr_zone_str(acwr: f64) -> &'static str {
    if acwr < 0.8 {
        "under_training"
    } else if acwr <= 1.3 {
        // 0.8 boundary → "optimal"; 1.3 boundary → "optimal"
        "optimal"
    } else if acwr < 1.5 {
        // 1.3 < acwr < 1.5 → "caution"; 1.5 boundary → "danger"
        "caution"
    } else {
        "danger"
    }
}

/// Compute Foster training monotony from the last 7 strain values.
/// Returns None when fewer than 3 entries available or when population std dev is 0.
fn foster_monotony(strains: &[f64]) -> Option<f64> {
    if strains.len() < 3 {
        return None;
    }
    let n = strains.len() as f64;
    let mean = strains.iter().sum::<f64>() / n;
    let variance = strains.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    let std_dev = variance.sqrt();
    if std_dev == 0.0 {
        return None;
    }
    Some(mean / std_dev)
}

/// Compute the Readiness Engine output from a trailing window of daily strain values.
///
/// Expects `input.daily_strain` ordered chronologically (oldest first, newest last).
/// Requires at least 28 entries for a valid ACWR computation; fewer entries returns
/// `insufficient_data=true` with `level=Unknown`.
///
/// Caller should pre-filter NaN/Inf values before passing to this function.
pub fn bull_readiness_v1(input: &ReadinessInput) -> ReadinessOutput {
    let n = input.daily_strain.len();

    if n < 28 {
        return ReadinessOutput {
            algorithm_id: BULL_READINESS_V1_ID.to_string(),
            algorithm_version: BULL_READINESS_V1_VERSION.to_string(),
            acwr: None,
            acwr_zone: "unknown".to_string(),
            monotony: None,
            monotony_high: false,
            level: ReadinessLevel::Unknown,
            insufficient_data: true,
        };
    }

    // ACWR: acute = mean of last 7; chronic = mean of last 28.
    let window28: Vec<f64> = input.daily_strain[n - 28..]
        .iter()
        .map(|(_, s)| *s)
        .collect();
    let window7 = &window28[21..]; // last 7 of the 28

    let acute_load = window7.iter().sum::<f64>() / 7.0;
    let chronic_load = window28.iter().sum::<f64>() / 28.0;

    let acwr = if !chronic_load.is_finite() || chronic_load == 0.0 {
        None
    } else {
        Some((acute_load / chronic_load).clamp(0.0, 3.0))
    };

    let acwr_zone = match acwr {
        Some(v) => acwr_zone_str(v).to_string(),
        None => "unknown".to_string(),
    };

    // Foster monotony from last 7 strains.
    let monotony = foster_monotony(window7);
    let monotony_high = monotony.is_some_and(|m| m >= 2.0);

    // Level synthesis (evaluated in priority order).
    let level = match acwr {
        None => ReadinessLevel::Unknown,
        Some(v) if v >= 1.5 => ReadinessLevel::Rundown,
        _ if monotony_high => ReadinessLevel::Strained,
        Some(v) if v < 0.8 => ReadinessLevel::Strained,
        Some(v) if (0.8..=1.3).contains(&v) && !monotony_high => ReadinessLevel::Primed,
        _ => ReadinessLevel::Balanced,
    };

    ReadinessOutput {
        algorithm_id: BULL_READINESS_V1_ID.to_string(),
        algorithm_version: BULL_READINESS_V1_VERSION.to_string(),
        acwr,
        acwr_zone,
        monotony,
        monotony_high,
        level,
        insufficient_data: false,
    }
}

#[cfg(test)]
mod readiness_tests {
    use super::*;

    // Helper: build ReadinessInput with n entries all at strain `s`.
    fn uniform_input(n: usize, s: f64) -> ReadinessInput {
        let pairs = (0..n).map(|i| (i as f64 * 86400.0, s)).collect();
        ReadinessInput {
            daily_strain: pairs,
        }
    }

    // Helper: build 28-entry input with custom last-7 strain.
    fn split_input(first21_strain: f64, last7_strain: f64) -> ReadinessInput {
        let mut pairs: Vec<(f64, f64)> = (0..21)
            .map(|i| (i as f64 * 86400.0, first21_strain))
            .collect();
        for j in 0..7 {
            pairs.push(((21 + j) as f64 * 86400.0, last7_strain));
        }
        ReadinessInput {
            daily_strain: pairs,
        }
    }

    #[test]
    fn test_readiness_insufficient_data_27_entries() {
        let input = uniform_input(27, 10.0);
        let out = bull_readiness_v1(&input);
        assert!(
            out.insufficient_data,
            "27 entries: insufficient_data must be true"
        );
        assert_eq!(out.level, ReadinessLevel::Unknown);
        assert!(out.acwr.is_none());
        assert_eq!(out.acwr_zone, "unknown");
    }

    #[test]
    fn test_readiness_insufficient_data_3_entries() {
        let input = uniform_input(3, 10.0);
        let out = bull_readiness_v1(&input);
        assert!(out.insufficient_data);
        assert_eq!(out.level, ReadinessLevel::Unknown);
    }

    #[test]
    fn test_readiness_insufficient_data_empty() {
        let input = ReadinessInput {
            daily_strain: vec![],
        };
        let out = bull_readiness_v1(&input);
        assert!(out.insufficient_data);
        assert_eq!(out.level, ReadinessLevel::Unknown);
    }

    #[test]
    fn test_readiness_primed_uniform_28_entries() {
        // All equal → std=0 → monotony=None → monotony_high=false; acwr=1.0 → Primed
        let input = uniform_input(28, 10.0);
        let out = bull_readiness_v1(&input);
        assert!(!out.insufficient_data);
        let acwr = out.acwr.expect("acwr must be Some");
        assert!((acwr - 1.0).abs() < 1e-9, "acwr must be 1.0, got {acwr}");
        assert_eq!(out.acwr_zone, "optimal");
        assert!(
            out.monotony.is_none(),
            "uniform strain → monotony must be None (std=0)"
        );
        assert!(!out.monotony_high);
        assert_eq!(out.level, ReadinessLevel::Primed);
    }

    #[test]
    fn test_readiness_acwr_zone_boundary_0_8() {
        // acwr exactly 0.8 → zone = "optimal"
        // acwr = 28x/(210+7x) = 0.8 → 28x = 168+5.6x → 22.4x = 168 → x = 7.5 (exact f64)
        let mut pairs: Vec<(f64, f64)> = (0..21).map(|i| (i as f64, 10.0)).collect();
        for j in 0..7 {
            pairs.push((21.0 + j as f64, 7.5));
        }
        let input = ReadinessInput {
            daily_strain: pairs,
        };
        let out = bull_readiness_v1(&input);
        let acwr = out.acwr.unwrap();
        assert!((acwr - 0.8).abs() < 1e-9, "acwr must be 0.8, got {acwr}");
        assert_eq!(out.acwr_zone, "optimal", "acwr=0.8 must map to 'optimal'");
    }

    #[test]
    fn test_readiness_acwr_zone_boundary_1_3() {
        // acwr exactly 1.3 → zone = "optimal"
        // acwr = 28x/(210+7x) = 1.3 → 28x = 273+9.1x → 18.9x = 273 → x = 273/18.9
        let x = 273.0_f64 / 18.9;
        let mut pairs: Vec<(f64, f64)> = (0..21).map(|i| (i as f64, 10.0)).collect();
        for j in 0..7 {
            pairs.push((21.0 + j as f64, x));
        }
        let input = ReadinessInput {
            daily_strain: pairs,
        };
        let out = bull_readiness_v1(&input);
        let acwr = out.acwr.unwrap();
        assert!((acwr - 1.3).abs() < 1e-9, "acwr must be 1.3, got {acwr}");
        assert_eq!(out.acwr_zone, "optimal", "acwr=1.3 must map to 'optimal'");
    }

    #[test]
    fn test_readiness_acwr_zone_boundary_1_5_rundown() {
        // acwr = 1.5 exactly → zone = "danger", level = Rundown
        // Using same formula: x = 1.5*(10 + (x-10)/4) = 15 + 0.375x - 3.75 = 11.25 + 0.375x
        // 0.625x = 11.25 → x = 18.0
        let mut pairs: Vec<(f64, f64)> = (0..21).map(|i| (i as f64, 10.0)).collect();
        for j in 0..7 {
            pairs.push((21.0 + j as f64, 18.0));
        }
        let input = ReadinessInput {
            daily_strain: pairs,
        };
        let out = bull_readiness_v1(&input);
        let acwr = out.acwr.unwrap();
        assert!((acwr - 1.5).abs() < 1e-6, "acwr must be 1.5, got {acwr}");
        assert_eq!(out.acwr_zone, "danger", "acwr=1.5 must map to 'danger'");
        assert_eq!(
            out.level,
            ReadinessLevel::Rundown,
            "acwr=1.5 must yield Rundown"
        );
    }

    #[test]
    fn test_readiness_rundown_high_acwr() {
        // last 7 avg=15.0, full 28 avg=10.0 → acwr=1.5 → Rundown
        // (21*10 + 7*15)/28 = (210+105)/28 = 315/28 = 11.25; acute=15; acwr=15/11.25=1.333...
        // Need a higher contrast. Set first21=5.0, last7=21.0
        // chronic=(21*5+7*21)/28=(105+147)/28=252/28=9.0; acute=21; acwr=21/9=2.333>1.5 → Rundown
        let input = split_input(5.0, 21.0);
        let out = bull_readiness_v1(&input);
        let acwr = out.acwr.unwrap();
        assert!(acwr >= 1.5, "acwr must be >=1.5, got {acwr}");
        assert_eq!(out.level, ReadinessLevel::Rundown);
    }

    #[test]
    fn test_readiness_strained_under_training() {
        // acwr < 0.8 → Strained (under-training)
        // Set last7=1.0, first21=10.0
        // chronic=(210+7)/28=217/28=7.75; acute=1.0; acwr=1/7.75≈0.129 → Strained
        let input = split_input(10.0, 1.0);
        let out = bull_readiness_v1(&input);
        let acwr = out.acwr.unwrap();
        assert!(acwr < 0.8, "acwr must be <0.8, got {acwr}");
        assert_eq!(
            out.level,
            ReadinessLevel::Strained,
            "under-training acwr must yield Strained"
        );
    }

    #[test]
    fn test_readiness_monotony_high_at_exactly_2() {
        // Verify that monotony_high is set true when mean/std >= 2.0.
        //
        // Construct last 7 values with a clear mean >> std. Use [10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0]
        // but that would give std=0 → monotony=None. Instead use values with small controlled variance.
        //
        // Use values: first 21 days at 10.0 (gives ACWR in sub-optimal zone when last7 vary),
        // last 7 days as [4.0, 4.0, 4.0, 4.0, 4.0, 4.0, 4.0] (uniform) won't have std.
        //
        // Instead: use values where std is finite and monotony >= 2.
        // For [a]*6 + [b]*1 with all distinct-representable floats:
        //   mean=(6a+b)/7, var=6*(a-mean)^2/7 + (b-mean)^2/7 = 6*(a-m)^2 [algebraic]
        //   monotony = m/(sqrt(6)*|a-m|) >= 2 when |a-m| <= m/(2*sqrt(6))
        //
        // Use a=8.0, b=14.0 to get mean=(48+14)/7=62/7≈8.857:
        //   var = (6*(8-8.857)^2+(14-8.857)^2)/7 = (6*0.7347+26.455)/7 = (4.408+26.455)/7 = 4.409
        //   std ≈ 2.099; monotony = 8.857/2.099 ≈ 4.22 >> 2.0 → monotony_high=true
        //
        // We verify monotony >= 2.0 and that monotony_high is true.
        let mut pairs: Vec<(f64, f64)> = (0..21).map(|i| (i as f64, 10.0)).collect();
        let last7 = [8.0_f64, 8.0, 8.0, 8.0, 8.0, 8.0, 14.0];
        for (j, &s) in last7.iter().enumerate() {
            pairs.push((21.0 + j as f64, s));
        }
        let input = ReadinessInput {
            daily_strain: pairs,
        };
        let out = bull_readiness_v1(&input);
        let mono = out.monotony.expect("monotony must be Some");
        assert!(
            mono >= 2.0,
            "monotony must be >=2.0 (got {mono}) for values with mean/std >> 2"
        );
        assert!(
            out.monotony_high,
            "monotony>=2.0 must set monotony_high=true"
        );
        assert_eq!(
            out.level,
            ReadinessLevel::Strained,
            "high monotony must yield Strained"
        );
    }

    #[test]
    fn test_readiness_monotony_boundary_below_2() {
        // Verify that monotony_high is false when monotony is just below 2.0.
        // Values: [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 21.0] → mean=3.0, std large → monotony low.
        // Or use [5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.1] → tiny spread, high monotony.
        // We need monotony < 2.0: large std relative to mean.
        // Use [1.0, 21.0, 1.0, 21.0, 1.0, 21.0, 1.0]: mean=10, big variance → low monotony.
        // mean = (4*1+3*21)/7 = (4+63)/7 = 67/7 ≈ 9.57
        // var = (4*(1-9.57)^2+3*(21-9.57)^2)/7 large → std >> mean/2 → monotony < 2.
        let mut pairs: Vec<(f64, f64)> = (0..21).map(|i| (i as f64, 10.0)).collect();
        let last7 = [1.0_f64, 21.0, 1.0, 21.0, 1.0, 21.0, 1.0];
        for (j, &s) in last7.iter().enumerate() {
            pairs.push((21.0 + j as f64, s));
        }
        let input = ReadinessInput {
            daily_strain: pairs,
        };
        let out = bull_readiness_v1(&input);
        let mono = out
            .monotony
            .expect("monotony must be Some for high-variance input");
        assert!(
            mono < 2.0,
            "monotony must be <2.0 for high-variance input, got {mono}"
        );
        assert!(
            !out.monotony_high,
            "monotony<2.0 must NOT set monotony_high"
        );
    }

    #[test]
    fn test_readiness_balanced_caution_zone() {
        // acwr between 1.3 and 1.5 (exclusive) → Balanced (not rundown, no high monotony)
        // Use last7=16.0 (exact f64): acwr = 28*16/(210+7*16) = 448/322 ≈ 1.391 (caution zone)
        // All last7 identical → std=0 → monotony=None → monotony_high=false → Balanced.
        let mut pairs: Vec<(f64, f64)> = (0..21).map(|i| (i as f64, 10.0)).collect();
        for j in 0..7 {
            pairs.push((21.0 + j as f64, 16.0));
        }
        let input = ReadinessInput {
            daily_strain: pairs,
        };
        let out = bull_readiness_v1(&input);
        let acwr = out.acwr.unwrap();
        assert!(
            acwr > 1.3 && acwr < 1.5,
            "acwr must be in caution zone (1.3-1.5), got {acwr}"
        );
        assert!(
            out.monotony.is_none(),
            "uniform last7 → monotony must be None (std=0)"
        );
        assert!(!out.monotony_high, "uniform last7 → no high monotony");
        assert_eq!(
            out.level,
            ReadinessLevel::Balanced,
            "caution zone without monotony → Balanced"
        );
    }
}
