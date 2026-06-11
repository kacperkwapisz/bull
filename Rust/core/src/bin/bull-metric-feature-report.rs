use std::fs;

use bull_core::{
    BullError,
    bridge::{BRIDGE_REQUEST_SCHEMA, BridgeRequest, handle_bridge_request},
    report::write_json_report,
    tool_args::{args, flag, path_value, value},
};
use serde_json::{Map, Value, json};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

fn run() -> bull_core::BullResult<()> {
    let args = args();
    let method = value(&args, "--method")?
        .or(value(&args, "--report")?)
        .ok_or_else(|| BullError::message("--method is required"))?;
    let method = metric_bridge_method(&method)?;
    let database_path = path_value(&args, "--database")?
        .ok_or_else(|| BullError::message("--database is required"))?;
    let output = path_value(&args, "--output")?;

    let mut request_args = Map::new();
    request_args.insert(
        "database_path".to_string(),
        json!(database_path.display().to_string()),
    );
    insert_string_arg(&mut request_args, &args, "--start", "start")?;
    insert_string_arg(&mut request_args, &args, "--end", "end")?;
    insert_usize_arg(
        &mut request_args,
        &args,
        "--min-owned-captures",
        "min_owned_captures",
    )?;
    insert_usize_arg(
        &mut request_args,
        &args,
        "--max-candidate-fields",
        "max_candidate_fields",
    )?;
    insert_i64_arg(
        &mut request_args,
        &args,
        "--manual-step-delta",
        "manual_step_delta",
    )?;
    insert_i64_arg(
        &mut request_args,
        &args,
        "--official-whoop-step-delta",
        "official_whoop_step_delta",
    )?;
    insert_i64_arg(
        &mut request_args,
        &args,
        "--step-delta-tolerance",
        "tolerance_steps",
    )?;
    insert_i64_arg(
        &mut request_args,
        &args,
        "--start-time-unix-ms",
        "start_time_unix_ms",
    )?;
    insert_i64_arg(
        &mut request_args,
        &args,
        "--end-time-unix-ms",
        "end_time_unix_ms",
    )?;
    insert_bool_flag(
        &mut request_args,
        &args,
        "--require-trusted-evidence",
        "require_trusted_evidence",
    );
    insert_bool_flag(
        &mut request_args,
        &args,
        "--require-baseline",
        "require_baseline",
    );
    insert_bool_flag(
        &mut request_args,
        &args,
        "--persist-algorithm-run",
        "persist_algorithm_run",
    );
    insert_bool_flag(
        &mut request_args,
        &args,
        "--history-import-in-progress",
        "history_import_in_progress",
    );
    insert_bool_flag(&mut request_args, &args, "--write-metric", "write_metric");

    for (arg, field) in [
        ("--resting-start", "resting_start"),
        ("--resting-end", "resting_end"),
        ("--hrv-start", "hrv_start"),
        ("--hrv-end", "hrv_end"),
        ("--hrv-baseline-start", "hrv_baseline_start"),
        ("--hrv-baseline-end", "hrv_baseline_end"),
        ("--sleep-start", "sleep_start"),
        ("--sleep-end", "sleep_end"),
        ("--prior-strain-start", "prior_strain_start"),
        ("--prior-strain-end", "prior_strain_end"),
        ("--algorithm-run-id", "algorithm_run_id"),
        ("--algorithm-id", "algorithm_id"),
        ("--algorithm-version", "algorithm_version"),
        ("--capture-kind", "capture_kind"),
        ("--date-key", "date_key"),
        ("--timezone", "timezone"),
        ("--profile-sex", "profile_sex"),
        ("--provided-vitals-source", "provided_vitals_source"),
        (
            "--provided-vitals-provenance-json",
            "provided_vitals_provenance_json",
        ),
    ] {
        insert_string_arg(&mut request_args, &args, arg, field)?;
    }
    for (arg, field) in [
        (
            "--min-rr-intervals-to-compute",
            "min_rr_intervals_to_compute",
        ),
        (
            "--hrv-min-rr-intervals-to-compute",
            "hrv_min_rr_intervals_to_compute",
        ),
        ("--baseline-min-days", "baseline_min_days"),
        ("--hrv-baseline-min-days", "hrv_baseline_min_days"),
        ("--resting-baseline-min-days", "resting_baseline_min_days"),
        ("--min-samples", "min_sample_count"),
        ("--min-heart-rate-samples", "min_heart_rate_samples"),
        ("--profile-age-years", "profile_age_years"),
        (
            "--prior-strain-resting-baseline-min-days",
            "prior_strain_resting_baseline_min_days",
        ),
        ("--min-step-samples", "min_sample_count"),
        ("--min-peak-spacing-samples", "min_peak_spacing_samples"),
    ] {
        insert_usize_arg(&mut request_args, &args, arg, field)?;
    }
    for (arg, field) in [
        ("--resting-hr-bpm", "resting_hr_bpm"),
        ("--max-hr-bpm", "max_hr_bpm"),
        ("--profile-weight-kg", "profile_weight_kg"),
        ("--prior-strain-max-hr-bpm", "prior_strain_max_hr_bpm"),
        ("--sleep-need-minutes", "sleep_need_minutes"),
        ("--low-motion-threshold", "low_motion_threshold_0_to_1"),
        (
            "--disturbance-motion-threshold",
            "disturbance_motion_threshold_0_to_1",
        ),
        (
            "--target-midpoint-minutes-since-midnight",
            "target_midpoint_minutes_since_midnight",
        ),
        ("--respiratory-rate-rpm", "respiratory_rate_rpm"),
        (
            "--respiratory-rate-baseline-rpm",
            "respiratory_rate_baseline_rpm",
        ),
        (
            "--official-whoop-respiratory-rate-rpm",
            "official_whoop_respiratory_rate_rpm",
        ),
        ("--respiratory-rate-tolerance-rpm", "tolerance_rpm"),
        (
            "--official-whoop-oxygen-saturation-percent",
            "official_whoop_oxygen_saturation_percent",
        ),
        (
            "--official-whoop-spo2-percent",
            "official_whoop_oxygen_saturation_percent",
        ),
        ("--oxygen-saturation-tolerance-percent", "tolerance_percent"),
        ("--spo2-tolerance-percent", "tolerance_percent"),
        (
            "--official-whoop-skin-temperature-delta-c",
            "official_whoop_skin_temperature_delta_c",
        ),
        (
            "--official-whoop-temperature-delta-c",
            "official_whoop_skin_temperature_delta_c",
        ),
        ("--skin-temperature-tolerance-c", "tolerance_c"),
        ("--temperature-tolerance-c", "tolerance_c"),
        ("--skin-temp-delta-c", "skin_temp_delta_c"),
        (
            "--official-whoop-resting-hr-bpm",
            "official_whoop_resting_hr_bpm",
        ),
        ("--rhr-tolerance-bpm", "tolerance_bpm"),
        (
            "--official-whoop-hrv-rmssd-ms",
            "official_whoop_hrv_rmssd_ms",
        ),
        ("--hrv-tolerance-ms", "tolerance_ms"),
        ("--official-whoop-active-kcal", "official_whoop_active_kcal"),
        (
            "--official-whoop-resting-kcal",
            "official_whoop_resting_kcal",
        ),
        ("--official-whoop-total-kcal", "official_whoop_total_kcal"),
        ("--energy-tolerance-kcal", "tolerance_kcal"),
        ("--energy-relative-tolerance", "relative_tolerance_fraction"),
        ("--motion-sample-rate-hz", "sample_rate_hz"),
        ("--motion-peak-threshold-i16", "peak_threshold_i16"),
    ] {
        insert_f64_arg(&mut request_args, &args, arg, field)?;
    }
    merge_args_json(&mut request_args, &args)?;

    let response = handle_bridge_request(BridgeRequest {
        schema: BRIDGE_REQUEST_SCHEMA.to_string(),
        request_id: "metric-feature-report-cli".to_string(),
        method: method.to_string(),
        args: Value::Object(request_args),
    });
    if !response.ok {
        let message = response
            .error
            .map(|error| format!("{}: {}", error.code, error.message))
            .unwrap_or_else(|| "metric feature report failed".to_string());
        return Err(BullError::message(message));
    }
    let report = response
        .result
        .ok_or_else(|| BullError::message("metric feature report missing result"))?;
    let pass = report.get("pass").and_then(Value::as_bool).unwrap_or(false);

    write_json_report(&report, output.as_deref())?;
    if pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

fn metric_bridge_method(value: &str) -> bull_core::BullResult<&'static str> {
    match value.trim() {
        "motion" | "motion_features" | "metrics.motion_features" => Ok("metrics.motion_features"),
        "heart-rate" | "heart_rate" | "heart_rate_features" | "metrics.heart_rate_features" => {
            Ok("metrics.heart_rate_features")
        }
        "vital-event" | "vital_event" | "vital_event_features" | "metrics.vital_event_features" => {
            Ok("metrics.vital_event_features")
        }
        "step-discovery"
        | "step_discovery"
        | "step_packet_discovery"
        | "metrics.step_packet_discovery" => Ok("metrics.step_packet_discovery"),
        "step-validation"
        | "step_validation"
        | "step_capture_validation"
        | "metrics.step_capture_validation" => Ok("metrics.step_capture_validation"),
        "raw-motion-steps"
        | "raw_motion_steps"
        | "motion-step-estimate"
        | "motion_step_estimate"
        | "raw_motion_step_estimate"
        | "metrics.raw_motion_step_estimate" => Ok("metrics.raw_motion_step_estimate"),
        "step-counter-ingest"
        | "step_counter_ingest"
        | "steps-ingest"
        | "metrics.step_counter_ingest" => Ok("metrics.step_counter_ingest"),
        "step-rollup"
        | "step_counter_daily_rollup"
        | "steps-rollup"
        | "metrics.step_counter_daily_rollup" => Ok("metrics.step_counter_daily_rollup"),
        "step-hourly-rollup"
        | "step_counter_hourly_rollup"
        | "steps-hourly-rollup"
        | "hourly-step-rollup"
        | "hourly-steps-rollup"
        | "metrics.step_counter_hourly_rollup" => Ok("metrics.step_counter_hourly_rollup"),
        "activity-unavailable-status"
        | "activity_unavailable_status"
        | "activity-unavailable-daily-status"
        | "activity_unavailable_daily_status"
        | "step-unavailable-status"
        | "step_unavailable_status"
        | "steps-unavailable-status"
        | "steps_unavailable_status"
        | "metrics.activity_unavailable_daily_status" => {
            Ok("metrics.activity_unavailable_daily_status")
        }
        "energy-rollup"
        | "energy_daily_rollup"
        | "calorie-rollup"
        | "calories-rollup"
        | "metrics.energy_daily_rollup" => Ok("metrics.energy_daily_rollup"),
        "energy-unavailable-status"
        | "energy_unavailable_status"
        | "energy-unavailable-daily-status"
        | "energy_unavailable_daily_status"
        | "calorie-unavailable-status"
        | "calorie_unavailable_status"
        | "calories-unavailable-status"
        | "calories_unavailable_status"
        | "metrics.energy_unavailable_daily_status" => {
            Ok("metrics.energy_unavailable_daily_status")
        }
        "energy-hourly-rollup"
        | "energy_hourly_rollup"
        | "hourly-energy-rollup"
        | "hourly_calorie_rollup"
        | "hourly-calorie-rollup"
        | "calorie-hourly-rollup"
        | "metrics.energy_hourly_rollup" => Ok("metrics.energy_hourly_rollup"),
        "energy-validation"
        | "energy_capture_validation"
        | "calorie-validation"
        | "calories-validation"
        | "metrics.energy_capture_validation" => Ok("metrics.energy_capture_validation"),
        "hrv" | "hrv_features" | "metrics.hrv_features" => Ok("metrics.hrv_features"),
        "hrv-validation"
        | "hrv_capture_validation"
        | "hrv-capture-validation"
        | "metrics.hrv_capture_validation" => Ok("metrics.hrv_capture_validation"),
        "respiratory-rate-validation"
        | "respiratory_rate_validation"
        | "respiratory-rate-capture-validation"
        | "respiratory_rate_capture_validation"
        | "rr-validation"
        | "metrics.respiratory_rate_capture_validation" => {
            Ok("metrics.respiratory_rate_capture_validation")
        }
        "oxygen-saturation-validation"
        | "oxygen_saturation_validation"
        | "oxygen-saturation-capture-validation"
        | "oxygen_saturation_capture_validation"
        | "spo2-validation"
        | "spo2_capture_validation"
        | "metrics.oxygen_saturation_capture_validation" => {
            Ok("metrics.oxygen_saturation_capture_validation")
        }
        "temperature-validation"
        | "temperature_capture_validation"
        | "temperature-capture-validation"
        | "skin-temperature-validation"
        | "skin_temperature_capture_validation"
        | "skin-temperature-capture-validation"
        | "temp-validation"
        | "metrics.temperature_capture_validation" => Ok("metrics.temperature_capture_validation"),
        "recovery-sensors"
        | "recovery_sensor_discovery"
        | "health-sensors"
        | "health_sensor_discovery"
        | "metrics.recovery_sensor_discovery" => Ok("metrics.recovery_sensor_discovery"),
        "recovery-sensor-rollup"
        | "recovery_sensor_rollup"
        | "recovery-sensor-daily-rollup"
        | "recovery_sensor_daily_rollup"
        | "recovery-vitals-rollup"
        | "recovery_vitals_rollup"
        | "metrics.recovery_sensor_daily_rollup" => Ok("metrics.recovery_sensor_daily_rollup"),
        "recovery-unavailable-status"
        | "recovery_unavailable_status"
        | "recovery-unavailable-daily-status"
        | "recovery_unavailable_daily_status"
        | "recovery-widget-status"
        | "recovery_widget_status"
        | "metrics.recovery_unavailable_daily_status" => {
            Ok("metrics.recovery_unavailable_daily_status")
        }
        "window" | "window_features" | "metric_window" | "metrics.window_features" => {
            Ok("metrics.window_features")
        }
        "resting-hr" | "resting_hr" | "resting_hr_features" | "metrics.resting_hr_features" => {
            Ok("metrics.resting_hr_features")
        }
        "rhr-rollup"
        | "resting-hr-rollup"
        | "resting_hr_daily_rollup"
        | "metrics.resting_hr_daily_rollup" => Ok("metrics.resting_hr_daily_rollup"),
        "rhr-validation"
        | "resting-hr-validation"
        | "resting_hr_capture_validation"
        | "metrics.resting_hr_capture_validation" => Ok("metrics.resting_hr_capture_validation"),
        "sleep-score"
        | "sleep_score"
        | "sleep_score_from_features"
        | "metrics.sleep_score_from_features" => Ok("metrics.sleep_score_from_features"),
        "recovery-score"
        | "recovery_score"
        | "recovery_score_from_features"
        | "metrics.recovery_score_from_features" => Ok("metrics.recovery_score_from_features"),
        "strain-score"
        | "strain_score"
        | "strain_score_from_features"
        | "metrics.strain_score_from_features" => Ok("metrics.strain_score_from_features"),
        "stress-score"
        | "stress_score"
        | "stress_score_from_features"
        | "metrics.stress_score_from_features" => Ok("metrics.stress_score_from_features"),
        other => Err(BullError::message(format!(
            "unsupported metric feature method: {other}"
        ))),
    }
}

