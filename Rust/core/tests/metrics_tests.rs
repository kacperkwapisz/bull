use std::collections::BTreeMap;

use bull_core::{
    metrics::{
        BULL_HRV_V0_ID, BULL_HRV_V0_VERSION, BULL_RECOVERY_V0_ID, BULL_SLEEP_V0_ID,
        BULL_SLEEP_V1_ID, BULL_STRAIN_V0_ID, BULL_STRESS_V0_ID, HrvInput, RecoveryInput,
        SleepInput, SleepModelStatus, SleepModelStatusInput, SleepNightHistoryInput,
        SleepStageSegment, SleepV1Input, SleepV1Output, StrainInput, StressInput,
        algorithm_run_record, built_in_algorithm_definitions,
        built_in_default_algorithm_preferences, evaluate_sleep_model_status, bull_hrv_v0,
        bull_recovery_v0, bull_sleep_v0, bull_sleep_v1, bull_strain_v0, bull_strain_v1,
        bull_stress_v0, banister_trimp_zone_midpoint, estimate_hrmax_from_history,
        fit_strain_denominator, resolve_effective_hrmax, tanaka_hrmax,
        hrv_run_record, sleep_baseline_from_history,
    },
    store::BullStore,
};

#[test]
fn bull_hrv_v0_computes_hand_derived_time_domain_metrics() {
    let result = bull_hrv_v0(&HrvInput {
        start_time: "2026-05-27T00:00:00Z".to_string(),
        end_time: "2026-05-27T00:01:00Z".to_string(),
        rr_intervals_ms: vec![800.0, 810.0, 790.0, 800.0],
        input_ids: vec!["hand-derived".to_string()],
            rr_timestamps_s: None,
        stage_segments: None,
});

    let output = result.output.unwrap();
    assert_eq!(output.algorithm_id, BULL_HRV_V0_ID);
    assert_eq!(output.algorithm_version, BULL_HRV_V0_VERSION);
    assert_eq!(output.interval_count, 4);
    assert_eq!(output.valid_interval_count, 4);
    assert_close(output.mean_nn_ms, 800.0);
    assert_close(output.rmssd_ms, 200.0_f64.sqrt());
    assert_close(output.sdnn_ms, (200.0_f64 / 3.0).sqrt());
    assert_close(output.pnn50_fraction, 0.0);
    assert!(
        result
            .quality_flags
            .contains(&"low_interval_count".to_string())
    );
}

#[test]
fn bull_hrv_v0_pnn50_uses_strictly_greater_than_50_ms() {
    let result = bull_hrv_v0(&HrvInput {
        start_time: "2026-05-27T00:00:00Z".to_string(),
        end_time: "2026-05-27T00:01:00Z".to_string(),
        rr_intervals_ms: vec![800.0, 850.0, 901.0],
        input_ids: Vec::new(),
            rr_timestamps_s: None,
        stage_segments: None,
});

    let output = result.output.unwrap();
    assert_close(output.pnn50_fraction, 0.5);
}

#[test]
fn bull_hrv_v0_drops_nonphysiological_intervals_and_flags_quality() {
    let result = bull_hrv_v0(&HrvInput {
        start_time: "2026-05-27T00:00:00Z".to_string(),
        end_time: "2026-05-27T00:01:00Z".to_string(),
        rr_intervals_ms: vec![800.0, 100.0, 810.0, 2500.0, 790.0],
        input_ids: Vec::new(),
            rr_timestamps_s: None,
        stage_segments: None,
});

    let output = result.output.unwrap();
    assert_eq!(output.interval_count, 5);
    assert_eq!(output.valid_interval_count, 3);
    assert_eq!(output.invalid_interval_count, 2);
    assert_close(output.rmssd_ms, 250.0_f64.sqrt());
    assert!(
        result
            .quality_flags
            .contains(&"invalid_rr_interval_dropped".to_string())
    );
}

#[test]
fn bull_hrv_v0_reports_insufficient_data_without_output() {
    let result = bull_hrv_v0(&HrvInput {
        start_time: "2026-05-27T00:00:00Z".to_string(),
        end_time: "2026-05-27T00:01:00Z".to_string(),
        rr_intervals_ms: vec![800.0],
        input_ids: Vec::new(),
            rr_timestamps_s: None,
        stage_segments: None,
});

    assert!(result.output.is_none());
    assert!(
        result
            .errors
            .contains(&"not_enough_valid_rr_intervals".to_string())
    );
}

#[test]
fn bull_hrv_v0_ectopic_filter_removes_outlier_beat() {
    // One in-range but physiologically implausible beat (1300 ms among ~800 ms),
    // centred with padding so boundary windows don't flag legitimate beats, must be
    // rejected by the ectopic filter before RMSSD.
    let result = bull_hrv_v0(&HrvInput {
        start_time: "2026-05-27T00:00:00Z".to_string(),
        end_time: "2026-05-27T00:01:00Z".to_string(),
        rr_intervals_ms: vec![
            800.0, 800.0, 800.0, 800.0, 1300.0, 800.0, 800.0, 800.0, 800.0,
        ],
        input_ids: Vec::new(),
        rr_timestamps_s: None,
        stage_segments: None,
    });

    let output = result.output.unwrap();
    // All nine beats are inside the 300-2000 ms range gate.
    assert_eq!(output.valid_interval_count, 9);
    // Exactly one of nine beats removed by the ectopic filter.
    assert_close(output.ectopic_filter_removal_fraction, 1.0 / 9.0);
    // With the outlier gone the remaining beats are all 800 ms -> zero RMSSD.
    assert_close(output.rmssd_ms, 0.0);
    assert!(
        result
            .quality_flags
            .contains(&"ectopic_beats_removed".to_string())
    );
}

#[test]
fn bull_hrv_v0_segments_rmssd_across_capture_gaps() {
    // A large gap between successive capture timestamps must split the series so
    // RMSSD never differences beats across the gap.
    let result = bull_hrv_v0(&HrvInput {
        start_time: "2026-05-27T00:00:00Z".to_string(),
        end_time: "2026-05-27T00:01:00Z".to_string(),
        rr_intervals_ms: vec![800.0, 805.0, 802.0, 808.0],
        input_ids: Vec::new(),
        // Gap of 19 s between the 2nd and 3rd beats (> 3 s threshold).
        rr_timestamps_s: Some(vec![0.0, 1.0, 20.0, 21.0]),
        stage_segments: None,
    });

    let output = result.output.unwrap();
    // Only within-segment pairs (5 ms and 6 ms diffs) contribute to RMSSD.
    assert_close(output.rmssd_ms, ((25.0 + 36.0) / 2.0_f64).sqrt());
    assert!(
        result
            .quality_flags
            .contains(&"rr_segment_gap_detected".to_string())
    );
}

#[test]
fn bull_hrv_v0_selects_deep_sleep_window_tier() {
    // A deep segment >= 5 min selects tier 1 and RMSSD is computed over that window.
    let result = bull_hrv_v0(&HrvInput {
        start_time: "2026-05-27T00:00:00Z".to_string(),
        end_time: "2026-05-27T00:20:00Z".to_string(),
        rr_intervals_ms: (0..10).map(|i| 800.0 + (i % 2) as f64 * 10.0).collect(),
        input_ids: Vec::new(),
        rr_timestamps_s: None,
        stage_segments: Some(vec![
            SleepStageSegment {
                stage_kind: "light".to_string(),
                start_time: "2026-05-27T00:00:00Z".to_string(),
                end_time: "2026-05-27T00:10:00Z".to_string(),
                duration_minutes: 10.0,
                confidence_0_to_1: 0.9,
                stage_probabilities: BTreeMap::new(),
            },
            SleepStageSegment {
                stage_kind: "deep".to_string(),
                start_time: "2026-05-27T00:10:00Z".to_string(),
                end_time: "2026-05-27T00:20:00Z".to_string(),
                duration_minutes: 10.0,
                confidence_0_to_1: 0.9,
                stage_probabilities: BTreeMap::new(),
            },
        ]),
    });

    let output = result.output.unwrap();
    assert_eq!(output.window_tier_used, 1);
}

