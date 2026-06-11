use bull_core::{
    energy_rollup::{
        EnergyDailyRollupOptions, BULL_ENERGY_UNAVAILABLE_STATUS_V0_ID,
        BULL_ENERGY_UNAVAILABLE_STATUS_V0_VERSION, harris_benedict_rmr_kcal_day,
        keytel_active_kcal_per_min, rmr_mifflin_st_jeor,
        rollup_energy_unavailable_daily_status_for_store,
    },
    store::{DailyActivityMetricInput, BullStore},
};

#[test]
fn energy_unavailable_status_writes_calorie_activity_metrics_with_provenance() {
    let store = BullStore::open_in_memory().unwrap();

    let report = rollup_energy_unavailable_daily_status_for_store(
        &store,
        "synthetic.sqlite",
        EnergyDailyRollupOptions {
            date_key: "2026-06-02",
            timezone: "Europe/London",
            start: "2026-06-02T00:00:00Z",
            end: "2026-06-03T00:00:00Z",
            min_owned_captures_per_summary: 1,
            require_trusted_evidence: true,
            profile_weight_kg: Some(80.0),
            profile_age_years: Some(30),
            profile_sex: Some("male"),
            resting_hr_bpm: Some(60.0),
            max_hr_bpm: Some(180.0),
            min_heart_rate_samples: 2,
            write_metric: true,
        },
    )
    .unwrap();

    assert!(report.pass, "{:?}", report.issues);
    assert_eq!(
        report.schema,
        "bull.energy-unavailable-daily-status-report.v1"
    );
    assert_eq!(report.energy_daily_rollup.pass, false);
    assert_eq!(report.available_energy_metric_count, 0);
    assert_eq!(report.unavailable_metric_count, 3);
    assert_eq!(report.written_metric_count, 3);
    assert_eq!(report.metric_provenance_written_count, 3);
    assert_eq!(
        report
            .statuses
            .iter()
            .map(|status| status.metric_id.as_str())
            .collect::<Vec<_>>(),
        vec!["active_kcal", "resting_kcal", "total_kcal"]
    );
    assert!(report.statuses.iter().all(|status| {
        status.source_kind == "unavailable"
            && status.promotion_status == "blocked"
            && status
                .blocker_reasons
                .contains(&"insufficient_heart_rate_samples".to_string())
    }));

    let rows = store.daily_activity_metrics_between(0, i64::MAX).unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|row| {
        row.steps.is_none()
            && row.active_kcal.is_none()
            && row.resting_kcal.is_none()
            && row.total_kcal.is_none()
            && row.source_kind == "unavailable"
            && row.confidence == 0.0
    }));

    let active = rows
        .iter()
        .find(|row| row.daily_metric_id.contains("active-kcal"))
        .unwrap();
    let provenance: serde_json::Value = serde_json::from_str(&active.provenance_json).unwrap();
    assert_eq!(
        provenance["algorithm"],
        BULL_ENERGY_UNAVAILABLE_STATUS_V0_ID
    );
    assert_eq!(
        provenance["algorithm_version"],
        BULL_ENERGY_UNAVAILABLE_STATUS_V0_VERSION
    );
    assert_eq!(provenance["source_kind"], "unavailable");
    assert_eq!(provenance["metric_id"], "active_kcal");
    assert_eq!(
        provenance["value_policy"],
        "no_calorie_value_written_until_whoop_packet_hr_motion_inputs_support_local_estimate"
    );

    let provenance_rows = store
        .metric_provenance_for_metric("daily_activity", &active.daily_metric_id)
        .unwrap();
    assert_eq!(provenance_rows.len(), 1);
    assert_eq!(provenance_rows[0].source_kind, "unavailable");
    assert_eq!(provenance_rows[0].confidence, Some(0.0));
}