fn insert_string_arg(
    object: &mut Map<String, Value>,
    args: &[String],
    name: &str,
    field: &str,
) -> bull_core::BullResult<()> {
    if let Some(value) = value(args, name)? {
        object.insert(field.to_string(), json!(value));
    }
    Ok(())
}

fn insert_bool_flag(object: &mut Map<String, Value>, args: &[String], name: &str, field: &str) {
    if flag(args, name) {
        object.insert(field.to_string(), json!(true));
    }
}

fn insert_usize_arg(
    object: &mut Map<String, Value>,
    args: &[String],
    name: &str,
    field: &str,
) -> bull_core::BullResult<()> {
    if let Some(raw) = value(args, name)? {
        let parsed = raw
            .parse::<usize>()
            .map_err(|source| BullError::message(format!("invalid {name}: {source}")))?;
        object.insert(field.to_string(), json!(parsed));
    }
    Ok(())
}

fn insert_i64_arg(
    object: &mut Map<String, Value>,
    args: &[String],
    name: &str,
    field: &str,
) -> bull_core::BullResult<()> {
    if let Some(raw) = value(args, name)? {
        let parsed = raw
            .parse::<i64>()
            .map_err(|source| BullError::message(format!("invalid {name}: {source}")))?;
        object.insert(field.to_string(), json!(parsed));
    }
    Ok(())
}

