use bull_core::{
    energy_rollup::{
        EnergyDailyRollupOptions, BULL_ENERGY_UNAVAILABLE_STATUS_V0_ID,
        BULL_ENERGY_UNAVAILABLE_STATUS_V0_VERSION,
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
