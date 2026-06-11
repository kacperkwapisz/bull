use bull_core::{
    algorithm_compare::{
        ALGORITHM_COMPARISON_SCHEMA, compare_hrv_bull_to_reference,
        compare_sleep_bull_to_external_reference_report, compare_sleep_bull_to_reference,
        compare_sleep_v1_bull_to_external_reference_report, compare_sleep_v1_bull_to_reference,
        compare_strain_bull_to_reference, compare_stress_bull_to_reference,
    },
    metrics::{
        HrvInput, SleepInput, SleepModelStatusInput, SleepV1Input, StrainInput, StressInput,
    },
};

#[test]
fn hrv_comparison_reports_zero_deltas_for_shared_time_domain_fields() {
    let report = compare_hrv_bull_to_reference(&HrvInput {
        start_time: "2026-05-27T00:00:00Z".to_string(),
        end_time: "2026-05-27T00:01:00Z".to_string(),
        rr_intervals_ms: vec![800.0, 810.0, 790.0, 800.0],
        input_ids: vec!["hand-derived.hrv".to_string()],
    })
    .unwrap();

    assert_eq!(report.schema, ALGORITHM_COMPARISON_SCHEMA);
    assert!(report.pass, "{:?}", report.errors);
    assert!(report.reference_contract_valid);
    assert!(report.bull_output_ready);
    assert!(report.reference_output_ready);
    assert!(report.shared_fields_ready);
    assert_eq!(report.family, "hrv");
    assert_eq!(report.comparable_fields.len(), 4);
    assert!(report.non_comparable_fields.is_empty());
    for delta in report.deltas {
        assert_close(delta.absolute_delta, 0.0);
    }
}

#[test]
fn sleep_comparison_reports_shared_window_and_actigraphy_summary_fields() {
    let report = compare_sleep_bull_to_reference(&SleepInput {
        start_time: "2026-05-27T22:30:00Z".to_string(),
        end_time: "2026-05-28T06:30:00Z".to_string(),
        sleep_duration_minutes: 420.0,
        sleep_need_minutes: 480.0,
        time_in_bed_minutes: 480.0,
        midpoint_deviation_minutes: 30.0,
        disturbance_count: 4,
        input_ids: vec!["hand-derived.sleep".to_string()],
        ..Default::default()
    })
    .unwrap();

    assert!(report.pass, "{:?}", report.errors);
    assert_eq!(report.family, "sleep");
    assert_eq!(
        report.comparable_fields,
        vec![
            "time_in_bed_minutes",
            "sleep_minutes",
            "wake_minutes",
            "sleep_efficiency_fraction",
            "wake_after_sleep_onset_minutes",
            "disturbance_count",
            "fragmentation_index_per_hour"
        ]
    );
    assert_close(report.deltas[0].bull_value, 480.0);
    assert_close(report.deltas[0].reference_value, 480.0);
    assert_close(report.deltas[1].bull_value, 420.0);
    assert_close(report.deltas[1].reference_value, 420.0);
    assert_close(report.deltas[2].bull_value, 60.0);
    assert_close(report.deltas[2].reference_value, 60.0);
    assert_close(report.deltas[3].bull_value, 0.875);
    assert_close(report.deltas[3].reference_value, 0.875);
    assert_close(report.deltas[4].bull_value, 60.0);
    assert_close(report.deltas[4].reference_value, 60.0);
    assert_close(report.deltas[5].bull_value, 4.0);
    assert_close(report.deltas[5].reference_value, 4.0);
    assert_close(report.deltas[6].bull_value, 4.0 / 7.0);
    assert_close(report.deltas[6].reference_value, 4.0 / 7.0);
    for delta in &report.deltas {
        assert_close(delta.absolute_delta, 0.0);
    }
    assert!(report.non_comparable_fields.iter().any(|field| {
        field.contains("score_0_to_100 has no benchmark-only actigraphy score equivalent")
    }));
    assert_eq!(
        report.provenance["comparison_policy"],
        "shared_sleep_window_and_actigraphy_summary_fields"
    );
}