fn insert_f64_arg(
    object: &mut Map<String, Value>,
    args: &[String],
    name: &str,
    field: &str,
) -> bull_core::BullResult<()> {
    if let Some(raw) = value(args, name)? {
        let parsed = raw
            .parse::<f64>()
            .map_err(|source| BullError::message(format!("invalid {name}: {source}")))?;
        object.insert(field.to_string(), json!(parsed));
    }
    Ok(())
}

fn merge_args_json(
    object: &mut Map<String, Value>,
    args: &[String],
) -> bull_core::BullResult<()> {
    if let Some(raw) = value(args, "--args-json")? {
        merge_args_value(
            object,
            serde_json::from_str(&raw).map_err(|source| {
                BullError::message(format!("invalid --args-json object: {source}"))
            })?,
        )?;
    }
    if let Some(path) = path_value(args, "--args-json-file")? {
        let raw = fs::read_to_string(&path).map_err(|source| BullError::io(&path, source))?;
        merge_args_value(
            object,
            serde_json::from_str(&raw).map_err(|source| {
                BullError::message(format!("invalid --args-json-file object: {source}"))
            })?,
        )?;
    }
    Ok(())
}

fn merge_args_value(object: &mut Map<String, Value>, value: Value) -> bull_core::BullResult<()> {
    let Value::Object(extra) = value else {
        return Err(BullError::message(
            "metric feature extra args must be a JSON object",
        ));
    };
    for (key, value) in extra {
        object.insert(key, value);
    }
    Ok(())
}