#[test]
fn hrv_definition_and_run_persist_to_sqlite() {
    let store = BullStore::open_in_memory().unwrap();
    let definition = built_in_algorithm_definitions().remove(0);
    store.upsert_algorithm_definition(&definition).unwrap();

    let saved = store
        .algorithm_definition(BULL_HRV_V0_ID, BULL_HRV_V0_VERSION)
        .unwrap()
        .unwrap();
    assert_eq!(saved.metric_family, "hrv");
    assert_eq!(saved.status, "beta");

    let result = bull_hrv_v0(&HrvInput {
        start_time: "2026-05-27T00:00:00Z".to_string(),
        end_time: "2026-05-27T00:01:00Z".to_string(),
        rr_intervals_ms: vec![800.0, 810.0, 790.0, 800.0],
        input_ids: vec!["fixture.synthetic".to_string()],
            rr_timestamps_s: None,
        stage_segments: None,
});
    let record = hrv_run_record("hrv-run-1", &result).unwrap();
    assert!(store.insert_algorithm_run(&record).unwrap());
    assert!(!store.insert_algorithm_run(&record).unwrap());

    let saved_run = store.algorithm_run("hrv-run-1").unwrap().unwrap();
    assert_eq!(saved_run.algorithm_id, BULL_HRV_V0_ID);
    assert!(saved_run.output_json.contains("\"rmssd_ms\""));
    let metric_values = store.metric_values_for_run("hrv-run-1").unwrap();
    assert_eq!(metric_values.len(), 9);
    assert!(metric_values.iter().any(|row| {
        row.metric_value_id == "hrv-run-1.rmssd_ms"
            && row.metric_family == "hrv"
            && row.unit == "ms"
            && (row.value - 14.142135623730951).abs() < 1e-12
    }));
    assert!(metric_values.iter().any(|row| {
        row.metric_value_id == "hrv-run-1.pnn50_fraction"
            && row.unit == "fraction"
            && row.value == 0.0
    }));
    let metric_components = store.metric_components_for_run("hrv-run-1").unwrap();
    assert_eq!(metric_components.len(), 4);
    assert!(metric_components.iter().any(|row| {
        row.metric_component_id == "hrv-run-1.component.1.rmssd"
            && row.component_name == "rmssd"
            && row.unit == "ms"
            && serde_json::from_str::<serde_json::Value>(&row.contribution_json)
                .unwrap()
                .is_object()
    }));
    assert_eq!(
        store
            .algorithm_runs_overlapping("2026-05-27T00:00:30Z", "2026-05-27T00:02:00Z")
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn built_in_registry_includes_flagship_bull_score_family() {
    let definitions = built_in_algorithm_definitions();
    let ids = definitions
        .iter()
        .map(|definition| definition.algorithm_id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(ids[0], BULL_HRV_V0_ID);
    assert!(ids.contains(&BULL_SLEEP_V0_ID));
    assert!(ids.contains(&BULL_SLEEP_V1_ID));
    assert!(ids.contains(&BULL_STRAIN_V0_ID));
    assert!(ids.contains(&BULL_RECOVERY_V0_ID));
    assert!(ids.contains(&BULL_STRESS_V0_ID));
    assert_eq!(definitions.len(), 6);
    assert!(
        definitions
            .iter()
            .all(|definition| definition.implementation == "rust")
    );
}

#[test]
fn built_in_sleep_v0_remains_stable_default_while_sleep_v1_is_experimental() {
    let definitions = built_in_algorithm_definitions();
    let sleep_v0 = definitions
        .iter()
        .find(|definition| definition.algorithm_id == BULL_SLEEP_V0_ID)
        .unwrap();
    let sleep_v1 = definitions
        .iter()
        .find(|definition| definition.algorithm_id == BULL_SLEEP_V1_ID)
        .unwrap();
    let sleep_preference = built_in_default_algorithm_preferences()
        .into_iter()
        .find(|preference| preference.metric_family == "sleep")
        .unwrap();

    assert_eq!(sleep_v0.status, "experimental");
    assert_eq!(sleep_v1.status, "experimental");
    assert_eq!(sleep_preference.algorithm_id, BULL_SLEEP_V0_ID);
    assert_eq!(sleep_preference.version, sleep_v0.version);
}

#[test]
fn bull_sleep_v0_computes_hand_derived_component_score() {
    let result = bull_sleep_v0(&SleepInput {
        start_time: "2026-05-27T22:30:00Z".to_string(),
        end_time: "2026-05-28T06:30:00Z".to_string(),
        sleep_duration_minutes: 420.0,
        sleep_need_minutes: 480.0,
        time_in_bed_minutes: 480.0,
        midpoint_deviation_minutes: 30.0,
        disturbance_count: 4,
        input_ids: vec!["hand-derived.sleep".to_string()],
        ..Default::default()
    });

    let output = result.output.unwrap();
    assert_eq!(output.algorithm_id, BULL_SLEEP_V0_ID);
    assert_close(output.sleep_debt_minutes, 60.0);
    assert_close(output.efficiency_fraction, 0.875);
    assert_close(output.score_0_to_100, 84.875);
    assert!(result.errors.is_empty());
}

#[test]
fn bull_sleep_v0_reports_sleep_architecture_latency_and_hr_dip() {
    let result = bull_sleep_v0(&SleepInput {
        start_time: "2026-05-27T22:30:00Z".to_string(),
        end_time: "2026-05-28T06:30:00Z".to_string(),
        sleep_duration_minutes: 420.0,
        sleep_need_minutes: 480.0,
        time_in_bed_minutes: 480.0,
        midpoint_deviation_minutes: 30.0,
        disturbance_count: 4,
        sleep_latency_minutes: 18.0,
        wake_after_sleep_onset_minutes: 42.0,
        wake_episode_count: 2,
        stage_minutes: BTreeMap::from([
            ("awake".to_string(), 60.0),
            ("core".to_string(), 210.0),
            ("deep".to_string(), 90.0),
            ("rem".to_string(), 120.0),
        ]),
        heart_rate_dip_percent: Some(12.5),
        input_ids: vec!["hand-derived.sleep.rich".to_string()],
    });

    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(
        !result
            .quality_flags
            .contains(&"sleep_architecture_unavailable".to_string())
    );
    let output = result.output.unwrap();
    assert_close(output.score_0_to_100, 84.875);
    assert_close(output.sleep_performance_fraction, 0.875);
    assert_close(output.awake_minutes, 60.0);
    assert_close(output.restorative_sleep_minutes, 210.0);
    assert_close(output.restorative_sleep_fraction, 0.5);
    assert_close(output.sleep_latency_minutes, 18.0);
    assert_close(output.wake_after_sleep_onset_minutes, 42.0);
    assert_eq!(output.wake_episode_count, 2);
    assert_close(output.heart_rate_dip_percent.unwrap(), 12.5);
    assert!(
        output
            .components
            .iter()
            .any(|component| component.name == "restorative_sleep" && component.weight == 0.0)
    );
}

#[test]
fn bull_sleep_v0_reports_invalid_inputs_without_output() {
    let result = bull_sleep_v0(&SleepInput {
        start_time: "2026-05-27T22:30:00Z".to_string(),
        end_time: "2026-05-28T06:30:00Z".to_string(),
        sleep_duration_minutes: 0.0,
        sleep_need_minutes: 0.0,
        time_in_bed_minutes: 0.0,
        midpoint_deviation_minutes: 0.0,
        disturbance_count: 0,
        input_ids: Vec::new(),
        ..Default::default()
    });

    assert!(result.output.is_none());
    assert!(
        result
            .errors
            .contains(&"sleep_need_minutes_must_be_finite_positive".to_string())
    );
    assert!(
        result
            .quality_flags
            .contains(&"short_sleep_window".to_string())
    );
}

#[test]
fn sleep_model_status_reports_setup_needed_without_history_or_permission() {
    let report = evaluate_sleep_model_status(&SleepModelStatusInput::default());

    assert_eq!(report.status, SleepModelStatus::SetupNeeded);
    assert_eq!(report.status_label, "Setup needed");
    assert_eq!(report.report_state, "pending");
    assert_eq!(report.valid_sleep_nights, 0);
    assert!(!report.can_show_provisional_score);
    assert!(!report.can_show_final_score);
    assert!(
        report
            .quality_flags
            .contains(&"sleep_history_permission_missing".to_string())
    );
}

#[test]
fn sleep_model_status_keeps_permission_only_setup_pending() {
    let report = evaluate_sleep_model_status(&SleepModelStatusInput {
        sleep_permission_granted: true,
        motion_coverage_fraction: Some(0.92),
        heart_rate_coverage_fraction: Some(0.80),
        ..Default::default()
    });

    assert_eq!(report.status, SleepModelStatus::SetupNeeded);
    assert_eq!(report.report_state, "pending");
    assert_eq!(report.valid_sleep_nights, 0);
    assert!(!report.can_show_provisional_score);
    assert!(!report.can_show_final_score);
    assert!(
        report
            .next_actions
            .contains(&"Complete one sleep night to start learning.".to_string())
    );
}

#[test]
fn sleep_model_status_reports_learning_for_first_packet_derived_night() {
    let report = evaluate_sleep_model_status(&SleepModelStatusInput {
        sleep_permission_granted: true,
        trusted_bull_sleep_nights: 1,
        motion_coverage_fraction: Some(0.92),
        heart_rate_coverage_fraction: Some(0.80),
        ..Default::default()
    });

    assert_eq!(report.status, SleepModelStatus::Learning);
    assert_eq!(report.report_state, "provisional");
    assert_eq!(report.valid_sleep_nights, 1);
    assert_eq!(report.nights_until_baseline, 6);
    assert_eq!(report.nights_until_bull_training, 6);
    assert!(report.can_show_provisional_score);
    assert!(!report.can_show_personal_baseline);
    assert!(report.status_reason.contains("6 more for baseline"));
}

#[test]
fn sleep_model_status_saturates_malformed_history_counts() {
    let report = evaluate_sleep_model_status(&SleepModelStatusInput {
        sleep_permission_granted: true,
        trusted_bull_sleep_nights: u32::MAX,
        imported_platform_sleep_nights: 1,
        motion_coverage_fraction: Some(0.92),
        heart_rate_coverage_fraction: Some(0.80),
        ..Default::default()
    });

    assert_eq!(report.valid_sleep_nights, u32::MAX);
    assert_eq!(report.nights_until_baseline, 0);
    assert_eq!(report.nights_until_bull_training, 0);
    assert_eq!(report.status, SleepModelStatus::BaselineReady);
}

#[test]
fn sleep_model_status_reports_importing_history_as_provisional() {
    let report = evaluate_sleep_model_status(&SleepModelStatusInput {
        sleep_permission_granted: true,
        history_import_in_progress: true,
        imported_platform_sleep_nights: 3,
        trusted_bull_sleep_nights: 1,
        motion_coverage_fraction: Some(0.90),
        heart_rate_coverage_fraction: Some(0.80),
        ..Default::default()
    });

    assert_eq!(report.status, SleepModelStatus::ImportingHistory);
    assert_eq!(report.report_state, "provisional");
    assert!(report.can_show_provisional_score);
    assert!(!report.can_show_final_score);
    assert!(
        report
            .quality_flags
            .contains(&"sleep_history_import_in_progress".to_string())
    );
}

#[test]
fn sleep_model_status_uses_imported_sleep_history_for_baseline_readiness() {
    let report = evaluate_sleep_model_status(&SleepModelStatusInput {
        sleep_permission_granted: true,
        imported_platform_sleep_nights: 12,
        trusted_bull_sleep_nights: 1,
        excluded_sleep_nights: 2,
        motion_coverage_fraction: Some(0.90),
        heart_rate_coverage_fraction: Some(0.75),
        ..Default::default()
    });

    assert_eq!(report.status, SleepModelStatus::BaselineReady);
    assert_eq!(report.report_state, "final");
    assert_eq!(report.valid_sleep_nights, 13);
    assert_eq!(report.excluded_sleep_nights, 2);
    assert!(report.can_show_final_score);
    assert!(report.can_show_personal_baseline);
    assert!(!report.can_show_trained_score);
}

#[test]
fn sleep_model_status_requires_bull_night_for_final_score_visibility() {
    let report = evaluate_sleep_model_status(&SleepModelStatusInput {
        sleep_permission_granted: true,
        imported_platform_sleep_nights: 12,
        trusted_bull_sleep_nights: 0,
        motion_coverage_fraction: Some(0.90),
        heart_rate_coverage_fraction: Some(0.75),
        ..Default::default()
    });

    assert_eq!(report.status, SleepModelStatus::BaselineReady);
    assert_eq!(report.report_state, "provisional");
    assert!(report.can_show_provisional_score);
    assert!(!report.can_show_final_score);
    assert!(report.can_show_personal_baseline);
    assert!(!report.can_show_trained_score);
    assert!(
        report.next_actions.contains(
            &"Complete one Bull packet-derived sleep night before showing a final Sleep V1 score."
                .to_string()
        )
    );
}

#[test]
fn sleep_model_status_requires_bull_nights_for_trained() {
    let report = evaluate_sleep_model_status(&SleepModelStatusInput {
        sleep_permission_granted: true,
        imported_platform_sleep_nights: 30,
        trusted_bull_sleep_nights: 7,
        motion_coverage_fraction: Some(0.88),
        heart_rate_coverage_fraction: Some(0.70),
        calibration_label_count: 20,
        holdout_validation_passed: true,
        ..Default::default()
    });

    assert_eq!(report.status, SleepModelStatus::Trained);
    assert_eq!(report.report_state, "final");
    assert_eq!(report.calibration_label_count, 20);
    assert_eq!(report.nights_until_bull_training, 0);
    assert_eq!(report.nights_until_training, 0);
    assert!(report.can_show_trained_score);
}

#[test]
fn sleep_model_status_requires_calibration_labels_for_trained() {
    let report = evaluate_sleep_model_status(&SleepModelStatusInput {
        sleep_permission_granted: true,
        imported_platform_sleep_nights: 30,
        trusted_bull_sleep_nights: 7,
        motion_coverage_fraction: Some(0.88),
        heart_rate_coverage_fraction: Some(0.70),
        calibration_label_count: 0,
        holdout_validation_passed: true,
        ..Default::default()
    });

    assert_eq!(report.status, SleepModelStatus::BaselineReady);
    assert_eq!(report.report_state, "final");
    assert_eq!(report.calibration_label_count, 0);
    assert_eq!(report.nights_until_bull_training, 0);
    assert_eq!(report.nights_until_training, 14);
    assert!(!report.can_show_trained_score);
    assert!(
        report.next_actions.contains(
            &"Add 14 more user-owned sleep calibration labels before training.".to_string()
        )
    );
}

#[test]
fn sleep_model_status_names_packet_nights_before_training() {
    let report = evaluate_sleep_model_status(&SleepModelStatusInput {
        sleep_permission_granted: true,
        imported_platform_sleep_nights: 12,
        trusted_bull_sleep_nights: 3,
        motion_coverage_fraction: Some(0.88),
        heart_rate_coverage_fraction: Some(0.70),
        calibration_label_count: 20,
        holdout_validation_passed: true,
        ..Default::default()
    });

    assert_eq!(report.status, SleepModelStatus::BaselineReady);
    assert_eq!(report.report_state, "final");
    assert_eq!(report.nights_until_bull_training, 4);
    assert_eq!(report.nights_until_training, 0);
    assert!(!report.can_show_trained_score);
    assert!(report.next_actions.contains(
        &"Collect 4 more Bull packet-derived sleep nights before training.".to_string()
    ));
}

#[test]
fn sleep_model_status_does_not_train_on_imported_history_without_bull_nights() {
    let report = evaluate_sleep_model_status(&SleepModelStatusInput {
        sleep_permission_granted: true,
        imported_platform_sleep_nights: 30,
        trusted_bull_sleep_nights: 0,
        motion_coverage_fraction: Some(0.88),
        heart_rate_coverage_fraction: Some(0.70),
        calibration_label_count: 20,
        holdout_validation_passed: false,
        ..Default::default()
    });

    assert_eq!(report.status, SleepModelStatus::BaselineReady);
    assert_eq!(report.report_state, "provisional");
    assert_eq!(report.nights_until_bull_training, 7);
    assert_eq!(report.nights_until_training, 0);
    assert!(!report.can_show_final_score);
    assert!(!report.can_show_trained_score);
    assert!(
        report.next_actions.contains(
            &"Complete one Bull packet-derived sleep night before showing a final Sleep V1 score."
                .to_string()
        )
    );
}

#[test]
fn sleep_model_status_requires_heart_rate_coverage_for_trained() {
    let report = evaluate_sleep_model_status(&SleepModelStatusInput {
        sleep_permission_granted: true,
        imported_platform_sleep_nights: 30,
        trusted_bull_sleep_nights: 7,
        motion_coverage_fraction: Some(0.88),
        heart_rate_coverage_fraction: Some(0.20),
        calibration_label_count: 20,
        holdout_validation_passed: true,
        ..Default::default()
    });

    assert_eq!(report.status, SleepModelStatus::BaselineReady);
    assert_eq!(report.report_state, "provisional");
    assert!(!report.can_show_final_score);
    assert!(!report.can_show_trained_score);
    assert!(
        report
            .quality_flags
            .contains(&"heart_rate_coverage_low".to_string())
    );
}

#[test]
fn sleep_model_status_prioritizes_relearn_signals_over_trained() {
    let pattern_shift = evaluate_sleep_model_status(&SleepModelStatusInput {
        sleep_permission_granted: true,
        imported_platform_sleep_nights: 30,
        trusted_bull_sleep_nights: 7,
        motion_coverage_fraction: Some(0.88),
        heart_rate_coverage_fraction: Some(0.70),
        calibration_label_count: 20,
        holdout_validation_passed: true,
        timezone_or_schedule_shift_detected: true,
        ..Default::default()
    });

    assert_eq!(pattern_shift.status, SleepModelStatus::NeedsRelearn);
    assert_eq!(pattern_shift.report_state, "provisional");
    assert!(!pattern_shift.can_show_trained_score);
    assert!(
        pattern_shift
            .quality_flags
            .contains(&"timezone_or_schedule_shift_detected".to_string())
    );

    let stale = evaluate_sleep_model_status(&SleepModelStatusInput {
        sleep_permission_granted: true,
        imported_platform_sleep_nights: 30,
        trusted_bull_sleep_nights: 7,
        motion_coverage_fraction: Some(0.88),
        heart_rate_coverage_fraction: Some(0.70),
        calibration_label_count: 20,
        holdout_validation_passed: true,
        days_since_last_valid_night: Some(14),
        ..Default::default()
    });

    assert_eq!(stale.status, SleepModelStatus::NeedsRelearn);
    assert_eq!(stale.report_state, "provisional");
    assert!(!stale.can_show_trained_score);
    assert!(
        stale
            .quality_flags
            .contains(&"sleep_baseline_stale".to_string())
    );
}

#[test]
fn sleep_model_status_keeps_low_coverage_reports_provisional() {
    let report = evaluate_sleep_model_status(&SleepModelStatusInput {
        sleep_permission_granted: true,
        imported_platform_sleep_nights: 10,
        trusted_bull_sleep_nights: 1,
        motion_coverage_fraction: Some(0.68),
        heart_rate_coverage_fraction: Some(0.40),
        ..Default::default()
    });

    assert_eq!(report.status, SleepModelStatus::BaselineReady);
    assert_eq!(report.report_state, "provisional");
    assert!(report.can_show_provisional_score);
    assert!(!report.can_show_final_score);
    assert!(
        report
            .quality_flags
            .contains(&"motion_coverage_low".to_string())
    );
    assert!(
        report
            .quality_flags
            .contains(&"heart_rate_coverage_low".to_string())
    );
}

#[test]
fn sleep_model_status_requires_explicit_coverage_for_final_or_trained() {
    let report = evaluate_sleep_model_status(&SleepModelStatusInput {
        sleep_permission_granted: true,
        imported_platform_sleep_nights: 10,
        trusted_bull_sleep_nights: 7,
        calibration_label_count: 20,
        holdout_validation_passed: true,
        ..Default::default()
    });

    assert_eq!(report.status, SleepModelStatus::BaselineReady);
    assert_eq!(report.report_state, "provisional");
    assert!(report.can_show_provisional_score);
    assert!(!report.can_show_final_score);
    assert!(!report.can_show_trained_score);
    assert!(
        report
            .quality_flags
            .contains(&"motion_coverage_low".to_string())
    );
    assert!(
        report
            .quality_flags
            .contains(&"heart_rate_coverage_low".to_string())
    );
}

#[test]
fn sleep_model_status_blocks_when_timestamps_are_untrusted() {
    let report = evaluate_sleep_model_status(&SleepModelStatusInput {
        sleep_permission_granted: true,
        trusted_bull_sleep_nights: 8,
        imported_platform_sleep_nights: 8,
        timestamp_sync_blocked: true,
        ..Default::default()
    });

    assert_eq!(report.status, SleepModelStatus::Blocked);
    assert_eq!(report.report_state, "blocked");
    assert!(!report.can_show_provisional_score);
    assert!(!report.can_show_final_score);
    assert!(!report.can_show_personal_baseline);
    assert!(
        report
            .quality_flags
            .contains(&"timestamp_sync_blocked".to_string())
    );
}

#[test]
fn bull_sleep_v1_computes_hand_derived_component_score() {
    let result = bull_sleep_v1(&SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 420.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            midpoint_deviation_minutes: 30.0,
            disturbance_count: 4,
            sleep_latency_minutes: 18.0,
            wake_after_sleep_onset_minutes: 42.0,
            wake_episode_count: 2,
            stage_minutes: BTreeMap::from([
                ("awake".to_string(), 60.0),
                ("core".to_string(), 210.0),
                ("deep".to_string(), 90.0),
                ("rem".to_string(), 120.0),
            ]),
            heart_rate_dip_percent: Some(12.5),
            input_ids: vec!["hand-derived.sleep.v1".to_string()],
        },
        model_status: SleepModelStatusInput {
            sleep_permission_granted: true,
            imported_platform_sleep_nights: 10,
            trusted_bull_sleep_nights: 2,
            motion_coverage_fraction: Some(0.94),
            heart_rate_coverage_fraction: Some(0.82),
            ..Default::default()
        },
        prior_nights: Vec::new(),
        stage_segments: Vec::new(),
        rolling_sleep_debt_minutes: 90.0,
        bedtime_deviation_minutes: 20.0,
        wake_time_deviation_minutes: 15.0,
        sleep_hr_average_bpm: Some(61.0),
        sleep_hr_min_bpm: Some(54.0),
        pre_sleep_awake_hr_average_bpm: None,
        sleep_hr_trend_bpm_per_hour: Some(-1.2),
        naps_minutes: 25.0,
        prior_day_strain: Some(8.5),
        data_coverage_fraction: Some(0.92),
    });

    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.algorithm_id, BULL_SLEEP_V1_ID);
    let output = result.output.unwrap();
    assert_eq!(output.algorithm_id, BULL_SLEEP_V1_ID);
    assert_eq!(output.model_status, SleepModelStatus::BaselineReady);
    assert_eq!(output.model_status_label, "Baseline ready");
    assert_close(output.score_0_to_100, 82.01361892264234);
    assert_close(output.sleep_need_minutes, 480.0);
    assert_close(output.rolling_sleep_debt_minutes, 90.0);
    assert_close(output.bedtime_deviation_minutes, 20.0);
    assert_close(output.wake_time_deviation_minutes, 15.0);
    assert_close(output.deep_sleep_minutes, 90.0);
    assert_close(output.rem_sleep_minutes, 120.0);
    assert_close(output.core_sleep_minutes, 210.0);
    assert_close(output.sleep_hr_average_bpm.unwrap(), 61.0);
    assert_close(output.sleep_hr_min_bpm.unwrap(), 54.0);
    assert_close(output.sleep_hr_trend_bpm_per_hour.unwrap(), -1.2);
    assert_close(output.sleep_hr_dip_percent.unwrap(), 12.5);
    assert_close(output.sleep_hr_recovery_score.unwrap(), 62.5);
    assert_close(output.naps_minutes, 25.0);
    assert_close(output.prior_day_strain.unwrap(), 8.5);
    assert_close(output.data_coverage_fraction.unwrap(), 0.92);
    assert_close(output.sleep_window_confidence_0_to_1, 0.884);
    assert!(output.confidence_0_to_1 > 0.75);
    assert!(output.baseline.is_none());
    assert!(output.status_report.can_show_personal_baseline);
    assert_eq!(output.quality_flags, result.quality_flags);
    assert_eq!(
        output.provenance["score_policy"],
        "weighted_sleep_v1_components_with_fragmentation_guardrails"
    );
    assert_eq!(
        output.provenance["status_policy"],
        "rust_sleep_model_status_report"
    );
    assert_eq!(
        output
            .components
            .iter()
            .map(|component| component.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "sleep_need_fulfillment",
            "continuity",
            "schedule_regularity",
            "sleep_architecture",
            "cardiovascular_recovery",
            "context_adjustment",
            "data_confidence",
        ]
    );
    assert_eq!(
        output
            .components
            .iter()
            .map(|component| component.weight)
            .sum::<f64>(),
        1.0
    );
    assert_eq!(output.component_provenance.len(), output.components.len());
    let sleep_need_provenance = output
        .component_provenance
        .get("sleep_need_fulfillment")
        .unwrap();
    assert_eq!(
        sleep_need_provenance["inputs"]["rolling_sleep_debt_minutes"],
        90.0
    );
    assert_eq!(
        output.component_provenance["sleep_architecture"]["policy"],
        "deep_rem_restorative_balance_vs_personal_baseline_when_available_with_architecture_confidence"
    );
    assert_eq!(
        output.component_provenance["data_confidence"]["inputs"]["motion_coverage_fraction"],
        0.94
    );
    assert_close(
        output.component_provenance["data_confidence"]["inputs"]["sleep_window_confidence_0_to_1"]
            .as_f64()
            .unwrap(),
        0.884,
    );
    let data_confidence = output
        .components
        .iter()
        .find(|component| component.name == "data_confidence")
        .unwrap();
    assert_close(
        data_confidence.score_0_to_100,
        output.confidence_0_to_1 * output.sleep_window_confidence_0_to_1 * 0.92 * 100.0,
    );
    assert_eq!(
        output.component_provenance["data_confidence"]["policy"],
        "combined_sleep_v1_confidence_window_confidence_and_coverage"
    );
}

