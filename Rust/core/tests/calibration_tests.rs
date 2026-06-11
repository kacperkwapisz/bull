use bull_core::{
    calibration::{
        CalibrationApplicationInput, CalibrationDataset, CalibrationOptions, apply_calibration,
        calibration_run_record, evaluate_linear_calibration,
    },
    store::{AlgorithmDefinitionRecord, CalibrationRunTimes, BullStore},
};

const FIXTURE: &str = include_str!("../fixtures/synthetic/recovery_calibration_linear.json");

#[test]
fn evaluates_linear_calibration_with_date_split_and_holdout_improvement() {
    let dataset: CalibrationDataset = serde_json::from_str(FIXTURE).unwrap();
    let report = evaluate_linear_calibration(&dataset, &default_options());

    assert!(report.pass, "{:?}", report.issues);
    assert!(report.dataset_valid);
    assert!(report.labels_valid);
    assert!(report.split_valid);
    assert!(report.model_fit_ready);
    assert!(report.train_metrics_ready);
    assert!(report.holdout_metrics_ready);
    assert!(report.holdout_improvement_valid);
    assert!(report.calibration_ready);
    assert!(report.next_actions.is_empty(), "{:?}", report.next_actions);
    assert_eq!(report.train_count, 3);
    assert_eq!(report.holdout_count, 2);
    assert_eq!(
        report.holdout_start.as_deref(),
        Some("2026-05-04T00:00:00Z")
    );
    assert!(report.leakage_checks.train_rows_before_split);
    assert!(report.leakage_checks.holdout_rows_at_or_after_split);
    assert!(report.leakage_checks.no_session_overlap);
    assert!(report.holdout_improved);

    let model = report.model.unwrap();
    assert_close(model.slope, 1.2);
    assert_close(model.intercept, -5.0);
    let uncalibrated_holdout = report.uncalibrated_holdout.as_ref().unwrap();
    let calibrated_holdout = report.calibrated_holdout.as_ref().unwrap();
    assert_close(uncalibrated_holdout.mae, 10.0);
    assert_close(calibrated_holdout.mae, 0.0);
    assert_close(calibrated_holdout.correlation.unwrap(), 1.0);
}

#[test]
fn split_boundary_goes_to_holdout_not_train() {
    let dataset: CalibrationDataset = serde_json::from_str(FIXTURE).unwrap();
    let report = evaluate_linear_calibration(&dataset, &default_options());

    assert_eq!(report.train_end.as_deref(), Some("2026-05-03T00:00:00Z"));
    assert_eq!(
        report.holdout_start.as_deref(),
        Some("2026-05-04T00:00:00Z")
    );
}

#[test]
fn detects_session_leakage_between_train_and_holdout() {
    let mut dataset: CalibrationDataset = serde_json::from_str(FIXTURE).unwrap();
    dataset.records[3].session_id = dataset.records[0].session_id.clone();
    let report = evaluate_linear_calibration(&dataset, &default_options());

    assert!(!report.pass);
    assert!(report.dataset_valid);
    assert!(report.labels_valid);
    assert!(!report.split_valid);
    assert!(!report.model_fit_ready);
    assert!(!report.calibration_ready);
    assert!(!report.leakage_checks.no_session_overlap);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.contains("session_id appears"))
    );
    assert!(
        report
            .next_actions
            .iter()
            .any(|action| action.reason == "session_leakage")
    );
}

#[test]
fn requires_label_provenance_and_allowed_label_source() {
    let mut dataset: CalibrationDataset = serde_json::from_str(FIXTURE).unwrap();
    dataset.records[0].label_provenance = serde_json::Value::Null;
    dataset.records[1].label_source = "private_api_replay".to_string();
    let report = evaluate_linear_calibration(&dataset, &default_options());

    assert!(!report.pass);
    assert!(report.dataset_valid);
    assert!(!report.labels_valid);
    assert!(report.split_valid);
    assert!(!report.model_fit_ready);
    assert!(!report.calibration_ready);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.contains("missing label_provenance"))
    );
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.contains("unsupported label_source"))
    );
    assert!(
        report
            .next_actions
            .iter()
            .any(|action| action.reason == "missing_label_provenance")
    );
    assert!(
        report
            .next_actions
            .iter()
            .any(|action| action.reason == "unsupported_label_source")
    );
}

