use bull_core::{
    metrics::{HrvInput, SleepInput, StrainInput, StressInput, bull_hrv_v0},
    reference::{
        REFERENCE_HRV_TIME_DOMAIN_ID, REFERENCE_HRV_TIME_DOMAIN_VERSION,
        REFERENCE_SLEEP_ACTIGRAPHY_ID, REFERENCE_SLEEP_ACTIGRAPHY_VERSION,
        REFERENCE_STRAIN_EDWARDS_ID, REFERENCE_STRAIN_EDWARDS_VERSION, REFERENCE_STRESS_HRV_HR_ID,
        REFERENCE_STRESS_HRV_HR_VERSION, hrv_reference_run_record, reference_algorithm_definitions,
        reference_hrv_time_domain, reference_sleep_actigraphy_summary,
        reference_strain_edwards_load, reference_stress_hrv_hr_proxy, sleep_reference_run_record,
        strain_reference_run_record, stress_reference_run_record,
    },
    store::BullStore,
};

#[test]
fn reference_hrv_time_domain_matches_hand_derived_values() {
    let result = reference_hrv_time_domain(&HrvInput {
        start_time: "2026-05-27T00:00:00Z".to_string(),
        end_time: "2026-05-27T00:01:00Z".to_string(),
        rr_intervals_ms: vec![800.0, 810.0, 790.0, 800.0],
        input_ids: vec!["hand-derived".to_string()],
    });

    let output = result.output.unwrap();
    assert_eq!(result.algorithm_id, REFERENCE_HRV_TIME_DOMAIN_ID);
    assert_eq!(result.algorithm_version, REFERENCE_HRV_TIME_DOMAIN_VERSION);
    assert_eq!(output.interval_count, 4);
    assert_eq!(output.valid_interval_count, 4);
    assert_close(output.mean_nn_ms, 800.0);
    assert_close(output.rmssd_ms, 200.0_f64.sqrt());
    assert_close(output.sdnn_sample_ms, (200.0_f64 / 3.0).sqrt());
    assert_close(output.pnn50_fraction, 0.0);
}

#[test]
fn reference_hrv_time_domain_flags_invalid_intervals() {
    let result = reference_hrv_time_domain(&HrvInput {
        start_time: "2026-05-27T00:00:00Z".to_string(),
        end_time: "2026-05-27T00:01:00Z".to_string(),
        rr_intervals_ms: vec![800.0, f64::NAN, 810.0, 2501.0, 790.0],
        input_ids: Vec::new(),
    });

    let output = result.output.unwrap();
    assert_eq!(output.valid_interval_count, 3);
    assert_eq!(output.invalid_interval_count, 2);
    assert!(
        result
            .quality_flags
            .contains(&"invalid_rr_interval_dropped".to_string())
    );
}

#[test]
fn reference_hrv_time_domain_reports_insufficient_data_without_output() {
    let result = reference_hrv_time_domain(&HrvInput {
        start_time: "2026-05-27T00:00:00Z".to_string(),
        end_time: "2026-05-27T00:01:00Z".to_string(),
        rr_intervals_ms: vec![100.0, 800.0],
        input_ids: Vec::new(),
    });

    assert!(result.output.is_none());
    assert!(
        result
            .errors
            .contains(&"not_enough_valid_rr_intervals".to_string())
    );
}

#[test]
fn bull_hrv_v0_matches_internal_reference_for_shared_policy() {
    let input = HrvInput {
        start_time: "2026-05-27T00:00:00Z".to_string(),
        end_time: "2026-05-27T00:01:00Z".to_string(),
        rr_intervals_ms: vec![800.0, 810.0, 790.0, 800.0],
        input_ids: Vec::new(),
    };

    let bull = bull_hrv_v0(&input).output.unwrap();
    let reference = reference_hrv_time_domain(&input).output.unwrap();

    assert_close(bull.mean_nn_ms, reference.mean_nn_ms);
    assert_close(bull.rmssd_ms, reference.rmssd_ms);
    assert_close(bull.sdnn_ms, reference.sdnn_sample_ms);
    assert_close(bull.pnn50_fraction, reference.pnn50_fraction);
}