#[test]
fn bull_sleep_v1_caps_confidence_when_heart_rate_coverage_is_low() {
    let mut input = SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 420.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            midpoint_deviation_minutes: 20.0,
            disturbance_count: 2,
            sleep_latency_minutes: 18.0,
            wake_after_sleep_onset_minutes: 34.0,
            wake_episode_count: 2,
            stage_minutes: BTreeMap::from([
                ("core".to_string(), 220.0),
                ("deep".to_string(), 85.0),
                ("rem".to_string(), 115.0),
            ]),
            heart_rate_dip_percent: Some(12.0),
            input_ids: vec!["sleep.v1.low-hr-coverage.fixture".to_string()],
        },
        model_status: SleepModelStatusInput {
            sleep_permission_granted: true,
            imported_platform_sleep_nights: 14,
            trusted_bull_sleep_nights: 7,
            motion_coverage_fraction: Some(0.94),
            heart_rate_coverage_fraction: Some(0.90),
            calibration_label_count: 14,
            holdout_validation_passed: true,
            ..Default::default()
        },
        sleep_hr_average_bpm: Some(61.0),
        sleep_hr_min_bpm: Some(54.0),
        pre_sleep_awake_hr_average_bpm: Some(68.0),
        sleep_hr_trend_bpm_per_hour: Some(-0.8),
        data_coverage_fraction: Some(0.94),
        ..Default::default()
    };

    let high_coverage = bull_sleep_v1(&input).output.unwrap();
    input.model_status.heart_rate_coverage_fraction = Some(0.20);
    let low_coverage_result = bull_sleep_v1(&input);

    assert!(
        low_coverage_result
            .quality_flags
            .contains(&"heart_rate_coverage_low".to_string())
    );
    let low_coverage = low_coverage_result.output.unwrap();
    assert_eq!(low_coverage.model_status, SleepModelStatus::BaselineReady);
    assert_eq!(low_coverage.status_report.report_state, "provisional");
    assert!(!low_coverage.status_report.can_show_final_score);
    assert!(high_coverage.confidence_0_to_1 > low_coverage.confidence_0_to_1);
    assert!(
        high_coverage.sleep_window_confidence_0_to_1 > low_coverage.sleep_window_confidence_0_to_1
    );
    assert_close(low_coverage.confidence_0_to_1, 0.72);
    assert_close(low_coverage.sleep_window_confidence_0_to_1, 0.70);
    assert_eq!(
        low_coverage.component_provenance["data_confidence"]["inputs"]["heart_rate_coverage_fraction"],
        0.20
    );
}

#[test]
fn bull_sleep_v1_derives_stage_minutes_from_confident_segments() {
    let result = bull_sleep_v1(&SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 390.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            midpoint_deviation_minutes: 25.0,
            disturbance_count: 3,
            sleep_latency_minutes: 20.0,
            wake_after_sleep_onset_minutes: 40.0,
            wake_episode_count: 3,
            heart_rate_dip_percent: Some(10.0),
            input_ids: vec!["segment-derived.sleep.v1".to_string()],
            ..Default::default()
        },
        model_status: SleepModelStatusInput {
            sleep_permission_granted: true,
            imported_platform_sleep_nights: 8,
            trusted_bull_sleep_nights: 2,
            motion_coverage_fraction: Some(0.90),
            heart_rate_coverage_fraction: Some(0.84),
            ..Default::default()
        },
        stage_segments: vec![
            SleepStageSegment {
                stage_kind: "core".to_string(),
                start_time: "2026-05-27T23:00:00Z".to_string(),
                end_time: "2026-05-28T02:00:00Z".to_string(),
                duration_minutes: 180.0,
                confidence_0_to_1: 0.86,
                stage_probabilities: BTreeMap::from([
                    ("awake".to_string(), 0.04),
                    ("core".to_string(), 0.82),
                    ("deep".to_string(), 0.10),
                    ("rem".to_string(), 0.04),
                ]),
            },
            SleepStageSegment {
                stage_kind: "deep".to_string(),
                start_time: "2026-05-28T02:00:00Z".to_string(),
                end_time: "2026-05-28T03:10:00Z".to_string(),
                duration_minutes: 70.0,
                confidence_0_to_1: 0.78,
                stage_probabilities: BTreeMap::from([
                    ("core".to_string(), 0.18),
                    ("deep".to_string(), 0.76),
                    ("rem".to_string(), 0.06),
                ]),
            },
            SleepStageSegment {
                stage_kind: "rem".to_string(),
                start_time: "2026-05-28T03:10:00Z".to_string(),
                end_time: "2026-05-28T05:30:00Z".to_string(),
                duration_minutes: 140.0,
                confidence_0_to_1: 0.82,
                stage_probabilities: BTreeMap::from([
                    ("core".to_string(), 0.12),
                    ("deep".to_string(), 0.04),
                    ("rem".to_string(), 0.84),
                ]),
            },
        ],
        bedtime_deviation_minutes: 20.0,
        wake_time_deviation_minutes: 30.0,
        data_coverage_fraction: Some(0.91),
        ..Default::default()
    });

    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let output = result.output.unwrap();
    assert_close(output.core_sleep_minutes, 180.0);
    assert_close(output.deep_sleep_minutes, 70.0);
    assert_close(output.rem_sleep_minutes, 140.0);
    assert_eq!(output.stage_segments.len(), 3);
    assert_close(
        output.stage_segment_confidence_0_to_1.unwrap(),
        0.8312820512820513,
    );
    assert_close(
        output.sleep_architecture_confidence_0_to_1.unwrap(),
        0.8253333333333333,
    );
    assert_eq!(
        output.component_provenance["sleep_architecture"]["inputs"]["stage_segment_count"],
        3
    );
    assert_close(
        output.component_provenance["sleep_architecture"]["inputs"]
            ["sleep_architecture_confidence_0_to_1"]
            .as_f64()
            .unwrap(),
        0.8253333333333333,
    );
    assert_close(
        output.component_provenance["data_confidence"]["inputs"]["stage_segment_confidence_0_to_1"]
            .as_f64()
            .unwrap(),
        0.8312820512820513,
    );
}