#[test]
fn energy_unavailable_status_skips_calories_when_available_metric_exists() {
    let store = BullStore::open_in_memory().unwrap();
    store
        .upsert_daily_activity_metric(DailyActivityMetricInput {
            daily_metric_id: "daily-activity-energy-2026-06-02-europe-london-local-estimate-v0",
            date_key: "2026-06-02",
            timezone: "Europe/London",
            start_time_unix_ms: 1_780_355_200_000,
            end_time_unix_ms: 1_780_441_600_000,
            steps: None,
            active_kcal: Some(420.0),
            resting_kcal: Some(1700.0),
            total_kcal: Some(2120.0),
            average_cadence_spm: None,
            source_kind: "local_estimate",
            confidence: 0.74,
            inputs_json: r#"{"heart_rate_sample_count":120}"#,
            quality_flags_json: r#"["local_energy_estimate"]"#,
            provenance_json: r#"{"algorithm":"bull.energy.local_estimate.v0","source_kind":"local_estimate"}"#,
        })
        .unwrap();

    let report = rollup_energy_unavailable_daily_status_for_store(
        &store,
        "synthetic.sqlite",
        EnergyDailyRollupOptions {
            date_key: "2026-06-02",
            timezone: "Europe/London",
            start: "2026-06-02T00:00:00Z",
            end: "2026-06-03T00:00:00Z",
            min_owned_captures_per_summary: 1,
            require_trusted_evidence: true,
            profile_weight_kg: Some(80.0),
            profile_age_years: Some(30),
            profile_sex: Some("male"),
            resting_hr_bpm: Some(60.0),
            max_hr_bpm: Some(180.0),
            min_heart_rate_samples: 2,
            write_metric: true,
        },
    )
    .unwrap();

    assert!(report.pass);
    assert_eq!(report.available_energy_metric_count, 3);
    assert_eq!(report.unavailable_metric_count, 0);
    assert_eq!(report.written_metric_count, 0);
    assert!(report.statuses.is_empty());
    assert_eq!(store.table_count("daily_activity_metrics").unwrap(), 1);
}

// ── Task 1 tests: rmr_mifflin_st_jeor, keytel_active_kcal_per_min, harris_benedict_rmr_kcal_day ──

#[test]
fn rmr_mifflin_st_jeor_male_exact_coefficients() {
    // Male: 10*w + 6.25*h - 5*a + 5
    // weight=80, height=175, age=30 → 10*80 + 6.25*175 - 5*30 + 5 = 800 + 1093.75 - 150 + 5 = 1748.75
    let result = rmr_mifflin_st_jeor(80.0, 175.0, 30.0, Some("male"));
    assert_eq!(
        result, 1748.75,
        "male Mifflin-St Jeor: 10*80 + 6.25*175 - 5*30 + 5"
    );
}

#[test]
fn rmr_mifflin_st_jeor_female_exact_coefficients() {
    // Female: 10*w + 6.25*h - 5*a - 161
    // weight=60, height=165, age=25 → 10*60 + 6.25*165 - 5*25 - 161 = 600 + 1031.25 - 125 - 161 = 1345.25
    let result = rmr_mifflin_st_jeor(60.0, 165.0, 25.0, Some("female"));
    assert_eq!(
        result, 1345.25,
        "female Mifflin-St Jeor: 10*60 + 6.25*165 - 5*25 - 161"
    );
}

#[test]
fn rmr_mifflin_st_jeor_unknown_sex_uses_mean_intercept() {
    // Unknown: 10*w + 6.25*h - 5*a - 78 (mean of +5 and -161)
    // weight=70, height=170, age=35 → 10*70 + 6.25*170 - 5*35 - 78 = 700 + 1062.5 - 175 - 78 = 1509.5
    let result_none = rmr_mifflin_st_jeor(70.0, 170.0, 35.0, None);
    let result_other = rmr_mifflin_st_jeor(70.0, 170.0, 35.0, Some("other"));
    assert_eq!(
        result_none, 1509.5,
        "unknown-sex Mifflin: 10*70 + 6.25*170 - 5*35 - 78"
    );
    assert_eq!(
        result_other, 1509.5,
        "other-sex Mifflin: same intercept -78"
    );
}