#[test]
fn sleep_v1_comparison_passes_reference_sleep_wake_summary_fields() {
    let report = compare_sleep_v1_bull_to_reference(&SleepV1Input {
        sleep: SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 420.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            midpoint_deviation_minutes: 30.0,
            disturbance_count: 4,
            wake_after_sleep_onset_minutes: 60.0,
            input_ids: vec!["hand-derived.sleep-v1".to_string()],
            ..Default::default()
        },
        model_status: SleepModelStatusInput {
            sleep_permission_granted: true,
            imported_platform_sleep_nights: 7,
            motion_coverage_fraction: Some(0.92),
            heart_rate_coverage_fraction: Some(0.80),
            ..Default::default()
        },
        data_coverage_fraction: Some(0.90),
        ..Default::default()
    })
    .unwrap();

    assert!(report.pass, "{:?}", report.errors);
    assert_eq!(report.bull_algorithm_id, "bull.sleep.v1");
    assert_eq!(
        report.provenance["comparison_policy"],
        "sleep_v1_shared_sleep_wake_summary_fields"
    );
    assert_eq!(
        report.comparable_fields,
        vec![
            "time_in_bed_minutes",
            "sleep_minutes",
            "wake_minutes",
            "sleep_efficiency_fraction",
            "wake_after_sleep_onset_minutes",
            "disturbance_count",
            "fragmentation_index_per_hour"
        ]
    );
    for delta in &report.deltas {
        assert_close(delta.absolute_delta, 0.0);
    }
}

#[test]
fn sleep_comparison_accepts_external_reference_report_output() {
    let reference_report = serde_json::json!({
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
            "fragmentation_index_per_hour": 4.0 / 7.0
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
    });
    let report = compare_sleep_bull_to_external_reference_report(
        &SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 420.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            midpoint_deviation_minutes: 30.0,
            disturbance_count: 4,
            input_ids: vec!["hand-derived.sleep".to_string()],
            ..Default::default()
        },
        &reference_report,
    )
    .unwrap();

    assert!(report.pass, "{:?}", report.errors);
    assert!(report.reference_contract_valid);
    assert!(report.bull_output_ready);
    assert!(report.reference_output_ready);
    assert!(report.shared_fields_ready);
    assert_eq!(
        report.reference_algorithm_id,
        "reference.sleep.ggir_summary.v1"
    );
    assert_eq!(
        report.provenance["comparison_policy"],
        "external_sleep_reference_shared_fields"
    );
    assert_eq!(report.deltas.len(), 7);
    for delta in &report.deltas {
        assert_close(delta.absolute_delta, 0.0);
    }
}

#[test]
fn sleep_v1_comparison_accepts_external_reference_report_output() {
    let reference_report = serde_json::json!({
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
            "fragmentation_index_per_hour": 4.0 / 7.0
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
    });
    let report = compare_sleep_v1_bull_to_external_reference_report(
        &SleepV1Input {
            sleep: SleepInput {
                start_time: "2026-05-27T22:30:00Z".to_string(),
                end_time: "2026-05-28T06:30:00Z".to_string(),
                sleep_duration_minutes: 420.0,
                sleep_need_minutes: 480.0,
                time_in_bed_minutes: 480.0,
                midpoint_deviation_minutes: 30.0,
                disturbance_count: 4,
                wake_after_sleep_onset_minutes: 60.0,
                input_ids: vec!["hand-derived.sleep-v1.external".to_string()],
                ..Default::default()
            },
            model_status: SleepModelStatusInput {
                sleep_permission_granted: true,
                imported_platform_sleep_nights: 7,
                motion_coverage_fraction: Some(0.92),
                heart_rate_coverage_fraction: Some(0.80),
                ..Default::default()
            },
            data_coverage_fraction: Some(0.90),
            ..Default::default()
        },
        &reference_report,
    )
    .unwrap();

    assert!(report.pass, "{:?}", report.errors);
    assert!(report.reference_contract_valid);
    assert_eq!(report.bull_algorithm_id, "bull.sleep.v1");
    assert_eq!(
        report.reference_algorithm_id,
        "reference.sleep.ggir_summary.v1"
    );
    assert_eq!(
        report.provenance["comparison_policy"],
        "sleep_v1_shared_sleep_wake_summary_fields"
    );
    assert_eq!(
        report.provenance["reference_report_schema"],
        "bull.reference-algo-report.v1"
    );
    assert_eq!(report.deltas.len(), 7);
    for delta in &report.deltas {
        assert_close(delta.absolute_delta, 0.0);
    }
}