#[test]
fn bull_sleep_v1_stage_confidence_is_duration_weighted() {
    let result = bull_sleep_v1(&SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 430.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            midpoint_deviation_minutes: 18.0,
            disturbance_count: 2,
            sleep_latency_minutes: 20.0,
            wake_after_sleep_onset_minutes: 30.0,
            wake_episode_count: 2,
            heart_rate_dip_percent: Some(12.0),
            input_ids: vec!["duration-weighted-stage-confidence.fixture".to_string()],
            ..Default::default()
        },
        model_status: SleepModelStatusInput {
            sleep_permission_granted: true,
            imported_platform_sleep_nights: 8,
            trusted_bull_sleep_nights: 2,
            motion_coverage_fraction: Some(0.92),
            heart_rate_coverage_fraction: Some(0.84),
            ..Default::default()
        },
        stage_segments: vec![
            SleepStageSegment {
                stage_kind: "core".to_string(),
                start_time: "2026-05-27T23:00:00Z".to_string(),
                end_time: "2026-05-28T06:00:00Z".to_string(),
                duration_minutes: 420.0,
                confidence_0_to_1: 0.90,
                stage_probabilities: BTreeMap::from([("core".to_string(), 0.90)]),
            },
            SleepStageSegment {
                stage_kind: "awake".to_string(),
                start_time: "2026-05-28T06:00:00Z".to_string(),
                end_time: "2026-05-28T06:10:00Z".to_string(),
                duration_minutes: 10.0,
                confidence_0_to_1: 0.30,
                stage_probabilities: BTreeMap::from([("awake".to_string(), 0.30)]),
            },
        ],
        data_coverage_fraction: Some(0.92),
        ..Default::default()
    });

    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let output = result.output.unwrap();
    assert_close(
        output.stage_segment_confidence_0_to_1.unwrap(),
        0.886046511627907,
    );
    assert_close(
        output.sleep_architecture_confidence_0_to_1.unwrap(),
        0.886046511627907,
    );
    assert_close(
        output.component_provenance["data_confidence"]["inputs"]["stage_segment_confidence_0_to_1"]
            .as_f64()
            .unwrap(),
        0.886046511627907,
    );
}

#[test]
fn bull_sleep_v1_architecture_confidence_uses_stage_probability_uncertainty() {
    let mut input = SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 420.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            midpoint_deviation_minutes: 20.0,
            sleep_latency_minutes: 20.0,
            wake_after_sleep_onset_minutes: 35.0,
            wake_episode_count: 2,
            heart_rate_dip_percent: Some(12.0),
            input_ids: vec!["architecture-confidence.fixture".to_string()],
            ..Default::default()
        },
        model_status: SleepModelStatusInput {
            sleep_permission_granted: true,
            imported_platform_sleep_nights: 8,
            trusted_bull_sleep_nights: 2,
            motion_coverage_fraction: Some(0.92),
            heart_rate_coverage_fraction: Some(0.84),
            ..Default::default()
        },
        stage_segments: vec![SleepStageSegment {
            stage_kind: "rem".to_string(),
            start_time: "2026-05-28T04:00:00Z".to_string(),
            end_time: "2026-05-28T05:00:00Z".to_string(),
            duration_minutes: 60.0,
            confidence_0_to_1: 0.90,
            stage_probabilities: BTreeMap::from([
                ("core".to_string(), 0.42),
                ("deep".to_string(), 0.08),
                ("rem".to_string(), 0.50),
            ]),
        }],
        data_coverage_fraction: Some(0.92),
        ..Default::default()
    };

    let uncertain_output = bull_sleep_v1(&input).output.unwrap();
    assert_close(
        uncertain_output.stage_segment_confidence_0_to_1.unwrap(),
        0.90,
    );
    assert_close(
        uncertain_output
            .sleep_architecture_confidence_0_to_1
            .unwrap(),
        0.74,
    );
    assert_close(
        uncertain_output.component_provenance["data_confidence"]["inputs"]
            ["sleep_architecture_confidence_0_to_1"]
            .as_f64()
            .unwrap(),
        0.74,
    );

    input.stage_segments[0].stage_probabilities =
        BTreeMap::from([("rem".to_string(), 0.90), ("core".to_string(), 0.10)]);
    let confident_output = bull_sleep_v1(&input).output.unwrap();
    assert!(
        confident_output
            .sleep_architecture_confidence_0_to_1
            .unwrap()
            > uncertain_output
                .sleep_architecture_confidence_0_to_1
                .unwrap()
    );
    assert!(confident_output.confidence_0_to_1 > uncertain_output.confidence_0_to_1);
}

#[test]
fn bull_sleep_v1_rejects_invalid_stage_segment_confidence_and_probabilities() {
    let result = bull_sleep_v1(&SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 390.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 450.0,
            input_ids: vec!["invalid-segment.sleep.v1".to_string()],
            ..Default::default()
        },
        stage_segments: vec![SleepStageSegment {
            stage_kind: "dreaming".to_string(),
            start_time: "2026-05-28T02:00:00Z".to_string(),
            end_time: "2026-05-28T03:00:00Z".to_string(),
            duration_minutes: 60.0,
            confidence_0_to_1: 1.2,
            stage_probabilities: BTreeMap::from([
                ("core".to_string(), 0.40),
                ("deep".to_string(), 0.80),
                ("dreaming".to_string(), 0.10),
            ]),
        }],
        ..Default::default()
    });

    assert!(result.output.is_none());
    assert!(
        result
            .errors
            .contains(&"stage_segments_0_stage_kind_unrecognized".to_string())
    );
    assert!(
        result
            .errors
            .contains(&"stage_segments_0_confidence_0_to_1_must_be_between_0_and_1".to_string())
    );
    assert!(
        result
            .errors
            .contains(&"stage_segments_0_stage_probability_dreaming_unrecognized".to_string())
    );
    assert!(
        result
            .errors
            .contains(&"stage_segments_0_stage_probability_sum_must_not_exceed_1".to_string())
    );
}

#[test]
fn bull_sleep_v1_rejects_unrecognized_current_stage_minutes() {
    let result = bull_sleep_v1(&SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 390.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 450.0,
            stage_minutes: BTreeMap::from([
                ("core".to_string(), 240.0),
                ("unknown".to_string(), 60.0),
            ]),
            input_ids: vec!["invalid-stage-minutes.sleep.v1".to_string()],
            ..Default::default()
        },
        ..Default::default()
    });

    assert!(result.output.is_none());
    assert!(
        result
            .errors
            .contains(&"sleep_stage_minutes_unknown_unrecognized".to_string())
    );
}

#[test]
fn bull_sleep_v1_rejects_impossible_stage_segment_timeline() {
    let result = bull_sleep_v1(&SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 390.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 450.0,
            input_ids: vec!["invalid-segment-timeline.sleep.v1".to_string()],
            ..Default::default()
        },
        stage_segments: vec![
            SleepStageSegment {
                stage_kind: "core".to_string(),
                start_time: "2026-05-27T22:00:00Z".to_string(),
                end_time: "2026-05-28T02:00:00Z".to_string(),
                duration_minutes: 240.0,
                confidence_0_to_1: 0.80,
                stage_probabilities: BTreeMap::from([("core".to_string(), 0.80)]),
            },
            SleepStageSegment {
                stage_kind: "deep".to_string(),
                start_time: "2026-05-28T01:30:00Z".to_string(),
                end_time: "2026-05-28T03:00:00Z".to_string(),
                duration_minutes: 45.0,
                confidence_0_to_1: 0.75,
                stage_probabilities: BTreeMap::from([("deep".to_string(), 0.75)]),
            },
            SleepStageSegment {
                stage_kind: "rem".to_string(),
                start_time: "2026-05-28T03:00:00Z".to_string(),
                end_time: "2026-05-28T08:00:00Z".to_string(),
                duration_minutes: 300.0,
                confidence_0_to_1: 0.70,
                stage_probabilities: BTreeMap::from([("rem".to_string(), 0.70)]),
            },
            SleepStageSegment {
                stage_kind: "awake".to_string(),
                start_time: "not-a-time".to_string(),
                end_time: "2026-05-28T04:00:00Z".to_string(),
                duration_minutes: 10.0,
                confidence_0_to_1: 0.60,
                stage_probabilities: BTreeMap::from([("awake".to_string(), 0.60)]),
            },
        ],
        ..Default::default()
    });

    assert!(result.output.is_none());
    assert!(
        result
            .errors
            .contains(&"stage_segments_0_outside_sleep_window".to_string())
    );
    assert!(
        result
            .errors
            .contains(&"stage_segments_1_duration_minutes_mismatch".to_string())
    );
    assert!(
        result
            .errors
            .contains(&"stage_segments_1_overlaps_previous_segment".to_string())
    );
    assert!(
        result
            .errors
            .contains(&"stage_segments_2_outside_sleep_window".to_string())
    );
    assert!(
        result
            .errors
            .contains(&"stage_segments_3_start_time_invalid".to_string())
    );
    assert!(
        result
            .errors
            .contains(&"stage_segments_total_duration_exceeds_time_in_bed".to_string())
    );
}

#[test]
fn bull_sleep_v1_rejects_impossible_sleep_window_math() {
    let result = bull_sleep_v1(&SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T02:30:00Z".to_string(),
            sleep_duration_minutes: 470.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 300.0,
            sleep_latency_minutes: 35.0,
            wake_after_sleep_onset_minutes: 20.0,
            input_ids: vec!["invalid-sleep-window.sleep.v1".to_string()],
            ..Default::default()
        },
        ..Default::default()
    });

    assert!(result.output.is_none());
    assert!(
        result
            .errors
            .contains(&"sleep_window_time_in_bed_minutes_mismatch".to_string())
    );
    assert!(
        result
            .errors
            .contains(&"sleep_window_sleep_duration_exceeds_time_in_bed".to_string())
    );
    assert!(
        result
            .errors
            .contains(&"sleep_window_sleep_latency_waso_duration_exceeds_time_in_bed".to_string())
    );
}

#[test]
fn bull_sleep_v1_rejects_invalid_sleep_window_timestamps() {
    let result = bull_sleep_v1(&SleepV1Input {
        sleep: SleepInput {
            start_time: "not-a-time".to_string(),
            end_time: "2026-05-28T02:30:00Z".to_string(),
            sleep_duration_minutes: 240.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 240.0,
            input_ids: vec!["invalid-sleep-window-time.sleep.v1".to_string()],
            ..Default::default()
        },
        ..Default::default()
    });

    assert!(result.output.is_none());
    assert!(
        result
            .errors
            .contains(&"sleep_window_start_time_invalid".to_string())
    );
}

#[test]
fn bull_sleep_v1_rejects_nonexistent_calendar_dates() {
    let result = bull_sleep_v1(&SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-02-30T22:30:00Z".to_string(),
            end_time: "2026-03-01T06:30:00Z".to_string(),
            sleep_duration_minutes: 420.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            input_ids: vec!["invalid-calendar-date.sleep.v1".to_string()],
            ..Default::default()
        },
        ..Default::default()
    });

    assert!(result.output.is_none());
    assert!(
        result
            .errors
            .contains(&"sleep_window_start_time_invalid".to_string())
    );
}