#[test]
fn reference_sleep_actigraphy_summary_matches_hand_derived_values() {
    let result = reference_sleep_actigraphy_summary(&SleepInput {
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
    assert_eq!(result.algorithm_id, REFERENCE_SLEEP_ACTIGRAPHY_ID);
    assert_eq!(result.algorithm_version, REFERENCE_SLEEP_ACTIGRAPHY_VERSION);
    assert_close(output.time_in_bed_minutes, 480.0);
    assert_close(output.sleep_minutes, 420.0);
    assert_close(output.wake_minutes, 60.0);
    assert_close(output.sleep_efficiency_fraction, 0.875);
    assert_close(output.wake_after_sleep_onset_minutes, 60.0);
    assert_eq!(output.disturbance_count, 4);
    assert_close(output.fragmentation_index_per_hour, 4.0 / 7.0);
}

#[test]
fn reference_sleep_actigraphy_summary_rejects_invalid_window() {
    let result = reference_sleep_actigraphy_summary(&SleepInput {
        start_time: "2026-05-27T22:30:00Z".to_string(),
        end_time: "2026-05-28T06:30:00Z".to_string(),
        sleep_duration_minutes: 500.0,
        sleep_need_minutes: 480.0,
        time_in_bed_minutes: 480.0,
        midpoint_deviation_minutes: 30.0,
        disturbance_count: 4,
        input_ids: Vec::new(),
        ..Default::default()
    });

    assert!(result.output.is_none());
    assert!(
        result
            .errors
            .contains(&"sleep_duration_must_not_exceed_time_in_bed".to_string())
    );
}

#[test]
fn reference_strain_edwards_load_matches_hand_derived_values() {
    let result = reference_strain_edwards_load(&StrainInput {
        start_time: "2026-05-28T12:00:00Z".to_string(),
        end_time: "2026-05-28T13:00:00Z".to_string(),
        duration_minutes: 60.0,
        resting_hr_bpm: 60.0,
        average_hr_bpm: 120.0,
        max_hr_bpm: 180.0,
        hr_zone_minutes: vec![10.0, 20.0, 30.0, 0.0, 0.0],
        input_ids: vec!["hand-derived.strain".to_string()],
    });

    let output = result.output.unwrap();
    assert_eq!(result.algorithm_id, REFERENCE_STRAIN_EDWARDS_ID);
    assert_eq!(result.algorithm_version, REFERENCE_STRAIN_EDWARDS_VERSION);
    assert_close(output.duration_minutes, 60.0);
    assert_close(output.edwards_load, 140.0);
    assert_close(output.edwards_load_per_hour, 140.0);
    assert_eq!(output.components.len(), 5);
    assert_close(output.components[0].value, 10.0);
    assert_close(output.components[1].value, 40.0);
    assert_close(output.components[2].value, 90.0);
}

#[test]
fn reference_strain_edwards_load_rejects_bad_zone_shape() {
    let result = reference_strain_edwards_load(&StrainInput {
        start_time: "2026-05-28T12:00:00Z".to_string(),
        end_time: "2026-05-28T13:00:00Z".to_string(),
        duration_minutes: 60.0,
        resting_hr_bpm: 60.0,
        average_hr_bpm: 120.0,
        max_hr_bpm: 180.0,
        hr_zone_minutes: vec![10.0, 20.0],
        input_ids: Vec::new(),
    });

    assert!(result.output.is_none());
    assert!(
        result
            .errors
            .contains(&"five_hr_zones_required".to_string())
    );
}

#[test]
fn reference_stress_hrv_hr_proxy_matches_hand_derived_values() {
    let result = reference_stress_hrv_hr_proxy(&StressInput {
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
    assert_eq!(result.algorithm_id, REFERENCE_STRESS_HRV_HR_ID);
    assert_eq!(result.algorithm_version, REFERENCE_STRESS_HRV_HR_VERSION);
    assert_close(output.heart_rate_elevation_score, 50.0);
    assert_close(output.hrv_suppression_score, 50.0);
    assert_close(output.unadjusted_stress_score_0_to_100, 50.0);
    assert_eq!(output.components.len(), 2);
}

#[test]
fn reference_stress_hrv_hr_proxy_rejects_invalid_hrv_baseline() {
    let result = reference_stress_hrv_hr_proxy(&StressInput {
        start_time: "2026-05-28T12:00:00Z".to_string(),
        end_time: "2026-05-28T12:05:00Z".to_string(),
        heart_rate_bpm: 90.0,
        resting_hr_bpm: 60.0,
        hrv_rmssd_ms: 25.0,
        hrv_baseline_rmssd_ms: 0.0,
        motion_intensity_0_to_1: 0.0,
        input_ids: Vec::new(),
    });

    assert!(result.output.is_none());
    assert!(
        result
            .errors
            .contains(&"hrv_baseline_rmssd_ms_must_be_finite_positive".to_string())
    );
}

#[test]
fn reference_definition_and_run_persist_to_sqlite() {
    let store = BullStore::open_in_memory().unwrap();
    for definition in reference_algorithm_definitions() {
        store.upsert_algorithm_definition(&definition).unwrap();
    }

    let saved = store
        .algorithm_definition(
            REFERENCE_HRV_TIME_DOMAIN_ID,
            REFERENCE_HRV_TIME_DOMAIN_VERSION,
        )
        .unwrap()
        .unwrap();
    assert_eq!(saved.metric_family, "hrv");
    assert_eq!(saved.status, "benchmark-only");

    let hrv_result = reference_hrv_time_domain(&HrvInput {
        start_time: "2026-05-27T00:00:00Z".to_string(),
        end_time: "2026-05-27T00:01:00Z".to_string(),
        rr_intervals_ms: vec![800.0, 810.0, 790.0, 800.0],
        input_ids: vec!["fixture.synthetic".to_string()],
    });
    let hrv_record = hrv_reference_run_record("reference-hrv-run-1", &hrv_result).unwrap();
    assert!(store.insert_algorithm_run(&hrv_record).unwrap());

    let sleep_result = reference_sleep_actigraphy_summary(&SleepInput {
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
    let sleep_record = sleep_reference_run_record("reference-sleep-run-1", &sleep_result).unwrap();
    assert!(store.insert_algorithm_run(&sleep_record).unwrap());

    let strain_result = reference_strain_edwards_load(&StrainInput {
        start_time: "2026-05-28T12:00:00Z".to_string(),
        end_time: "2026-05-28T13:00:00Z".to_string(),
        duration_minutes: 60.0,
        resting_hr_bpm: 60.0,
        average_hr_bpm: 120.0,
        max_hr_bpm: 180.0,
        hr_zone_minutes: vec![10.0, 20.0, 30.0, 0.0, 0.0],
        input_ids: vec!["fixture.synthetic.strain".to_string()],
    });
    let strain_record =
        strain_reference_run_record("reference-strain-run-1", &strain_result).unwrap();
    assert!(store.insert_algorithm_run(&strain_record).unwrap());

    let stress_result = reference_stress_hrv_hr_proxy(&StressInput {
        start_time: "2026-05-28T12:00:00Z".to_string(),
        end_time: "2026-05-28T12:05:00Z".to_string(),
        heart_rate_bpm: 90.0,
        resting_hr_bpm: 60.0,
        hrv_rmssd_ms: 25.0,
        hrv_baseline_rmssd_ms: 50.0,
        motion_intensity_0_to_1: 0.0,
        input_ids: vec!["fixture.synthetic.stress".to_string()],
    });
    let stress_record =
        stress_reference_run_record("reference-stress-run-1", &stress_result).unwrap();
    assert!(store.insert_algorithm_run(&stress_record).unwrap());

    let saved_run = store.algorithm_run("reference-hrv-run-1").unwrap().unwrap();
    assert_eq!(saved_run.algorithm_id, REFERENCE_HRV_TIME_DOMAIN_ID);
    assert!(saved_run.output_json.contains("\"sdnn_sample_ms\""));
    let saved_sleep = store
        .algorithm_run("reference-sleep-run-1")
        .unwrap()
        .unwrap();
    assert_eq!(saved_sleep.algorithm_id, REFERENCE_SLEEP_ACTIGRAPHY_ID);
    assert!(
        saved_sleep
            .output_json
            .contains("\"sleep_efficiency_fraction\"")
    );
    let saved_strain = store
        .algorithm_run("reference-strain-run-1")
        .unwrap()
        .unwrap();
    assert_eq!(saved_strain.algorithm_id, REFERENCE_STRAIN_EDWARDS_ID);
    assert!(saved_strain.output_json.contains("\"edwards_load\""));
    let saved_stress = store
        .algorithm_run("reference-stress-run-1")
        .unwrap()
        .unwrap();
    assert_eq!(saved_stress.algorithm_id, REFERENCE_STRESS_HRV_HR_ID);
    assert!(
        saved_stress
            .output_json
            .contains("\"unadjusted_stress_score_0_to_100\"")
    );
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "expected {expected}, got {actual}"
    );
}
