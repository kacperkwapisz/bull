use std::{
    fs,
    process::{Command, Stdio},
};

#[test]
fn algo_benchmark_reports_runtime_coverage_and_label_error() {
    let output_dir = tempfile::tempdir().unwrap();
    let output_path = output_dir.path().join("sleep-benchmark.json");

    let status = Command::new(env!("CARGO_BIN_EXE_bull-algo-benchmark"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdout(Stdio::null())
        .args([
            "--algorithm",
            "bull.sleep.v0",
            "--input",
            "fixtures/synthetic/sleep_bull_v0_hand_derived.json",
            "--label-value",
            "86.0",
            "--label-unit",
            "score_0_to_100",
            "--label-source",
            "manual",
            "--label-provenance-json",
            r#"{"entry":"typed_by_user","official_labels_are_labels":true}"#,
            "--max-absolute-error",
            "2.0",
            "--output",
            output_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(output_path).unwrap()).unwrap();
    assert_eq!(report["schema"], "bull.algo-benchmark-report.v1");
    assert_eq!(report["pass"], true);
    assert!(report["runtime_ms"].as_f64().unwrap() >= 0.0);
    assert_eq!(report["data_coverage"]["input_ids_count"], 1);
    assert_eq!(report["data_coverage"]["output_present"], true);
    assert_eq!(report["score_field"]["field"], "output.score_0_to_100");
    assert_eq!(report["score_field"]["unit"], "score_0_to_100");
    assert_eq!(
        report["label_comparison"]["official_labels_are_labels"],
        true
    );
    assert_close(
        report["label_comparison"]["prediction_value"]
            .as_f64()
            .unwrap(),
        84.875,
    );
    assert_close(
        report["label_comparison"]["signed_error"].as_f64().unwrap(),
        -1.125,
    );
    assert_close(
        report["label_comparison"]["absolute_error"]
            .as_f64()
            .unwrap(),
        1.125,
    );
    assert_eq!(report["label_comparison"]["error_within_threshold"], true);
    assert!(report["next_actions"].as_array().unwrap().is_empty());
}

#[test]
fn algo_benchmark_rejects_private_api_label_source_in_report() {
    let output_dir = tempfile::tempdir().unwrap();
    let output_path = output_dir.path().join("private-label-benchmark.json");

    let status = Command::new(env!("CARGO_BIN_EXE_bull-algo-benchmark"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdout(Stdio::null())
        .args([
            "--algorithm",
            "bull.sleep.v0",
            "--input",
            "fixtures/synthetic/sleep_bull_v0_hand_derived.json",
            "--label-value",
            "86.0",
            "--label-unit",
            "score_0_to_100",
            "--label-source",
            "private_api_replay",
            "--label-provenance-json",
            r#"{"source":"not_allowed","official_labels_are_labels":true}"#,
            "--output",
            output_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(!status.success());

    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(output_path).unwrap()).unwrap();
    assert_eq!(report["pass"], false);
    assert_eq!(
        report["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|error| error == "unsupported_label_source:private_api_replay"),
        true
    );
    assert_eq!(
        report["label_comparison"]["official_labels_are_labels"],
        true
    );
    assert!(
        report["next_actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action["reason"] == "unsupported_label_source")
    );
}

#[test]
fn algo_benchmark_fails_label_threshold_without_hiding_error_value() {
    let output_dir = tempfile::tempdir().unwrap();
    let output_path = output_dir.path().join("threshold-benchmark.json");

    let status = Command::new(env!("CARGO_BIN_EXE_bull-algo-benchmark"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdout(Stdio::null())
        .args([
            "--algorithm",
            "bull.strain.v0",
            "--input",
            "fixtures/synthetic/strain_bull_v0_hand_derived.json",
            "--label-value",
            "10.0",
            "--label-unit",
            "score_0_to_21",
            "--label-source",
            "synthetic",
            "--label-provenance-json",
            r#"{"fixture":"threshold","official_labels_are_labels":true}"#,
            "--max-absolute-error",
            "1.0",
            "--output",
            output_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(!status.success());

    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(output_path).unwrap()).unwrap();
    assert_eq!(report["pass"], false);
    assert_close(
        report["label_comparison"]["prediction_value"]
            .as_f64()
            .unwrap(),
        8.049999999999999,
    );
    assert_close(
        report["label_comparison"]["absolute_error"]
            .as_f64()
            .unwrap(),
        1.950000000000001,
    );
    assert_eq!(report["label_comparison"]["error_within_threshold"], false);
    assert!(report["errors"].as_array().unwrap().iter().any(|error| {
        error
            .as_str()
            .unwrap()
            .starts_with("label_error_exceeds_threshold:")
    }));
    assert!(
        report["next_actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action["reason"] == "label_error_exceeds_threshold")
    );
}

#[test]
fn algo_benchmark_reference_comparison_reports_runtime_and_coverage() {
    let output_dir = tempfile::tempdir().unwrap();
    let output_path = output_dir.path().join("reference-comparison.json");

    let status = Command::new(env!("CARGO_BIN_EXE_bull-algo-benchmark"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdout(Stdio::null())
        .args([
            "--compare-reference",
            "--family",
            "hrv",
            "--input",
            "fixtures/synthetic/hrv_bull_v0_hand_derived.json",
            "--output",
            output_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(output_path).unwrap()).unwrap();
    assert_eq!(report["schema"], "bull.algorithm-comparison-report.v1");
    assert_eq!(report["pass"], true);
    assert!(report["runtime_ms"].as_f64().unwrap() >= 0.0);
    assert_eq!(report["data_coverage"]["input_ids_count"], 1);
    assert_eq!(report["data_coverage"]["output_present"], true);
    assert_eq!(report["deltas"].as_array().unwrap().len(), 4);
    assert!(report["next_actions"].as_array().unwrap().is_empty());
}

#[test]
fn algo_benchmark_reference_comparison_supports_sleep_v1_input() {
    let output_dir = tempfile::tempdir().unwrap();
    let input_path = output_dir.path().join("sleep-v1-input.json");
    let output_path = output_dir.path().join("sleep-v1-reference-comparison.json");
    fs::write(
        &input_path,
        serde_json::to_string_pretty(&serde_json::json!({
            "start_time": "2026-05-27T22:30:00Z",
            "end_time": "2026-05-28T06:30:00Z",
            "sleep_duration_minutes": 420.0,
            "sleep_need_minutes": 480.0,
            "time_in_bed_minutes": 480.0,
            "midpoint_deviation_minutes": 30.0,
            "disturbance_count": 2,
            "sleep_latency_minutes": 18.0,
            "wake_after_sleep_onset_minutes": 42.0,
            "wake_episode_count": 2,
            "stage_minutes": {
                "awake": 60.0,
                "core": 210.0,
                "deep": 90.0,
                "rem": 120.0
            },
            "heart_rate_dip_percent": 12.5,
            "input_ids": ["sleep-v1-reference-comparison"],
            "model_status": {
                "sleep_permission_granted": true,
                "imported_platform_sleep_nights": 10,
                "trusted_bull_sleep_nights": 2,
                "motion_coverage_fraction": 0.94,
                "heart_rate_coverage_fraction": 0.82
            },
            "rolling_sleep_debt_minutes": 90.0,
            "bedtime_deviation_minutes": 20.0,
            "wake_time_deviation_minutes": 15.0,
            "sleep_hr_average_bpm": 61.0,
            "sleep_hr_min_bpm": 54.0,
            "sleep_hr_trend_bpm_per_hour": -1.2,
            "naps_minutes": 25.0,
            "prior_day_strain": 8.5,
            "data_coverage_fraction": 0.92
        }))
        .unwrap(),
    )
    .unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_bull-algo-benchmark"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdout(Stdio::null())
        .args([
            "--compare-reference",
            "--family",
            "sleep",
            "--algorithm",
            "bull.sleep.v1",
            "--input",
            input_path.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(output_path).unwrap()).unwrap();
    assert_eq!(report["schema"], "bull.algorithm-comparison-report.v1");
    assert_eq!(report["pass"], true);
    assert_eq!(report["bull_algorithm_id"], "bull.sleep.v1");
    assert_eq!(
        report["reference_algorithm_id"],
        "reference.sleep.actigraphy_summary.v1"
    );
    assert_eq!(report["data_coverage"]["input_ids_count"], 1);
    assert_eq!(
        report["data_coverage"]["bull_output_data_coverage_fraction"],
        0.92
    );
    assert!(
        report["non_comparable_fields"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field.as_str().unwrap().contains("stage_segments"))
    );
}

#[test]
fn algo_benchmark_reference_comparison_supports_stress() {
    let output_dir = tempfile::tempdir().unwrap();
    let output_path = output_dir.path().join("stress-reference-comparison.json");

    let status = Command::new(env!("CARGO_BIN_EXE_bull-algo-benchmark"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdout(Stdio::null())
        .args([
            "--compare-reference",
            "--family",
            "stress",
            "--input",
            "fixtures/synthetic/stress_bull_v0_hand_derived.json",
            "--output",
            output_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(output_path).unwrap()).unwrap();
    assert_eq!(report["schema"], "bull.algorithm-comparison-report.v1");
    assert_eq!(report["family"], "stress");
    assert_eq!(report["pass"], true);
    assert_eq!(
        report["reference_algorithm_id"],
        "reference.stress.hrv_hr_proxy.v1"
    );
    assert_eq!(report["deltas"].as_array().unwrap().len(), 2);
    assert!(report["runtime_ms"].as_f64().unwrap() >= 0.0);
}

#[test]
fn algo_benchmark_reference_comparison_accepts_external_sleep_report() {
    let output_dir = tempfile::tempdir().unwrap();
    let reference_path = output_dir.path().join("external-sleep-reference.json");
    let output_path = output_dir.path().join("external-sleep-comparison.json");
    fs::write(
        &reference_path,
        r#"{
  "schema": "bull.reference-algo-report.v1",
  "family": "sleep",
  "algorithm_id": "reference.sleep.ggir_summary.v1",
  "algorithm_version": "1.0.0",
  "start_time": "2026-05-27T22:30:00Z",
  "end_time": "2026-05-28T06:30:00Z",
  "output": {
    "time_in_bed_minutes": 480.0,
    "sleep_minutes": 420.0,
    "wake_minutes": 60.0,
    "sleep_efficiency_fraction": 0.875,
    "wake_after_sleep_onset_minutes": 60.0,
    "disturbance_count": 4,
    "fragmentation_index_per_hour": 0.5714285714285714
  },
  "quality_flags": [],
  "errors": [],
  "provenance": {
    "provider_kind": "external_reference",
    "external_provider": "external.ggir.sleep",
    "output_units": {
      "time_in_bed_minutes": "minutes",
      "sleep_minutes": "minutes",
      "wake_minutes": "minutes",
      "sleep_efficiency_fraction": "fraction",
      "wake_after_sleep_onset_minutes": "minutes",
      "disturbance_count": "count",
      "fragmentation_index_per_hour": "events_per_hour"
    }
  }
}"#,
    )
    .unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_bull-algo-benchmark"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdout(Stdio::null())
        .args([
            "--compare-reference",
            "--family",
            "sleep",
            "--input",
            "fixtures/synthetic/sleep_bull_v0_hand_derived.json",
            "--reference-report",
            reference_path.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(output_path).unwrap()).unwrap();
    assert_eq!(report["schema"], "bull.algorithm-comparison-report.v1");
    assert_eq!(report["family"], "sleep");
    assert_eq!(report["pass"], true);
    assert_eq!(report["reference_contract_valid"], true);
    assert_eq!(report["shared_fields_ready"], true);
    assert_eq!(
        report["reference_algorithm_id"],
        "reference.sleep.ggir_summary.v1"
    );
    assert_eq!(report["deltas"].as_array().unwrap().len(), 7);
    assert_eq!(
        report["provenance"]["comparison_policy"],
        "external_sleep_reference_shared_fields"
    );
    assert!(report["runtime_ms"].as_f64().unwrap() >= 0.0);
}

#[test]
fn algo_benchmark_external_reference_report_requires_unit_contract() {
    let output_dir = tempfile::tempdir().unwrap();
    let reference_path = output_dir.path().join("bad-external-sleep-reference.json");
    let output_path = output_dir.path().join("bad-external-sleep-comparison.json");
    fs::write(
        &reference_path,
        r#"{
  "schema": "bull.reference-algo-report.v1",
  "family": "sleep",
  "algorithm_id": "reference.sleep.ggir_summary.v1",
  "algorithm_version": "1.0.0",
  "start_time": "2026-05-27T22:30:00Z",
  "end_time": "2026-05-28T06:30:00Z",
  "output": {
    "time_in_bed_minutes": 480.0,
    "sleep_minutes": 420.0,
    "sleep_efficiency_fraction": 0.875
  },
  "quality_flags": [],
  "errors": [],
  "provenance": {}
}"#,
    )
    .unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_bull-algo-benchmark"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdout(Stdio::null())
        .args([
            "--compare-reference",
            "--family",
            "sleep",
            "--input",
            "fixtures/synthetic/sleep_bull_v0_hand_derived.json",
            "--reference-report",
            reference_path.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(!status.success());

    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(output_path).unwrap()).unwrap();
    assert_eq!(report["schema"], "bull.algorithm-comparison-report.v1");
    assert_eq!(report["pass"], false);
    assert_eq!(report["reference_contract_valid"], false);
    assert_eq!(report["bull_output_ready"], true);
    assert_eq!(report["reference_output_ready"], true);
    assert_eq!(report["shared_fields_ready"], false);
    assert!(
        report["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|error| { error == "reference_contract:missing_output_unit:time_in_bed_minutes" })
    );
    assert!(
        report["next_actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action["reason"] == "reference_output_unit_missing")
    );
    assert!(
        report["next_actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action["reason"] == "reference_provenance_missing")
    );
}

#[test]
fn algo_benchmark_reference_comparison_reports_next_actions_when_outputs_missing() {
    let output_dir = tempfile::tempdir().unwrap();
    let input_path = output_dir.path().join("bad-hrv.json");
    let output_path = output_dir.path().join("bad-reference-comparison.json");
    fs::write(
        &input_path,
        r#"{
  "start_time": "2026-05-27T00:00:00Z",
  "end_time": "2026-05-27T00:01:00Z",
  "rr_intervals_ms": [100.0],
  "input_ids": ["synthetic.bad.hrv"]
}"#,
    )
    .unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_bull-algo-benchmark"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdout(Stdio::null())
        .args([
            "--compare-reference",
            "--family",
            "hrv",
            "--input",
            input_path.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(!status.success());

    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(output_path).unwrap()).unwrap();
    assert_eq!(report["schema"], "bull.algorithm-comparison-report.v1");
    assert_eq!(report["pass"], false);
    assert!(
        report["next_actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action["reason"] == "comparison_outputs_missing")
    );
    assert!(
        report["next_actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action["reason"] == "bull_algorithm_error")
    );
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "expected {expected}, got {actual}"
    );
}
