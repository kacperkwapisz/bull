#[test]
fn rr_hr_consistency_cli_reports_insufficient_data_on_empty_database() {
    let tempdir = tempfile::tempdir().unwrap();
    let db = tempdir.path().join("bull.sqlite");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_bull-rr-hr-consistency"))
        .arg("--database")
        .arg(&db)
        .arg("--start")
        .arg("2026-05-30T00:00:00Z")
        .arg("--end")
        .arg("2026-05-31T00:00:00Z")
        .output()
        .unwrap();

    // No eligible frames -> insufficient_data -> non-zero exit (not verified).
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema"], "bull.rr-hr-consistency-report.v1");
    assert_eq!(report["generated_by"], "bull-rr-hr-consistency-verifier");
    assert_eq!(report["scale_basis_under_test"], "v24_history_rr_intervals_ms");
    assert_eq!(
        report["label_policy"],
        "device_internal_hr_only_no_official_labels"
    );
    assert_eq!(report["verdict"], "insufficient_data");
    assert_eq!(report["decoded_frame_count"], 0);
    assert_eq!(report["v24_history_frame_count"], 0);
    assert_eq!(report["candidate_frame_count"], 0);
    assert_eq!(report["eligible_frame_count"], 0);
    assert!(
        report["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b == "insufficient_eligible_v24_rr_hr_frames")
    );
}

#[test]
fn rr_hr_consistency_cli_requires_database() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_bull-rr-hr-consistency"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--database is required"), "stderr: {stderr}");
}