#[test]
fn sleep_baseline_from_history_derives_rolling_windows_and_exclusions() {
    let mut prior_nights = (0..8)
        .map(|index| sleep_history_night(index, 420.0 + index as f64 * 5.0, 0.90))
        .collect::<Vec<_>>();
    prior_nights.push(SleepNightHistoryInput {
        confidence_0_to_1: 0.40,
        excluded_from_baseline: true,
        ..sleep_history_night(99, 300.0, 0.40)
    });
    prior_nights.push(SleepNightHistoryInput {
        sleep_duration_minutes: 520.0,
        time_in_bed_minutes: 500.0,
        awake_minutes: 40.0,
        confidence_0_to_1: 0.90,
        ..sleep_history_night(100, 520.0, 0.90)
    });
    prior_nights.push(SleepNightHistoryInput {
        stage_minutes: BTreeMap::from([
            ("awake".to_string(), 120.0),
            ("core".to_string(), 300.0),
            ("deep".to_string(), 120.0),
            ("rem".to_string(), 120.0),
        ]),
        confidence_0_to_1: 0.90,
        ..sleep_history_night(101, 420.0, 0.90)
    });

    let baseline = sleep_baseline_from_history(&prior_nights).unwrap();

    assert_eq!(baseline.usable_night_count, 8);
    assert_eq!(baseline.excluded_night_count, 3);
    assert_close(baseline.rolling_sleep_debt_minutes, 340.0);
    let short = baseline.short_7_day.unwrap();
    assert_eq!(short.window_days, 7);
    assert_eq!(short.night_count, 7);
    assert_close(short.average_sleep_duration_minutes, 440.0);
    assert_close(short.average_sleep_debt_minutes, 40.0);
    assert_close(short.average_sleep_efficiency_fraction, 440.0 / 500.0);
    assert_close(short.average_deep_sleep_minutes, 75.0);
    assert_close(short.average_rem_sleep_minutes, 95.0);
    assert_close(short.average_restorative_sleep_minutes, 170.0);
    assert_close(short.average_sleep_hr_bpm.unwrap(), 59.0);
    assert_close(short.average_sleep_hr_trend_bpm_per_hour.unwrap(), -0.4);
    assert_close(short.average_hr_dip_percent.unwrap(), 11.0);
    let stable = baseline.stable_28_day.unwrap();
    assert_eq!(stable.night_count, 8);
    assert_close(stable.average_sleep_duration_minutes, 437.5);
}

#[test]
fn sleep_baseline_from_history_uses_latest_nights_by_timestamp() {
    let mut prior_nights = (0..8)
        .map(|index| {
            let mut night = sleep_history_night(index, 390.0 + index as f64 * 10.0, 0.90);
            night.stage_minutes = BTreeMap::new();
            night
        })
        .collect::<Vec<_>>();
    prior_nights.reverse();

    let baseline = sleep_baseline_from_history(&prior_nights).unwrap();
    let short = baseline.short_7_day.unwrap();
    let current = baseline.current_14_day.unwrap();

    assert_eq!(baseline.usable_night_count, 8);
    assert_eq!(short.night_count, 7);
    assert_close(short.average_sleep_duration_minutes, 430.0);
    assert_close(current.average_sleep_duration_minutes, 425.0);
}

#[test]
fn sleep_baseline_from_history_caps_rolling_sleep_debt_to_latest_28_nights() {
    let prior_nights = (0..35)
        .map(|index| {
            let mut night = sleep_history_night(index, if index < 7 { 300.0 } else { 450.0 }, 0.90);
            night.stage_minutes = BTreeMap::new();
            night.start_time = sleep_history_fixture_time(index + 1, "22:30:00");
            night.end_time = sleep_history_fixture_time(index + 2, "06:30:00");
            night
        })
        .collect::<Vec<_>>();

    let baseline = sleep_baseline_from_history(&prior_nights).unwrap();

    assert_eq!(baseline.usable_night_count, 35);
    assert_close(baseline.rolling_sleep_debt_minutes, 840.0);
    assert_close(
        baseline
            .stable_28_day
            .as_ref()
            .unwrap()
            .average_sleep_debt_minutes,
        30.0,
    );
}

#[test]
fn bull_sleep_v1_rejects_impossible_prior_night_duration_math() {
    let result = bull_sleep_v1(&SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 420.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            input_ids: vec!["invalid-prior-night.sleep.v1".to_string()],
            ..Default::default()
        },
        prior_nights: vec![SleepNightHistoryInput {
            sleep_duration_minutes: 520.0,
            time_in_bed_minutes: 500.0,
            awake_minutes: 40.0,
            ..sleep_history_night(0, 520.0, 0.90)
        }],
        ..Default::default()
    });

    assert!(result.output.is_none());
    assert!(
        result
            .errors
            .contains(&"prior_nights_0_sleep_duration_exceeds_time_in_bed".to_string())
    );
    assert!(result.errors.contains(
        &"prior_nights_0_sleep_duration_plus_awake_minutes_exceeds_time_in_bed".to_string()
    ));
}

#[test]
fn bull_sleep_v1_rejects_impossible_prior_night_stage_math() {
    let result = bull_sleep_v1(&SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 420.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            input_ids: vec!["invalid-prior-night-stages.sleep.v1".to_string()],
            ..Default::default()
        },
        prior_nights: vec![SleepNightHistoryInput {
            sleep_duration_minutes: 420.0,
            time_in_bed_minutes: 500.0,
            awake_minutes: 80.0,
            stage_minutes: BTreeMap::from([
                ("awake".to_string(), 90.0),
                ("core".to_string(), 300.0),
                ("deep".to_string(), 120.0),
                ("rem".to_string(), 120.0),
            ]),
            ..sleep_history_night(0, 420.0, 0.90)
        }],
        ..Default::default()
    });

    assert!(result.output.is_none());
    assert!(
        result
            .errors
            .contains(&"prior_nights_0_stage_minutes_exceed_time_in_bed".to_string())
    );
    assert!(
        result
            .errors
            .contains(&"prior_nights_0_asleep_stage_minutes_exceed_sleep_duration".to_string())
    );
}

#[test]
fn bull_sleep_v1_returns_baseline_from_prior_nights() {
    let prior_nights = (0..7)
        .map(|index| sleep_history_night(index, 420.0, 0.88))
        .collect::<Vec<_>>();
    let result = bull_sleep_v1(&SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 420.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            midpoint_deviation_minutes: 20.0,
            disturbance_count: 2,
            sleep_latency_minutes: 25.0,
            wake_after_sleep_onset_minutes: 35.0,
            wake_episode_count: 2,
            stage_minutes: BTreeMap::from([
                ("core".to_string(), 230.0),
                ("deep".to_string(), 80.0),
                ("rem".to_string(), 110.0),
            ]),
            heart_rate_dip_percent: Some(13.0),
            input_ids: vec!["sleep.v1.baseline.fixture".to_string()],
            ..Default::default()
        },
        model_status: SleepModelStatusInput {
            sleep_permission_granted: true,
            imported_platform_sleep_nights: 7,
            motion_coverage_fraction: Some(0.95),
            heart_rate_coverage_fraction: Some(0.90),
            ..Default::default()
        },
        prior_nights,
        sleep_hr_average_bpm: Some(61.0),
        sleep_hr_min_bpm: Some(54.0),
        pre_sleep_awake_hr_average_bpm: Some(68.0),
        sleep_hr_trend_bpm_per_hour: Some(-0.8),
        data_coverage_fraction: Some(0.95),
        ..Default::default()
    });

    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let output = result.output.unwrap();
    let baseline = output.baseline.unwrap();
    assert_eq!(baseline.usable_night_count, 7);
    assert_close(
        baseline.short_7_day.unwrap().average_sleep_duration_minutes,
        420.0,
    );
    assert_close(baseline.rolling_sleep_debt_minutes, 420.0);
    let previous = output.previous_night_comparison.unwrap();
    assert_eq!(previous.night_id, "sleep-history-6");
    assert_close(previous.sleep_duration_delta_minutes, 0.0);
    assert_close(previous.awake_minutes_delta, -20.0);
    assert_close(previous.sleep_debt_delta_minutes, 0.0);
    assert_close(previous.sleep_efficiency_delta_fraction, 0.035);
    assert_close(previous.sleep_latency_delta_minutes, 7.0);
    assert_close(previous.wake_after_sleep_onset_delta_minutes, -5.0);
    assert_eq!(previous.wake_episode_count_delta, -1);
    assert_close(previous.deep_sleep_delta_minutes, 5.0);
    assert_close(previous.rem_sleep_delta_minutes, 15.0);
    assert_close(previous.core_sleep_delta_minutes, -20.0);
    assert_close(previous.restorative_sleep_delta_minutes, 20.0);
    assert_close(previous.bedtime_deviation_delta_minutes, -15.0);
    assert_close(previous.wake_time_deviation_delta_minutes, -20.0);
    assert_close(previous.sleep_hr_average_delta_bpm.unwrap(), 2.0);
    assert_close(previous.sleep_hr_min_delta_bpm.unwrap(), 2.0);
    assert_close(previous.sleep_hr_trend_delta_bpm_per_hour.unwrap(), -0.4);
    assert_close(previous.sleep_hr_dip_delta_percent.unwrap(), 2.0);
    assert_eq!(
        output.provenance["previous_night_comparison"]["policy"],
        "latest_usable_prior_night_before_scored_sleep"
    );
    assert_eq!(
        output.provenance["previous_night_comparison"]["selected_night_id"],
        "sleep-history-6"
    );
    assert_eq!(
        output.provenance["previous_night_comparison"]["usable_prior_night_count"],
        7
    );
    assert_eq!(
        output.provenance["previous_night_comparison"]["fields"]
            .as_array()
            .unwrap()
            .len(),
        17
    );
}

#[test]
fn bull_sleep_v1_ignores_prior_nights_after_current_sleep_start() {
    let mut prior_nights = (0..7)
        .map(|index| sleep_history_night(index, 420.0, 0.88))
        .collect::<Vec<_>>();
    let mut future_night = sleep_history_night(20, 120.0, 0.95);
    future_night.night_id = "future-history-night".to_string();
    future_night.start_time = "2026-05-28T23:00:00Z".to_string();
    future_night.end_time = "2026-05-29T06:30:00Z".to_string();
    future_night.sleep_hr_average_bpm = Some(90.0);
    future_night.stage_minutes = BTreeMap::new();
    prior_nights.push(future_night);

    let result = bull_sleep_v1(&SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 420.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            midpoint_deviation_minutes: 20.0,
            disturbance_count: 2,
            stage_minutes: BTreeMap::from([
                ("core".to_string(), 230.0),
                ("deep".to_string(), 80.0),
                ("rem".to_string(), 110.0),
            ]),
            input_ids: vec!["sleep.v1.future-prior-night.fixture".to_string()],
            ..Default::default()
        },
        model_status: SleepModelStatusInput {
            sleep_permission_granted: true,
            imported_platform_sleep_nights: 8,
            motion_coverage_fraction: Some(0.95),
            heart_rate_coverage_fraction: Some(0.90),
            ..Default::default()
        },
        prior_nights,
        sleep_hr_average_bpm: Some(61.0),
        sleep_hr_trend_bpm_per_hour: Some(-0.8),
        data_coverage_fraction: Some(0.95),
        ..Default::default()
    });

    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(
        result
            .quality_flags
            .contains(&"sleep_v1_future_prior_nights_ignored".to_string())
    );
    let output = result.output.unwrap();
    let baseline = output.baseline.unwrap();
    assert_eq!(baseline.usable_night_count, 7);
    assert_close(
        baseline.short_7_day.unwrap().average_sleep_duration_minutes,
        420.0,
    );
    assert_eq!(
        output.previous_night_comparison.unwrap().night_id,
        "sleep-history-6"
    );
}

#[test]
fn bull_sleep_v1_uses_personal_baseline_for_architecture_and_hr_components() {
    let mut prior_nights = (0..14)
        .map(|index| sleep_history_night(index, 420.0, 0.90))
        .collect::<Vec<_>>();
    for night in &mut prior_nights {
        night.stage_minutes = BTreeMap::from([
            ("core".to_string(), 250.0),
            ("deep".to_string(), 75.0),
            ("rem".to_string(), 95.0),
        ]);
        night.sleep_hr_average_bpm = Some(58.0);
        night.sleep_hr_min_bpm = Some(51.0);
        night.heart_rate_dip_percent = Some(18.0);
    }

    let result = bull_sleep_v1(&SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 420.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            midpoint_deviation_minutes: 10.0,
            disturbance_count: 2,
            sleep_latency_minutes: 12.0,
            wake_after_sleep_onset_minutes: 30.0,
            wake_episode_count: 2,
            stage_minutes: BTreeMap::from([
                ("core".to_string(), 250.0),
                ("deep".to_string(), 75.0),
                ("rem".to_string(), 95.0),
            ]),
            heart_rate_dip_percent: Some(18.0),
            input_ids: vec!["sleep.v1.personal-baseline.fixture".to_string()],
        },
        model_status: SleepModelStatusInput {
            sleep_permission_granted: true,
            imported_platform_sleep_nights: 14,
            trusted_bull_sleep_nights: 2,
            motion_coverage_fraction: Some(0.96),
            heart_rate_coverage_fraction: Some(0.92),
            ..Default::default()
        },
        prior_nights,
        bedtime_deviation_minutes: 8.0,
        wake_time_deviation_minutes: 12.0,
        sleep_hr_average_bpm: Some(58.0),
        sleep_hr_min_bpm: Some(51.0),
        data_coverage_fraction: Some(0.96),
        ..Default::default()
    });

    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let output = result.output.unwrap();
    let architecture = output
        .components
        .iter()
        .find(|component| component.name == "sleep_architecture")
        .unwrap();
    let cardiovascular = output
        .components
        .iter()
        .find(|component| component.name == "cardiovascular_recovery")
        .unwrap();
    assert_close(architecture.score_0_to_100, 100.0);
    assert_close(cardiovascular.score_0_to_100, 95.5);
}

