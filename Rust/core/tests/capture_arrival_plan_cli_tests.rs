#[test]
fn capture_arrival_plan_cli_emits_machine_readable_blockers() {
    let tempdir = tempfile::tempdir().unwrap();
    let db = tempdir.path().join("bull.sqlite");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_bull-capture-arrival-plan"))
        .arg("--database")
        .arg(&db)
        .arg("--start")
        .arg("2026-05-30T00:00:00Z")
        .arg("--end")
        .arg("2026-05-31T00:00:00Z")
        .arg("--timezone")
        .arg("Europe/London")
        .arg("--min-owned-captures")
        .arg("1")
        .arg("--require-owned-captures")
        .arg("--require-scores-ready")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema"], "bull.capture-arrival-plan-report.v1");
    assert_eq!(report["generated_by"], "bull-capture-arrival-plan");
    assert_eq!(report["pass"], false);
    assert_eq!(report["min_owned_captures"], 1);
    assert_eq!(report["require_owned_captures"], true);
    assert_eq!(report["require_scores_ready"], true);
    assert!(report["action_count"].as_u64().unwrap() > 0);
    assert_eq!(report["physical_arrival_row_count"], 11);
    assert_eq!(
        report["capture_correlation"]["schema"],
        "bull.capture-correlation-report.v1"
    );
    assert_eq!(
        report["metric_input_readiness"]["schema"],
        "bull.metric-input-readiness-report.v1"
    );
    assert_eq!(
        report["recovery_sensor_discovery"]["schema"],
        "bull.recovery-sensor-discovery-report.v1"
    );
    assert_eq!(
        report["local_health_validation_review"]["schema"],
        "bull.local-health-validation-manifest-review.v1"
    );
    assert!(
        report["local_health_validation_review"]["acceptance_evidence_case_count"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(
        report["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action["source"] == "Capture Trust")
    );
    assert!(report["actions"].as_array().unwrap().iter().any(|action| {
        action["source"] == "Recovery Sensors"
            && action["reason"] == "oxygen_saturation_decoder_not_implemented"
    }));
    assert!(report["actions"].as_array().unwrap().iter().any(|action| {
        action["source"] == "Local Health Validation"
            && action["summary"]
                .as_str()
                .unwrap()
                .contains("validation label")
    }));
}
