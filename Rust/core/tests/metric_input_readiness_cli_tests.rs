#[test]
fn metric_input_readiness_cli_emits_machine_readable_blockers_from_database() {
    let tempdir = tempfile::tempdir().unwrap();
    let db = tempdir.path().join("bull.sqlite");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_bull-metric-input-readiness"))
        .arg("--database")
        .arg(&db)
        .arg("--start")
        .arg("2026-05-30T00:00:00Z")
        .arg("--end")
        .arg("2026-05-31T00:00:00Z")
        .arg("--min-owned-captures")
        .arg("1")
        .arg("--require-owned-captures")
        .arg("--require-scores-ready")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema"], "bull.metric-input-readiness-report.v1");
    assert_eq!(report["generated_by"], "bull-metric-input-readiness");
    assert_eq!(report["pass"], false);
    assert_eq!(report["require_scores_ready"], true);
    assert_eq!(report["capture_correlation_pass"], false);
    assert_eq!(report["family_count"], 6);
    assert_eq!(report["ready_family_count"], 0);
    assert!(
        report["next_actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action["scope"] == "capture_correlation"
                && action["reason"] == "capture_correlation_report_not_passed")
    );
    assert!(report["families"].as_array().unwrap().iter().any(|family| {
        family["metric_family"] == "recovery"
            && family["score_ready"] == false
            && family["next_actions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|action| {
                    action["scope"] == "respiratory_rate_rpm"
                        && action["action"]
                            .as_str()
                            .unwrap()
                            .contains("normal_history")
                })
    }));
}

#[test]
fn metric_input_readiness_cli_accepts_saved_capture_correlation_report() {
    let tempdir = tempfile::tempdir().unwrap();
    let correlation_path = tempdir.path().join("capture-correlation.json");
    std::fs::write(
        &correlation_path,
        serde_json::json!({
            "schema": "bull.capture-correlation-report.v1",
            "generated_by": "test",
            "fixture_root": "test",
            "pass": false,
            "min_owned_captures_per_summary": 1,
            "require_owned_captures": true,
            "observations": [],
            "summaries": [],
            "issues": ["no packet/event summaries found for capture correlation"],
            "next_capture_actions": []
        })
        .to_string(),
    )
    .unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_bull-metric-input-readiness"))
        .arg("--capture-correlation")
        .arg(&correlation_path)
        .arg("--require-scores-ready")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema"], "bull.metric-input-readiness-report.v1");
    assert_eq!(report["capture_correlation_pass"], false);
    assert!(
        report["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue == "capture_correlation_report_not_passed")
    );
}