#[test]
fn bull_sleep_v1_blends_stage_priors_by_baseline_maturity_and_confidence() {
    let mut prior_nights = (0..7)
        .map(|index| sleep_history_night(index, 420.0, 0.55))
        .collect::<Vec<_>>();
    for night in &mut prior_nights {
        night.stage_minutes = BTreeMap::from([
            ("core".to_string(), 410.0),
            ("deep".to_string(), 5.0),
            ("rem".to_string(), 5.0),
        ]);
    }

    let result = bull_sleep_v1(&SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 420.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            midpoint_deviation_minutes: 10.0,
            disturbance_count: 2,
            sleep_latency_minutes: 12.0,
            wake_after_sleep_onset_minutes: 30.0,
            wake_episode_count: 2,
            stage_minutes: BTreeMap::from([
                ("core".to_string(), 410.0),
                ("deep".to_string(), 5.0),
                ("rem".to_string(), 5.0),
            ]),
            heart_rate_dip_percent: Some(16.0),
            input_ids: vec!["sleep.v1.stage-prior.fixture".to_string()],
        },
        model_status: SleepModelStatusInput {
            sleep_permission_granted: true,
            imported_platform_sleep_nights: 7,
            trusted_bull_sleep_nights: 1,
            motion_coverage_fraction: Some(0.96),
            heart_rate_coverage_fraction: Some(0.92),
            ..Default::default()
        },
        prior_nights,
        bedtime_deviation_minutes: 8.0,
        wake_time_deviation_minutes: 12.0,
        sleep_hr_average_bpm: Some(58.0),
        sleep_hr_min_bpm: Some(51.0),
        data_coverage_fraction: Some(0.96),
        ..Default::default()
    });

    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let output = result.output.unwrap();
    let architecture = output
        .components
        .iter()
        .find(|component| component.name == "sleep_architecture")
        .unwrap();
    assert!(architecture.score_0_to_100 < 10.0);
    assert_close(architecture.score_0_to_100, 6.093358395989973);
    let stage_prior =
        &output.component_provenance["sleep_architecture"]["inputs"]["stage_prior_calibration"];
    assert_eq!(
        stage_prior["source"],
        "personal_stage_baseline_blended_with_population_prior"
    );
    assert_close(
        stage_prior["personal_prior_weight"].as_f64().unwrap(),
        0.017857142857142835,
    );
    assert_close(
        stage_prior["population_prior_weight"].as_f64().unwrap(),
        0.9821428571428572,
    );
}

#[test]
fn bull_sleep_v1_scores_overnight_hr_trend_against_personal_baseline() {
    let mut prior_nights = (0..14)
        .map(|index| sleep_history_night(index, 420.0, 0.90))
        .collect::<Vec<_>>();
    for night in &mut prior_nights {
        night.sleep_hr_average_bpm = Some(58.0);
        night.sleep_hr_min_bpm = Some(51.0);
        night.sleep_hr_trend_bpm_per_hour = Some(-0.5);
        night.heart_rate_dip_percent = Some(18.0);
    }

    let mut input = SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 420.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            midpoint_deviation_minutes: 10.0,
            disturbance_count: 2,
            sleep_latency_minutes: 12.0,
            wake_after_sleep_onset_minutes: 30.0,
            wake_episode_count: 2,
            heart_rate_dip_percent: Some(18.0),
            input_ids: vec!["sleep.v1.hr-trend.fixture".to_string()],
            ..Default::default()
        },
        model_status: SleepModelStatusInput {
            sleep_permission_granted: true,
            imported_platform_sleep_nights: 14,
            trusted_bull_sleep_nights: 2,
            motion_coverage_fraction: Some(0.96),
            heart_rate_coverage_fraction: Some(0.92),
            ..Default::default()
        },
        prior_nights,
        sleep_hr_average_bpm: Some(58.0),
        sleep_hr_min_bpm: Some(51.0),
        sleep_hr_trend_bpm_per_hour: Some(3.0),
        data_coverage_fraction: Some(0.96),
        ..Default::default()
    };
    let rising = bull_sleep_v1(&input).output.unwrap();
    let rising_cardio = rising
        .components
        .iter()
        .find(|component| component.name == "cardiovascular_recovery")
        .unwrap()
        .score_0_to_100;

    input.sleep_hr_trend_bpm_per_hour = Some(-1.5);
    let falling = bull_sleep_v1(&input).output.unwrap();
    let falling_cardio = falling
        .components
        .iter()
        .find(|component| component.name == "cardiovascular_recovery")
        .unwrap()
        .score_0_to_100;

    assert!(
        falling_cardio > rising_cardio,
        "{falling_cardio} <= {rising_cardio}"
    );
    assert_close(falling.sleep_hr_trend_bpm_per_hour.unwrap(), -1.5);
    assert_eq!(
        falling.component_provenance["cardiovascular_recovery"]["inputs"]["sleep_hr_trend_bpm_per_hour"],
        -1.5
    );
}

#[test]
fn bull_sleep_v1_uses_overnight_hr_trend_before_personal_baseline() {
    let mut input = SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 420.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            midpoint_deviation_minutes: 10.0,
            disturbance_count: 2,
            sleep_latency_minutes: 12.0,
            wake_after_sleep_onset_minutes: 30.0,
            wake_episode_count: 2,
            heart_rate_dip_percent: Some(12.0),
            input_ids: vec!["sleep.v1.no-baseline-hr-trend.fixture".to_string()],
            ..Default::default()
        },
        model_status: SleepModelStatusInput {
            sleep_permission_granted: true,
            trusted_bull_sleep_nights: 1,
            motion_coverage_fraction: Some(0.94),
            heart_rate_coverage_fraction: Some(0.90),
            ..Default::default()
        },
        sleep_hr_trend_bpm_per_hour: Some(3.0),
        data_coverage_fraction: Some(0.94),
        ..Default::default()
    };
    let rising = bull_sleep_v1(&input).output.unwrap();
    let rising_cardio = rising
        .components
        .iter()
        .find(|component| component.name == "cardiovascular_recovery")
        .unwrap()
        .score_0_to_100;

    input.sleep_hr_trend_bpm_per_hour = Some(-1.5);
    let falling = bull_sleep_v1(&input).output.unwrap();
    let falling_cardio = falling
        .components
        .iter()
        .find(|component| component.name == "cardiovascular_recovery")
        .unwrap()
        .score_0_to_100;

    assert!(
        falling_cardio > rising_cardio,
        "{falling_cardio} <= {rising_cardio}"
    );
    assert_eq!(
        falling.component_provenance["cardiovascular_recovery"]["policy"],
        "hr_dip_pre_sleep_awake_hr_overnight_trend_and_personal_baseline_when_available"
    );
    assert_eq!(
        falling.component_provenance["cardiovascular_recovery"]["inputs"]["sleep_hr_trend_bpm_per_hour"],
        -1.5
    );
}

#[test]
fn bull_sleep_v1_uses_pre_sleep_awake_hr_in_cardiovascular_recovery() {
    let mut input = SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 420.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            midpoint_deviation_minutes: 10.0,
            disturbance_count: 2,
            sleep_latency_minutes: 12.0,
            wake_after_sleep_onset_minutes: 30.0,
            wake_episode_count: 2,
            heart_rate_dip_percent: Some(12.0),
            input_ids: vec!["sleep.v1.pre-sleep-hr.fixture".to_string()],
            ..Default::default()
        },
        model_status: SleepModelStatusInput {
            sleep_permission_granted: true,
            trusted_bull_sleep_nights: 1,
            motion_coverage_fraction: Some(0.94),
            heart_rate_coverage_fraction: Some(0.90),
            ..Default::default()
        },
        sleep_hr_average_bpm: Some(62.0),
        pre_sleep_awake_hr_average_bpm: Some(58.0),
        data_coverage_fraction: Some(0.94),
        ..Default::default()
    };
    let elevated = bull_sleep_v1(&input).output.unwrap();
    let elevated_cardio = elevated
        .components
        .iter()
        .find(|component| component.name == "cardiovascular_recovery")
        .unwrap()
        .score_0_to_100;

    input.pre_sleep_awake_hr_average_bpm = Some(72.0);
    let recovered = bull_sleep_v1(&input).output.unwrap();
    let recovered_cardio = recovered
        .components
        .iter()
        .find(|component| component.name == "cardiovascular_recovery")
        .unwrap()
        .score_0_to_100;

    assert!(
        recovered_cardio > elevated_cardio,
        "{recovered_cardio} <= {elevated_cardio}"
    );
    assert_close(recovered.pre_sleep_awake_hr_average_bpm.unwrap(), 72.0);
    assert_eq!(
        recovered.component_provenance["cardiovascular_recovery"]["inputs"]["pre_sleep_awake_hr_average_bpm"],
        72.0
    );
    assert_eq!(
        recovered.component_provenance["cardiovascular_recovery"]["policy"],
        "hr_dip_pre_sleep_awake_hr_overnight_trend_and_personal_baseline_when_available"
    );
}

#[test]
fn bull_sleep_v1_guardrails_very_short_and_fragmented_sleep() {
    let result = bull_sleep_v1(&SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 150.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            midpoint_deviation_minutes: 20.0,
            disturbance_count: 12,
            sleep_latency_minutes: 60.0,
            wake_after_sleep_onset_minutes: 180.0,
            wake_episode_count: 12,
            stage_minutes: BTreeMap::from([
                ("awake".to_string(), 330.0),
                ("core".to_string(), 120.0),
                ("deep".to_string(), 15.0),
                ("rem".to_string(), 15.0),
            ]),
            heart_rate_dip_percent: Some(4.0),
            input_ids: vec!["sleep.v1.guardrail.fixture".to_string()],
        },
        model_status: SleepModelStatusInput {
            sleep_permission_granted: true,
            trusted_bull_sleep_nights: 1,
            motion_coverage_fraction: Some(0.90),
            heart_rate_coverage_fraction: Some(0.85),
            ..Default::default()
        },
        bedtime_deviation_minutes: 20.0,
        wake_time_deviation_minutes: 25.0,
        data_coverage_fraction: Some(0.90),
        ..Default::default()
    });

    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(result.output.unwrap().score_0_to_100 <= 45.0);
    assert!(
        result
            .quality_flags
            .contains(&"sleep_v1_guardrail_very_short_sleep".to_string())
    );
    assert!(
        result
            .quality_flags
            .contains(&"sleep_v1_guardrail_severe_fragmentation".to_string())
    );
}

#[test]
fn bull_sleep_v1_edge_cases_all_awake_no_hr_missing_stages_and_timestamp_blocked() {
    let all_awake = bull_sleep_v1(&SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 60.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            midpoint_deviation_minutes: 180.0,
            disturbance_count: 16,
            sleep_latency_minutes: 180.0,
            wake_after_sleep_onset_minutes: 240.0,
            wake_episode_count: 16,
            stage_minutes: BTreeMap::from([("awake".to_string(), 480.0)]),
            input_ids: vec!["sleep.v1.all-awake.fixture".to_string()],
            ..Default::default()
        },
        model_status: SleepModelStatusInput {
            sleep_permission_granted: true,
            trusted_bull_sleep_nights: 1,
            motion_coverage_fraction: Some(0.86),
            heart_rate_coverage_fraction: Some(0.0),
            timestamp_sync_blocked: true,
            ..Default::default()
        },
        data_coverage_fraction: Some(0.86),
        ..Default::default()
    });

    assert!(all_awake.errors.is_empty(), "{:?}", all_awake.errors);
    assert!(
        all_awake
            .quality_flags
            .contains(&"sleep_v1_status_blocked".to_string())
    );
    let all_awake_output = all_awake.output.unwrap();
    assert_eq!(all_awake_output.model_status, SleepModelStatus::Blocked);
    assert!(all_awake_output.score_0_to_100 <= 45.0);
    assert!(all_awake_output.sleep_hr_dip_percent.is_none());
    assert_close(all_awake_output.data_coverage_fraction.unwrap(), 0.86);

    let missing_stages = bull_sleep_v1(&SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 390.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            midpoint_deviation_minutes: 35.0,
            disturbance_count: 3,
            sleep_latency_minutes: 20.0,
            wake_after_sleep_onset_minutes: 40.0,
            wake_episode_count: 2,
            input_ids: vec!["sleep.v1.no-stage-no-hr.fixture".to_string()],
            ..Default::default()
        },
        model_status: SleepModelStatusInput {
            sleep_permission_granted: true,
            trusted_bull_sleep_nights: 2,
            motion_coverage_fraction: Some(0.91),
            heart_rate_coverage_fraction: Some(0.0),
            ..Default::default()
        },
        data_coverage_fraction: Some(0.91),
        ..Default::default()
    });

    assert!(
        missing_stages.errors.is_empty(),
        "{:?}",
        missing_stages.errors
    );
    assert!(
        missing_stages
            .quality_flags
            .contains(&"sleep_architecture_unavailable".to_string())
    );
    let missing_stage_output = missing_stages.output.unwrap();
    let architecture = missing_stage_output
        .components
        .iter()
        .find(|component| component.name == "sleep_architecture")
        .unwrap();
    let cardiovascular = missing_stage_output
        .components
        .iter()
        .find(|component| component.name == "cardiovascular_recovery")
        .unwrap();
    assert_close(architecture.score_0_to_100, 55.0);
    assert_close(cardiovascular.score_0_to_100, 60.0);
    assert!(missing_stage_output.sleep_hr_recovery_score.is_none());
}