#[test]
fn fails_when_holdout_does_not_improve() {
    let mut dataset: CalibrationDataset = serde_json::from_str(FIXTURE).unwrap();
    dataset.records[3].label = 70.0;
    dataset.records[4].label = 80.0;
    let report = evaluate_linear_calibration(&dataset, &default_options());

    assert!(!report.pass);
    assert!(report.dataset_valid);
    assert!(report.labels_valid);
    assert!(report.split_valid);
    assert!(report.model_fit_ready);
    assert!(report.train_metrics_ready);
    assert!(report.holdout_metrics_ready);
    assert!(!report.holdout_improvement_valid);
    assert!(!report.calibration_ready);
    assert!(!report.holdout_improved);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.contains("holdout MAE did not improve"))
    );
    assert!(
        report
            .next_actions
            .iter()
            .any(|action| action.reason == "holdout_not_improved")
    );
}

#[test]
fn calibration_run_persists_to_sqlite() {
    let dataset: CalibrationDataset = serde_json::from_str(FIXTURE).unwrap();
    let report = evaluate_linear_calibration(&dataset, &default_options());
    assert!(report.pass);

    let store = BullStore::open_in_memory().unwrap();
    store
        .upsert_algorithm_definition(&AlgorithmDefinitionRecord {
            algorithm_id: "bull.recovery.v0".to_string(),
            version: "0.1.0".to_string(),
            metric_family: "recovery".to_string(),
            display_name: "Bull Recovery v0".to_string(),
            implementation: "rust".to_string(),
            license: "UNLICENSED".to_string(),
            input_schema: "bull.recovery-input.v1".to_string(),
            output_schema: "bull.recovery-output.v1".to_string(),
            input_requirements_json: "{}".to_string(),
            params_json: "{}".to_string(),
            quality_gates_json: "[]".to_string(),
            status: "experimental".to_string(),
        })
        .unwrap();
    let record = calibration_run_record("calibration-run-1", &report).unwrap();
    assert!(store.insert_calibration_run(&record).unwrap());
    assert!(!store.insert_calibration_run(&record).unwrap());

    let saved = store.calibration_run("calibration-run-1").unwrap().unwrap();
    assert_eq!(saved.algorithm_id, "bull.recovery.v0");
    assert!(saved.params_json.contains("ordinary_least_squares_1d"));
    assert_eq!(
        store
            .calibration_runs_overlapping("2026-05-04T00:00:00Z", "2026-05-06T00:00:00Z")
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn applies_passed_calibration_model_to_local_bull_score() {
    let dataset: CalibrationDataset = serde_json::from_str(FIXTURE).unwrap();
    let report = evaluate_linear_calibration(&dataset, &default_options());
    assert!(report.pass);
    let record = calibration_run_record("calibration-run-1", &report).unwrap();

    let application = apply_calibration(&CalibrationApplicationInput {
        metric_family: "recovery".to_string(),
        algorithm_id: "bull.recovery.v0".to_string(),
        algorithm_version: "0.1.0".to_string(),
        raw_score: 70.0,
        input_run_id: Some("recovery-run-1".to_string()),
        score_min: 0.0,
        score_max: 100.0,
        calibration_run: record,
    });

    assert!(application.pass, "{:?}", application.issues);
    assert!(application.input_valid);
    assert!(application.score_range_valid);
    assert!(application.calibration_run_valid);
    assert!(application.model_ready);
    assert!(application.model_applied);
    assert!(application.application_ready);
    assert!(
        application.next_actions.is_empty(),
        "{:?}",
        application.next_actions
    );
    assert_close(application.calibrated_score.unwrap(), 79.0);
    assert_eq!(application.output_kind, "bull_calibrated_local_score");
    assert!(application.official_labels_are_labels);
    assert_eq!(
        application.provenance["label_policy"],
        "user_owned_labels_only"
    );
}

#[test]
fn apply_calibration_clamps_to_score_range_and_flags_it() {
    let dataset: CalibrationDataset = serde_json::from_str(FIXTURE).unwrap();
    let report = evaluate_linear_calibration(&dataset, &default_options());
    let record = calibration_run_record("calibration-run-1", &report).unwrap();

    let application = apply_calibration(&CalibrationApplicationInput {
        metric_family: "recovery".to_string(),
        algorithm_id: "bull.recovery.v0".to_string(),
        algorithm_version: "0.1.0".to_string(),
        raw_score: 200.0,
        input_run_id: None,
        score_min: 0.0,
        score_max: 100.0,
        calibration_run: record,
    });

    assert!(application.pass, "{:?}", application.issues);
    assert!(application.input_valid);
    assert!(application.score_range_valid);
    assert!(application.calibration_run_valid);
    assert!(application.model_ready);
    assert!(application.model_applied);
    assert!(application.application_ready);
    assert_close(application.calibrated_score.unwrap(), 100.0);
    assert!(
        application
            .quality_flags
            .contains(&"calibrated_score_clamped_to_range".to_string())
    );
}

#[test]
fn apply_calibration_rejects_failed_or_mismatched_calibration_runs() {
    let failed_record = bull_core::store::CalibrationRunRecord {
        calibration_run_id: "failed-calibration".to_string(),
        algorithm_id: "bull.sleep.v0".to_string(),
        version: "0.1.0".to_string(),
        times: CalibrationRunTimes {
            train_start: "2026-05-01T00:00:00Z".to_string(),
            train_end: "2026-05-02T00:00:00Z".to_string(),
            holdout_start: "2026-05-03T00:00:00Z".to_string(),
            holdout_end: "2026-05-04T00:00:00Z".to_string(),
        },
        metrics_json: "{}".to_string(),
        params_json: serde_json::json!({
            "model": {"model_type": "ordinary_least_squares_1d", "slope": 1.0, "intercept": 0.0},
            "pass": false
        })
        .to_string(),
    };

    let application = apply_calibration(&CalibrationApplicationInput {
        metric_family: "recovery".to_string(),
        algorithm_id: "bull.recovery.v0".to_string(),
        algorithm_version: "0.1.0".to_string(),
        raw_score: 70.0,
        input_run_id: None,
        score_min: 0.0,
        score_max: 100.0,
        calibration_run: failed_record,
    });

    assert!(!application.pass);
    assert!(application.input_valid);
    assert!(application.score_range_valid);
    assert!(!application.calibration_run_valid);
    assert!(application.model_ready);
    assert!(!application.model_applied);
    assert!(!application.application_ready);
    assert!(application.calibrated_score.is_none());
    assert!(
        application
            .issues
            .iter()
            .any(|issue| issue.contains("calibration run targets bull.sleep.v0"))
    );
    assert!(
        application
            .issues
            .iter()
            .any(|issue| issue.contains("did not pass holdout validation"))
    );
    assert!(
        application
            .next_actions
            .iter()
            .any(|action| action.reason == "calibration_run_mismatch")
    );
    assert!(
        application
            .next_actions
            .iter()
            .any(|action| action.reason == "calibration_run_failed")
    );
}

#[test]
fn reports_next_actions_for_insufficient_calibration_rows() {
    let dataset = CalibrationDataset {
        schema: "bull.calibration-dataset.v1".to_string(),
        records: Vec::new(),
    };
    let report = evaluate_linear_calibration(&dataset, &default_options());

    assert!(!report.pass);
    assert!(report.dataset_valid);
    assert!(report.labels_valid);
    assert!(!report.split_valid);
    assert!(!report.model_fit_ready);
    assert!(!report.train_metrics_ready);
    assert!(!report.holdout_metrics_ready);
    assert!(!report.holdout_improvement_valid);
    assert!(!report.calibration_ready);
    assert!(
        report
            .next_actions
            .iter()
            .any(|action| action.reason == "insufficient_train_rows")
    );
    assert!(
        report
            .next_actions
            .iter()
            .any(|action| action.reason == "insufficient_holdout_rows")
    );
}

fn default_options() -> CalibrationOptions {
    CalibrationOptions {
        metric_family: "recovery".to_string(),
        algorithm_id: "bull.recovery.v0".to_string(),
        algorithm_version: "0.1.0".to_string(),
        split_at: "2026-05-04T00:00:00Z".to_string(),
        min_train_rows: 2,
        min_holdout_rows: 1,
    }
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "expected {expected}, got {actual}"
    );
}