#[test]
fn external_sleep_reference_requires_units_and_provenance_before_comparison_passes() {
    let reference_report = serde_json::json!({
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
    });
    let report = compare_sleep_bull_to_external_reference_report(
        &SleepInput {
            start_time: "2026-05-27T22:30:00Z".to_string(),
            end_time: "2026-05-28T06:30:00Z".to_string(),
            sleep_duration_minutes: 420.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            midpoint_deviation_minutes: 30.0,
            disturbance_count: 4,
            input_ids: vec!["hand-derived.sleep".to_string()],
            ..Default::default()
        },
        &reference_report,
    )
    .unwrap();

    assert!(!report.pass);
    assert!(!report.reference_contract_valid);
    assert!(report.bull_output_ready);
    assert!(report.reference_output_ready);
    assert!(!report.shared_fields_ready);
    assert!(
        report
            .errors
            .iter()
            .any(|error| { error == "reference_contract:missing_output_unit:time_in_bed_minutes" })
    );
    assert!(
        report
            .errors
            .contains(&"reference_contract:missing_provenance".to_string())
    );
    assert!(report.next_actions.iter().any(|action| {
        action.reason == "reference_output_unit_missing"
            && action.action.contains("validated adapter")
    }));
    assert!(report.next_actions.iter().any(|action| {
        action.reason == "reference_provenance_missing" && action.scope == "reference_contract"
    }));
}

#[test]
fn strain_comparison_reports_edwards_zone_load_delta() {
    let report = compare_strain_bull_to_reference(&StrainInput {
        start_time: "2026-05-28T12:00:00Z".to_string(),
        end_time: "2026-05-28T13:00:00Z".to_string(),
        duration_minutes: 60.0,
        resting_hr_bpm: 60.0,
        average_hr_bpm: 120.0,
        max_hr_bpm: 180.0,
        hr_zone_minutes: vec![10.0, 20.0, 30.0, 0.0, 0.0],
        input_ids: vec!["hand-derived.strain".to_string()],
    })
    .unwrap();

    assert!(report.pass, "{:?}", report.errors);
    assert_eq!(report.family, "strain");
    assert_eq!(report.comparable_fields, vec!["zone_load"]);
    assert_close(report.deltas[0].bull_value, 140.0);
    assert_close(report.deltas[0].reference_value, 140.0);
    assert_close(report.deltas[0].absolute_delta, 0.0);
    assert!(
        report
            .non_comparable_fields
            .iter()
            .any(|field| field.contains("score_0_to_21 has no Edwards-zone-load score equivalent"))
    );
}

#[test]
fn stress_comparison_reports_shared_hr_and_hrv_proxy_deltas() {
    let report = compare_stress_bull_to_reference(&StressInput {
        start_time: "2026-05-28T12:00:00Z".to_string(),
        end_time: "2026-05-28T12:05:00Z".to_string(),
        heart_rate_bpm: 90.0,
        resting_hr_bpm: 60.0,
        hrv_rmssd_ms: 25.0,
        hrv_baseline_rmssd_ms: 50.0,
        motion_intensity_0_to_1: 0.0,
        input_ids: vec!["hand-derived.stress".to_string()],
    })
    .unwrap();

    assert!(report.pass, "{:?}", report.errors);
    assert_eq!(report.family, "stress");
    assert_eq!(
        report.comparable_fields,
        vec!["heart_rate_elevation_score", "hrv_suppression_score"]
    );
    assert_close(report.deltas[0].bull_value, 50.0);
    assert_close(report.deltas[0].reference_value, 50.0);
    assert_close(report.deltas[0].absolute_delta, 0.0);
    assert_close(report.deltas[1].bull_value, 50.0);
    assert_close(report.deltas[1].reference_value, 50.0);
    assert_close(report.deltas[1].absolute_delta, 0.0);
    assert!(report.non_comparable_fields.iter().any(|field| {
        field.contains("motion adjustment while the reference proxy is unadjusted")
    }));
}

#[test]
fn comparison_fails_when_both_algorithms_lack_comparable_outputs() {
    let report = compare_hrv_bull_to_reference(&HrvInput {
        start_time: "2026-05-27T00:00:00Z".to_string(),
        end_time: "2026-05-27T00:01:00Z".to_string(),
        rr_intervals_ms: vec![100.0],
        input_ids: Vec::new(),
    })
    .unwrap();

    assert!(!report.pass);
    assert!(report.deltas.is_empty());
    assert!(
        report
            .errors
            .contains(&"bull:not_enough_valid_rr_intervals".to_string())
    );
    assert!(
        report
            .errors
            .contains(&"reference:not_enough_valid_rr_intervals".to_string())
    );
    assert!(
        report
            .quality_flags
            .contains(&"comparison_outputs_missing".to_string())
    );
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "expected {expected}, got {actual}"
    );
}