#[test]
fn bull_sleep_v1_input_and_output_round_trip_json() {
    let input = SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 420.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            midpoint_deviation_minutes: 30.0,
            disturbance_count: 4,
            sleep_latency_minutes: 18.0,
            wake_after_sleep_onset_minutes: 42.0,
            wake_episode_count: 2,
            stage_minutes: BTreeMap::from([
                ("core".to_string(), 210.0),
                ("deep".to_string(), 90.0),
                ("rem".to_string(), 120.0),
            ]),
            heart_rate_dip_percent: Some(12.5),
            input_ids: vec!["sleep.v1.roundtrip.fixture".to_string()],
        },
        model_status: SleepModelStatusInput {
            sleep_permission_granted: true,
            imported_platform_sleep_nights: 7,
            trusted_bull_sleep_nights: 2,
            motion_coverage_fraction: Some(0.94),
            heart_rate_coverage_fraction: Some(0.82),
            ..Default::default()
        },
        prior_nights: vec![sleep_history_night(0, 420.0, 0.90)],
        stage_segments: vec![SleepStageSegment {
            stage_kind: "deep".to_string(),
            start_time: "2026-05-28T01:00:00Z".to_string(),
            end_time: "2026-05-28T01:45:00Z".to_string(),
            duration_minutes: 45.0,
            confidence_0_to_1: 0.88,
            stage_probabilities: BTreeMap::from([
                ("core".to_string(), 0.10),
                ("deep".to_string(), 0.86),
                ("rem".to_string(), 0.04),
            ]),
        }],
        rolling_sleep_debt_minutes: 120.0,
        bedtime_deviation_minutes: 20.0,
        wake_time_deviation_minutes: 15.0,
        sleep_hr_average_bpm: Some(61.0),
        sleep_hr_min_bpm: Some(54.0),
        pre_sleep_awake_hr_average_bpm: Some(68.0),
        sleep_hr_trend_bpm_per_hour: Some(-0.8),
        naps_minutes: 25.0,
        prior_day_strain: Some(8.5),
        data_coverage_fraction: Some(0.92),
    };

    let serialized_input = serde_json::to_value(&input).unwrap();
    let input_round_trip: SleepV1Input = serde_json::from_value(serialized_input).unwrap();
    assert_eq!(input_round_trip, input);

    let output = bull_sleep_v1(&input).output.unwrap();
    let serialized_output = serde_json::to_value(&output).unwrap();
    assert_eq!(
        serialized_output["component_provenance"]["sleep_need_fulfillment"]["policy"],
        "duration_vs_need_with_debt_pressure_and_nap_credit"
    );
    assert_eq!(
        serialized_output["quality_flags"],
        serde_json::to_value(&output.quality_flags).unwrap()
    );
    assert_eq!(
        serialized_output["provenance"]["score_policy"],
        "weighted_sleep_v1_components_with_fragmentation_guardrails"
    );
    let output_round_trip: SleepV1Output = serde_json::from_value(serialized_output).unwrap();
    assert_eq!(output_round_trip, output);
}

#[test]
fn bull_strain_v0_computes_hand_derived_zone_and_hr_reserve_score() {
    let result = bull_strain_v0(&StrainInput {
        start_time: "2026-05-28T12:00:00Z".to_string(),
        end_time: "2026-05-28T13:00:00Z".to_string(),
        duration_minutes: 60.0,
        resting_hr_bpm: 60.0,
        average_hr_bpm: 120.0,
        max_hr_bpm: 180.0,
        hr_zone_minutes: vec![10.0, 20.0, 30.0, 0.0, 0.0],
        input_ids: vec!["hand-derived.strain".to_string()],
        ..Default::default()
    });

    let output = result.output.unwrap();
    assert_eq!(output.algorithm_id, BULL_STRAIN_V0_ID);
    assert_close(output.zone_load, 140.0);
    assert_close(output.average_hr_reserve_fraction, 0.5);
    assert_close(output.score_0_to_21, 8.05);
}

#[test]
fn bull_strain_v0_is_monotonic_when_minutes_move_to_higher_zone() {
    let easy = bull_strain_v0(&StrainInput {
        start_time: "2026-05-28T12:00:00Z".to_string(),
        end_time: "2026-05-28T13:00:00Z".to_string(),
        duration_minutes: 60.0,
        resting_hr_bpm: 60.0,
        average_hr_bpm: 120.0,
        max_hr_bpm: 180.0,
        hr_zone_minutes: vec![60.0, 0.0, 0.0, 0.0, 0.0],
        input_ids: Vec::new(),
        ..Default::default()
    })
    .output
    .unwrap();
    let hard = bull_strain_v0(&StrainInput {
        start_time: "2026-05-28T12:00:00Z".to_string(),
        end_time: "2026-05-28T13:00:00Z".to_string(),
        duration_minutes: 60.0,
        resting_hr_bpm: 60.0,
        average_hr_bpm: 120.0,
        max_hr_bpm: 180.0,
        hr_zone_minutes: vec![0.0, 0.0, 0.0, 0.0, 60.0],
        input_ids: Vec::new(),
        ..Default::default()
    })
    .output
    .unwrap();

    assert!(hard.score_0_to_21 > easy.score_0_to_21);
}

#[test]
fn bull_recovery_v0_computes_hand_derived_interpretable_composite() {
    let result = bull_recovery_v0(&RecoveryInput {
        start_time: "2026-05-28T00:00:00Z".to_string(),
        end_time: "2026-05-28T08:00:00Z".to_string(),
        hrv_rmssd_ms: 50.0,
        hrv_baseline_rmssd_ms: 50.0,
        resting_hr_bpm: 60.0,
        resting_hr_baseline_bpm: 60.0,
        respiratory_rate_rpm: Some(14.0),
        respiratory_rate_baseline_rpm: Some(14.0),
        skin_temp_delta_c: Some(0.0),
        sleep_score_0_to_100: 80.0,
        prior_strain_0_to_21: 10.5,
        input_ids: vec!["hand-derived.recovery".to_string()],
    });

    let output = result.output.unwrap();
    assert_eq!(output.algorithm_id, BULL_RECOVERY_V0_ID);
    assert_close(output.score_0_to_100, 77.5);
    assert_eq!(output.components.len(), 6);
}

#[test]
fn bull_recovery_v0_flags_low_sleep_and_high_prior_strain() {
    let result = bull_recovery_v0(&RecoveryInput {
        start_time: "2026-05-28T00:00:00Z".to_string(),
        end_time: "2026-05-28T08:00:00Z".to_string(),
        hrv_rmssd_ms: 45.0,
        hrv_baseline_rmssd_ms: 50.0,
        resting_hr_bpm: 62.0,
        resting_hr_baseline_bpm: 60.0,
        respiratory_rate_rpm: Some(15.0),
        respiratory_rate_baseline_rpm: Some(14.0),
        skin_temp_delta_c: Some(0.3),
        sleep_score_0_to_100: 55.0,
        prior_strain_0_to_21: 15.0,
        input_ids: Vec::new(),
    });

    assert!(result.output.is_some());
    assert!(
        result
            .quality_flags
            .contains(&"low_sleep_score".to_string())
    );
    assert!(
        result
            .quality_flags
            .contains(&"high_prior_strain".to_string())
    );
}

#[test]
fn bull_stress_v0_computes_hand_derived_hr_and_hrv_score() {
    let result = bull_stress_v0(&StressInput {
        start_time: "2026-05-28T12:00:00Z".to_string(),
        end_time: "2026-05-28T12:05:00Z".to_string(),
        heart_rate_bpm: 90.0,
        resting_hr_bpm: 60.0,
        hrv_rmssd_ms: 25.0,
        hrv_baseline_rmssd_ms: 50.0,
        motion_intensity_0_to_1: 0.0,
        input_ids: vec!["hand-derived.stress".to_string()],
    });

    let output = result.output.unwrap();
    assert_eq!(output.algorithm_id, BULL_STRESS_V0_ID);
    assert_close(output.heart_rate_elevation_score, 50.0);
    assert_close(output.hrv_suppression_score, 50.0);
    assert_close(output.score_0_to_100, 50.0);
}

#[test]
fn bull_stress_v0_lowers_hr_contribution_when_motion_explains_elevation() {
    let still = bull_stress_v0(&StressInput {
        start_time: "2026-05-28T12:00:00Z".to_string(),
        end_time: "2026-05-28T12:05:00Z".to_string(),
        heart_rate_bpm: 90.0,
        resting_hr_bpm: 60.0,
        hrv_rmssd_ms: 25.0,
        hrv_baseline_rmssd_ms: 50.0,
        motion_intensity_0_to_1: 0.0,
        input_ids: Vec::new(),
    })
    .output
    .unwrap();
    let moving = bull_stress_v0(&StressInput {
        start_time: "2026-05-28T12:00:00Z".to_string(),
        end_time: "2026-05-28T12:05:00Z".to_string(),
        heart_rate_bpm: 90.0,
        resting_hr_bpm: 60.0,
        hrv_rmssd_ms: 25.0,
        hrv_baseline_rmssd_ms: 50.0,
        motion_intensity_0_to_1: 1.0,
        input_ids: Vec::new(),
    })
    .output
    .unwrap();

    assert!(moving.score_0_to_100 < still.score_0_to_100);
}

#[test]
fn score_family_run_record_persists_to_sqlite() {
    let store = BullStore::open_in_memory().unwrap();
    for definition in built_in_algorithm_definitions() {
        store.upsert_algorithm_definition(&definition).unwrap();
    }

    let result = bull_sleep_v0(&SleepInput {
        start_time: "2026-05-27T22:30:00Z".to_string(),
        end_time: "2026-05-28T06:30:00Z".to_string(),
        sleep_duration_minutes: 420.0,
        sleep_need_minutes: 480.0,
        time_in_bed_minutes: 480.0,
        midpoint_deviation_minutes: 30.0,
        disturbance_count: 4,
        input_ids: vec!["fixture.synthetic.sleep".to_string()],
        ..Default::default()
    });
    let record = algorithm_run_record("sleep-run-1", &result).unwrap();
    assert!(store.insert_algorithm_run(&record).unwrap());

    let saved_run = store.algorithm_run("sleep-run-1").unwrap().unwrap();
    assert_eq!(saved_run.algorithm_id, BULL_SLEEP_V0_ID);
    assert!(saved_run.output_json.contains("\"score_0_to_100\""));
    assert!(saved_run.provenance_json.contains("hand-derived-tests"));
}

#[test]
fn flagship_score_fixtures_match_hand_derived_expected_outputs() {
    let sleep: SleepInput = serde_json::from_str(include_str!(
        "../fixtures/synthetic/sleep_bull_v0_hand_derived.json"
    ))
    .unwrap();
    assert_close(
        bull_sleep_v0(&sleep).output.unwrap().score_0_to_100,
        84.875,
    );

    let strain: StrainInput = serde_json::from_str(include_str!(
        "../fixtures/synthetic/strain_bull_v0_hand_derived.json"
    ))
    .unwrap();
    assert_close(bull_strain_v0(&strain).output.unwrap().score_0_to_21, 8.05);

    let recovery: RecoveryInput = serde_json::from_str(include_str!(
        "../fixtures/synthetic/recovery_bull_v0_hand_derived.json"
    ))
    .unwrap();
    assert_close(
        bull_recovery_v0(&recovery).output.unwrap().score_0_to_100,
        77.5,
    );

    let stress: StressInput = serde_json::from_str(include_str!(
        "../fixtures/synthetic/stress_bull_v0_hand_derived.json"
    ))
    .unwrap();
    assert_close(
        bull_stress_v0(&stress).output.unwrap().score_0_to_100,
        50.0,
    );
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "expected {expected}, got {actual}"
    );
}

fn sleep_history_fixture_time(day_number: u32, time: &str) -> String {
    if day_number <= 30 {
        format!("2026-04-{day_number:02}T{time}Z")
    } else {
        format!("2026-05-{day:02}T{time}Z", day = day_number - 30)
    }
}

fn sleep_history_night(
    index: u32,
    sleep_duration_minutes: f64,
    confidence_0_to_1: f64,
) -> SleepNightHistoryInput {
    SleepNightHistoryInput {
        night_id: format!("sleep-history-{index}"),
        start_time: format!("2026-05-{index:02}T22:30:00Z", index = index + 1),
        end_time: format!("2026-05-{index:02}T06:30:00Z", index = index + 2),
        sleep_duration_minutes,
        sleep_need_minutes: 480.0,
        time_in_bed_minutes: 500.0,
        awake_minutes: 500.0 - sleep_duration_minutes,
        sleep_latency_minutes: 18.0,
        wake_after_sleep_onset_minutes: 40.0,
        wake_episode_count: 3,
        stage_minutes: BTreeMap::from([
            ("core".to_string(), 250.0),
            ("deep".to_string(), 75.0),
            ("rem".to_string(), 95.0),
        ]),
        heart_rate_dip_percent: Some(11.0),
        sleep_hr_average_bpm: Some(59.0),
        sleep_hr_min_bpm: Some(52.0),
        pre_sleep_awake_hr_average_bpm: Some(66.0),
        sleep_hr_trend_bpm_per_hour: Some(-0.4),
        bedtime_deviation_minutes: 15.0,
        wake_time_deviation_minutes: 20.0,
        midpoint_deviation_minutes: 10.0,
        naps_minutes: 0.0,
        confidence_0_to_1,
        source: "healthkit".to_string(),
        excluded_from_baseline: false,
    }
}