#[test]
fn keytel_active_kcal_per_min_male_exact_coefficients() {
    // Male: (-55.0969 + 0.6309*hr + 0.1988*weight + 0.2017*age) / 251.04
    // hr=150, weight=80, age=30, hrmax=190
    // = (-55.0969 + 0.6309*150 + 0.1988*80 + 0.2017*30) / 251.04
    // = (-55.0969 + 94.635 + 15.904 + 6.051) / 251.04
    // = 61.4931 / 251.04
    let expected = (-55.0969_f64 + 0.6309 * 150.0 + 0.1988 * 80.0 + 0.2017 * 30.0) / 251.04;
    let result = keytel_active_kcal_per_min(150.0, 80.0, 30.0, Some("male"), 190.0);
    assert!(
        (result - expected).abs() < 1e-10,
        "male Keytel: expected {expected}, got {result}"
    );
}

#[test]
fn keytel_active_kcal_per_min_female_exact_coefficients() {
    // Female: (-20.4022 + 0.4472*hr - 0.1263*weight + 0.0740*age) / 251.04
    // hr=140, weight=60, age=25, hrmax=190
    let expected = (-20.4022_f64 + 0.4472 * 140.0 - 0.1263 * 60.0 + 0.0740 * 25.0) / 251.04;
    let result = keytel_active_kcal_per_min(140.0, 60.0, 25.0, Some("female"), 190.0);
    assert!(
        (result - expected).abs() < 1e-10,
        "female Keytel: expected {expected}, got {result}"
    );
}

#[test]
fn keytel_active_kcal_per_min_clamped_to_zero_for_low_hr() {
    // Very low HR should give negative raw value → clamped to 0.0
    // Male: (-55.0969 + 0.6309*1 + 0.1988*80 + 0.2017*30) / 251.04
    // = (-55.0969 + 0.6309 + 15.904 + 6.051) / 251.04 < 0
    let result = keytel_active_kcal_per_min(1.0, 80.0, 30.0, Some("male"), 190.0);
    assert_eq!(result, 0.0, "Keytel must be clamped >= 0 for low HR");
}

#[test]
fn keytel_active_kcal_per_min_hr_capped_at_hrmax() {
    // HR > hrmax → use hrmax instead
    let with_hrmax = keytel_active_kcal_per_min(190.0, 80.0, 30.0, Some("male"), 190.0);
    let with_above_hrmax = keytel_active_kcal_per_min(250.0, 80.0, 30.0, Some("male"), 190.0);
    assert_eq!(
        with_hrmax, with_above_hrmax,
        "HR above hrmax should be capped at hrmax"
    );
}

#[test]
fn harris_benedict_rmr_male_exact_coefficients() {
    // Male: 88.362 + 13.397*weight + 479.9*(height_cm/100) - 5.677*age
    // weight=80, height=175, age=30
    // = 88.362 + 13.397*80 + 479.9*(175/100) - 5.677*30
    // = 88.362 + 1071.76 + 839.825 - 170.31
    let expected = 88.362_f64 + 13.397 * 80.0 + 479.9 * (175.0 / 100.0) - 5.677 * 30.0;
    let result = harris_benedict_rmr_kcal_day(80.0, 175.0, 30.0, Some("male"));
    assert!(
        (result - expected).abs() < 1e-10,
        "male Harris-Benedict: expected {expected}, got {result}"
    );
}

#[test]
fn harris_benedict_rmr_female_exact_coefficients() {
    // Female: 447.593 + 9.247*weight + 309.8*(height_cm/100) - 4.330*age
    // weight=60, height=165, age=25
    let expected = 447.593_f64 + 9.247 * 60.0 + 309.8 * (165.0 / 100.0) - 4.330 * 25.0;
    let result = harris_benedict_rmr_kcal_day(60.0, 165.0, 25.0, Some("female"));
    assert!(
        (result - expected).abs() < 1e-10,
        "female Harris-Benedict: expected {expected}, got {result}"
    );
}