// --- strain v1 cluster (ported) ---
#[test]
fn tanaka_hrmax_returns_exact_value_for_age_50() {
    let hrmax = tanaka_hrmax(50.0);
    assert_eq!(
        hrmax,
        208.0 - 0.7 * 50.0,
        "tanaka_hrmax(50) must equal 173.0 exactly"
    );
    assert_eq!(hrmax, 173.0);
}

#[test]
fn tanaka_hrmax_differs_from_220_minus_age_by_at_least_2_for_ages_47_to_80() {
    // tanaka - classic = (208 - 0.7*age) - (220 - age) = -12 + 0.3*age
    // >= 2 when age >= 46.67, i.e. integer age >= 47
    for age in 47u32..=80 {
        let age_f = age as f64;
        let tanaka = tanaka_hrmax(age_f);
        let classic = 220.0 - age_f;
        assert!(
            (tanaka - classic).abs() >= 2.0,
            "tanaka_hrmax({age}) = {tanaka} vs classic = {classic}: difference must be >= 2 bpm"
        );
    }
}

#[test]
fn estimate_hrmax_from_history_returns_none_with_599_samples() {
    let samples: Vec<f64> = (0..599).map(|i| 100.0 + i as f64 * 0.1).collect();
    let result = estimate_hrmax_from_history(&samples);
    assert!(result.is_none(), "must return None for < 600 samples");
}

#[test]
fn estimate_hrmax_from_history_returns_some_with_600_samples() {
    let samples: Vec<f64> = (0..600).map(|i| 100.0 + i as f64 * 0.1).collect();
    let result = estimate_hrmax_from_history(&samples);
    assert!(result.is_some(), "must return Some for >= 600 samples");
}

#[test]
fn estimate_hrmax_from_history_returns_p99_5_percentile() {
    let mut samples: Vec<f64> = (0..600).map(|i| i as f64).collect();
    // Sort ascending to know expected index
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let len = samples.len() as f64;
    // CR-01 fix: nearest-rank P99.5 is ceil(0.995*n) - 1 (0-indexed).
    let expected_index = ((0.995 * len).ceil() as usize)
        .saturating_sub(1)
        .min(samples.len() - 1);
    let expected_value = samples[expected_index];

    let result = estimate_hrmax_from_history(&samples);
    assert_eq!(result.unwrap(), expected_value);
}

#[test]
fn estimate_hrmax_from_history_ignores_non_finite_samples() {
    // Only 599 finite + some NaN/inf — must return None (< 600 finite)
    let mut samples: Vec<f64> = (0..599).map(|i| 100.0 + i as f64 * 0.1).collect();
    samples.push(f64::NAN);
    samples.push(f64::INFINITY);
    let result = estimate_hrmax_from_history(&samples);
    assert!(
        result.is_none(),
        "must return None when fewer than 600 finite samples"
    );
}

// ── ALG-STR-01: Task 2 tests ────────────────────────────────────────────────

#[test]
fn resolve_effective_hrmax_returns_observed_from_history() {
    // 600 samples: sufficient history → source = "observed"
    let history: Vec<f64> = (0..600).map(|i| 100.0 + i as f64 * 0.1).collect();
    let (hrmax, source) = resolve_effective_hrmax(175.0, Some(35.0), &history);
    assert_eq!(source, "observed");
    // value must equal estimate_hrmax_from_history result
    let expected = estimate_hrmax_from_history(&history).unwrap();
    assert_eq!(hrmax, expected);
}

#[test]
fn resolve_effective_hrmax_returns_tanaka_when_history_insufficient_and_age_present() {
    // < 600 samples, profile_age = Some(40) → source = "tanaka"
    let history: Vec<f64> = (0..100).map(|i| 150.0 + i as f64 * 0.1).collect();
    let session_max_hr = 175.0;
    let age = 40.0;
    let (hrmax, source) = resolve_effective_hrmax(session_max_hr, Some(age), &history);
    assert_eq!(source, "tanaka");
    let expected = session_max_hr.max(tanaka_hrmax(age));
    assert_eq!(hrmax, expected);
}

#[test]
fn resolve_effective_hrmax_returns_fallback_when_no_history_and_no_age() {
    // < 600 samples, no profile_age → source = "fallback"
    let history: Vec<f64> = vec![150.0, 160.0];
    let session_max_hr = 178.0;
    let (hrmax, source) = resolve_effective_hrmax(session_max_hr, None, &history);
    assert_eq!(source, "fallback");
    assert_eq!(hrmax, session_max_hr);
}

#[test]
fn resolve_effective_hrmax_source_is_one_of_known_labels() {
    let valid_sources = ["observed", "tanaka", "fallback"];

    // observed path
    let history_600: Vec<f64> = (0..600).map(|i| 100.0 + i as f64 * 0.1).collect();
    let (_, source) = resolve_effective_hrmax(175.0, Some(35.0), &history_600);
    assert!(valid_sources.contains(&source.as_str()), "got: {source}");

    // tanaka path
    let history_small: Vec<f64> = vec![150.0; 50];
    let (_, source) = resolve_effective_hrmax(175.0, Some(35.0), &history_small);
    assert!(valid_sources.contains(&source.as_str()), "got: {source}");

    // fallback path
    let (_, source) = resolve_effective_hrmax(175.0, None, &history_small);
    assert!(valid_sources.contains(&source.as_str()), "got: {source}");
}

// ── ALG-STR-02: banister_trimp_zone_midpoint tests ─────────────────────────

#[test]
fn banister_trimp_male_greater_than_female_for_identical_zones() {
    // Male b=1.92 > female b=1.67 → male TRIMP larger for identical inputs
    let zones = vec![10.0, 20.0, 15.0, 10.0, 5.0];
    let resting_hr = 55.0;
    let hrmax = 185.0;
    let male = banister_trimp_zone_midpoint(&zones, resting_hr, hrmax, Some("male"));
    let female = banister_trimp_zone_midpoint(&zones, resting_hr, hrmax, Some("female"));
    assert!(
        male > female,
        "expected male TRIMP ({male}) > female TRIMP ({female})"
    );
}

#[test]
fn banister_trimp_unknown_sex_lies_between_male_and_female() {
    // unknown/None sex uses b=1.795 → result strictly between male and female
    let zones = vec![10.0, 20.0, 15.0, 10.0, 5.0];
    let resting_hr = 55.0;
    let hrmax = 185.0;
    let male = banister_trimp_zone_midpoint(&zones, resting_hr, hrmax, Some("male"));
    let female = banister_trimp_zone_midpoint(&zones, resting_hr, hrmax, Some("female"));
    let unknown_none = banister_trimp_zone_midpoint(&zones, resting_hr, hrmax, None);
    let unknown_str = banister_trimp_zone_midpoint(&zones, resting_hr, hrmax, Some("other"));
    assert!(
        unknown_none > female && unknown_none < male,
        "expected female ({female}) < unknown_none ({unknown_none}) < male ({male})"
    );
    assert_eq!(
        unknown_none, unknown_str,
        "None and unknown string must use same b constant"
    );
}

#[test]
fn banister_trimp_zone_midpoints_and_hrr_fraction_formula() {
    // Verify formula manually for a single zone scenario:
    // zone_mid_hr = 0.55 * hrmax; x = (zone_mid - resting) / (hrmax - resting)
    // contribution = minutes * x * 0.64 * exp(b * x)
    let hrmax = 200.0;
    let resting_hr = 60.0;
    let b_male = 1.92_f64;
    // All 60 minutes in zone 1, zones 2-5 = 0
    let zones = vec![60.0, 0.0, 0.0, 0.0, 0.0];
    let zone1_mid = 0.55_f64 * hrmax;
    let x = ((zone1_mid - resting_hr) / (hrmax - resting_hr)).clamp(0.0_f64, 1.0_f64);
    let expected = 60.0_f64 * x * 0.64_f64 * (b_male * x).exp();
    let actual = banister_trimp_zone_midpoint(&zones, resting_hr, hrmax, Some("male"));
    assert!(
        (actual - expected).abs() < 1e-9,
        "expected {expected}, got {actual}"
    );
}

// ── ALG-STR-03: fit_strain_denominator tests ───────────────────────────────

#[test]
fn fit_strain_denominator_recovers_known_d() {
    // Generate pairs from a known D=7201 using strain = 21 * ln(TRIMP+1) / ln(D)
    let d_true = 7201.0_f64;
    let trimp_values = [50.0_f64, 100.0, 200.0, 400.0];
    let pairs: Vec<(f64, f64)> = trimp_values
        .iter()
        .map(|&t| {
            let strain = 21.0 * (t + 1.0).ln() / d_true.ln();
            (t, strain)
        })
        .collect();
    let d_fit = fit_strain_denominator(&pairs).expect("should return Some for 4 pairs");
    assert!(
        (d_fit - d_true).abs() < 1.0,
        "expected D near {d_true}, got {d_fit}"
    );
}

#[test]
fn fit_strain_denominator_returns_none_for_fewer_than_two_pairs() {
    assert!(fit_strain_denominator(&[]).is_none());
    assert!(fit_strain_denominator(&[(100.0, 10.0)]).is_none());
}

#[test]
fn fit_strain_denominator_two_pairs_sufficient() {
    let d_true = 5000.0_f64;
    let pairs: Vec<(f64, f64)> = [80.0_f64, 200.0]
        .iter()
        .map(|&t| (t, 21.0 * (t + 1.0).ln() / d_true.ln()))
        .collect();
    let d_fit = fit_strain_denominator(&pairs).expect("should return Some for 2 pairs");
    assert!(
        (d_fit - d_true).abs() < 1.0,
        "expected D near {d_true}, got {d_fit}"
    );
}

// ── ALG-STR-02/03: bull_strain_v1 tests ──────────────────────────────────

#[test]
fn bull_strain_v1_contains_banister_approximation_quality_flag() {
    let input = StrainInput {
        start_time: "2024-01-01T06:00:00Z".to_string(),
        end_time: "2024-01-01T07:00:00Z".to_string(),
        duration_minutes: 60.0,
        resting_hr_bpm: 55.0,
        average_hr_bpm: 140.0,
        max_hr_bpm: 175.0,
        hr_zone_minutes: vec![10.0, 15.0, 20.0, 10.0, 5.0],
        input_ids: vec![],
        profile_sex: Some("male".to_string()),
        profile_age: Some(35.0),
    };
    let result = bull_strain_v1(&input);
    assert!(
        result
            .quality_flags
            .contains(&"banister_trimp_zone_midpoint_approximation".to_string()),
        "quality_flags must always contain banister_trimp_zone_midpoint_approximation, got: {:?}",
        result.quality_flags
    );
}

#[test]
fn bull_strain_v1_output_contains_both_edwards_and_banister_scores() {
    let input = StrainInput {
        start_time: "2024-01-01T06:00:00Z".to_string(),
        end_time: "2024-01-01T07:00:00Z".to_string(),
        duration_minutes: 60.0,
        resting_hr_bpm: 55.0,
        average_hr_bpm: 140.0,
        max_hr_bpm: 175.0,
        hr_zone_minutes: vec![10.0, 15.0, 20.0, 10.0, 5.0],
        input_ids: vec![],
        profile_sex: Some("female".to_string()),
        profile_age: None,
    };
    let result = bull_strain_v1(&input);
    let output = result
        .output
        .expect("should produce output for valid input");
    let component_names: Vec<&str> = output.components.iter().map(|c| c.name.as_str()).collect();
    assert!(
        component_names.contains(&"edwards_zone_load"),
        "output must contain edwards_zone_load component, got: {component_names:?}"
    );
    assert!(
        component_names.contains(&"banister_trimp"),
        "output must contain banister_trimp component, got: {component_names:?}"
    );
}

#[test]
fn bull_strain_v1_uses_resolve_effective_hrmax_and_records_hrmax_source() {
    let input = StrainInput {
        start_time: "2024-01-01T06:00:00Z".to_string(),
        end_time: "2024-01-01T07:00:00Z".to_string(),
        duration_minutes: 60.0,
        resting_hr_bpm: 55.0,
        average_hr_bpm: 140.0,
        max_hr_bpm: 175.0,
        hr_zone_minutes: vec![10.0, 15.0, 20.0, 10.0, 5.0],
        input_ids: vec![],
        profile_sex: None,
        profile_age: Some(40.0),
    };
    let result = bull_strain_v1(&input);
    let provenance = &result.provenance;
    assert!(
        provenance.get("hrmax_source").is_some(),
        "provenance must contain hrmax_source, got: {provenance:?}"
    );
    // With age=40 and no history, source should be "tanaka"
    assert_eq!(provenance["hrmax_source"].as_str().unwrap_or(""), "tanaka");
}

