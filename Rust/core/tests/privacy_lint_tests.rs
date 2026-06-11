use std::{fs, fs::File, io::Write, path::Path};

use bull_core::{
    capture_import::{CaptureImportOptions, import_fixture_index},
    export::{RawExportFilters, RawExportOptions, export_raw_timeframe},
    fixtures::build_fixture_index,
    privacy_lint::lint_privacy_path,
    store::{
        ActivityIntervalInput, ActivityLabelInput, ActivityMetricInput, ActivitySessionInput,
        BullStore,
    },
};
use zip::{CompressionMethod, ZipWriter, write::FileOptions};

#[test]
fn clean_bullbundle_export_passes_privacy_lint() {
    let tempdir = tempfile::tempdir().unwrap();
    let db_path = tempdir.path().join("bull.sqlite");
    let export_dir = tempdir.path().join("export.bullbundle");
    let store = BullStore::open(&db_path).unwrap();
    let fixture_root = Path::new("fixtures");
    let index = build_fixture_index(fixture_root).unwrap();
    let import_report = import_fixture_index(
        &store,
        &index,
        CaptureImportOptions {
            fixture_root,
            database_path: &db_path,
            parser_version: "bull-core/test",
        },
    );
    assert!(import_report.pass, "{:?}", import_report.issues);

    let export_report = export_raw_timeframe(
        &store,
        RawExportOptions {
            output_dir: &export_dir,
            start: "2026-05-01T00:00:00Z",
            end: "2026-05-28T00:00:00Z",
            app_version: "bull-app/test",
            core_version: "bull-core/test",
            data_families: Vec::new(),
            filters: Default::default(),
            sqlite_source_path: Some(&db_path),
            zip_output_path: None,
        },
    )
    .unwrap();
    assert!(export_report.pass, "{:?}", export_report.issues);

    let lint = lint_privacy_path(&export_dir).unwrap();

    assert!(lint.pass, "{:?}", lint.issues);
    assert!(lint.input_valid);
    assert!(lint.files_readable);
    assert!(lint.scan_coverage_ready);
    assert!(lint.auth_tokens_clear);
    assert!(lint.debug_tokens_clear);
    assert!(lint.private_api_clear);
    assert!(lint.direct_identifiers_clear);
    assert!(lint.privacy_ready);
    assert!(
        lint.files
            .iter()
            .any(|file| file.path == "data/decoded_frames.jsonl" && file.scanned)
    );
    assert!(
        lint.files
            .iter()
            .any(|file| file.path == "data/bull.sqlite" && file.skipped)
    );
}

#[test]
fn clean_zipped_bullbundle_export_passes_privacy_lint() {
    let tempdir = tempfile::tempdir().unwrap();
    let db_path = tempdir.path().join("bull.sqlite");
    let export_dir = tempdir.path().join("export.bullbundle");
    let zip_path = tempdir.path().join("export.bullbundle.zip");
    let store = BullStore::open(&db_path).unwrap();
    let fixture_root = Path::new("fixtures");
    let index = build_fixture_index(fixture_root).unwrap();
    let import_report = import_fixture_index(
        &store,
        &index,
        CaptureImportOptions {
            fixture_root,
            database_path: &db_path,
            parser_version: "bull-core/test",
        },
    );
    assert!(import_report.pass, "{:?}", import_report.issues);

    let export_report = export_raw_timeframe(
        &store,
        RawExportOptions {
            output_dir: &export_dir,
            start: "2026-05-01T00:00:00Z",
            end: "2026-05-28T00:00:00Z",
            app_version: "bull-app/test",
            core_version: "bull-core/test",
            data_families: Vec::new(),
            filters: Default::default(),
            sqlite_source_path: Some(&db_path),
            zip_output_path: Some(&zip_path),
        },
    )
    .unwrap();
    assert!(export_report.pass, "{:?}", export_report.issues);

    let lint = lint_privacy_path(&zip_path).unwrap();

    assert!(lint.pass, "{:?}", lint.issues);
    assert!(lint.input_valid);
    assert!(lint.files_readable);
    assert!(lint.scan_coverage_ready);
    assert!(lint.auth_tokens_clear);
    assert!(lint.debug_tokens_clear);
    assert!(lint.private_api_clear);
    assert!(lint.direct_identifiers_clear);
    assert!(lint.privacy_ready);
    assert!(
        lint.files
            .iter()
            .any(|file| file.path.ends_with("!data/decoded_frames.jsonl") && file.scanned)
    );
}

#[test]
fn activity_export_bundle_passes_privacy_lint_without_leaking_identifiers() {
    let tempdir = tempfile::tempdir().unwrap();
    let db_path = tempdir.path().join("bull.sqlite");
    let export_dir = tempdir.path().join("activity.bullbundle");
    let store = BullStore::open(&db_path).unwrap();

    let session_provenance = serde_json::json!({
        "source": "synthetic.activity.export",
        "session_kind": "run_like",
        "status": "pre_device",
    })
    .to_string();
    let metric_quality_flags = serde_json::json!(["steady", "trusted"]).to_string();
    let metric_provenance = serde_json::json!({
        "source": "synthetic.activity.export",
        "metric_kind": "derived",
        "status": "pre_device",
    })
    .to_string();
    let interval_metadata = serde_json::json!({
        "source": "synthetic.activity.export",
        "interval_kind": "lap",
        "status": "pre_device",
    })
    .to_string();
    let interval_provenance = serde_json::json!({
        "source": "synthetic.activity.export",
        "interval_kind": "lap",
        "status": "pre_device",
    })
    .to_string();
    let label_provenance = serde_json::json!({
        "source": "synthetic.activity.export",
        "label_kind": "synthetic",
        "status": "pre_device",
    })
    .to_string();

    assert!(
        store
            .insert_activity_session(ActivitySessionInput {
                session_id: "activity-session-1",
                source: "official_app",
                start_time_unix_ms: 1779840000000,
                end_time_unix_ms: 1779843600000,
                activity_type: "running",
                external_activity_type_code: Some("RUN-42"),
                external_activity_type_name: Some("Morning Run"),
                custom_label: Some("morning run"),
                confidence: 0.84,
                detection_method: "official_capture",
                sync_status: "synced",
                provenance_json: &session_provenance,
            })
            .unwrap()
    );
    assert!(
        store
            .insert_activity_metric(ActivityMetricInput {
                metric_id: "activity-metric-1",
                activity_session_id: "activity-session-1",
                metric_name: "heart_rate",
                value: 152.5,
                unit: "bpm",
                start_time_unix_ms: 1779840060000,
                end_time_unix_ms: 1779840120000,
                quality_flags_json: &metric_quality_flags,
                provenance_json: &metric_provenance,
            })
            .unwrap()
    );
    assert!(
        store
            .insert_activity_interval(ActivityIntervalInput {
                interval_id: "activity-interval-1",
                activity_session_id: "activity-session-1",
                interval_type: "work",
                start_time_unix_ms: 1779840180000,
                end_time_unix_ms: 1779840240000,
                sequence: 1,
                metadata_json: &interval_metadata,
                provenance_json: &interval_provenance,
            })
            .unwrap()
    );
    assert!(
        store
            .insert_activity_label(ActivityLabelInput {
                label_id: "activity-label-1",
                activity_session_id: "activity-session-1",
                label_type: "user",
                value: "easy run",
                source: "manual",
                confidence: Some(0.93),
                provenance_json: &label_provenance,
            })
            .unwrap()
    );

    let export_report = export_raw_timeframe(
        &store,
        RawExportOptions {
            output_dir: &export_dir,
            start: "2026-05-27T00:00:00Z",
            end: "2026-05-28T00:00:00Z",
            app_version: "bull-app/test",
            core_version: "bull-core/test",
            data_families: vec![
                "activity_sessions".to_string(),
                "activity_metrics".to_string(),
                "activity_intervals".to_string(),
                "activity_labels".to_string(),
            ],
            filters: RawExportFilters {
                include_raw_bytes: false,
                ..Default::default()
            },
            sqlite_source_path: None,
            zip_output_path: None,
        },
    )
    .unwrap();

    assert!(export_report.pass, "{:?}", export_report.issues);
    assert_eq!(
        export_report.manifest.data_families,
        vec![
            "activity_sessions".to_string(),
            "activity_metrics".to_string(),
            "activity_intervals".to_string(),
            "activity_labels".to_string(),
        ]
    );
    assert_eq!(export_report.activity_session_rows, 1);
    assert_eq!(export_report.activity_metric_rows, 1);
    assert_eq!(export_report.activity_interval_rows, 1);
    assert_eq!(export_report.activity_label_rows, 1);

    let activity_sessions = read_jsonl_values(&export_dir.join("data/activity_sessions.jsonl"));
    let activity_metrics = read_jsonl_values(&export_dir.join("data/activity_metrics.jsonl"));
    let activity_intervals = read_jsonl_values(&export_dir.join("data/activity_intervals.jsonl"));
    let activity_labels = read_jsonl_values(&export_dir.join("data/activity_labels.jsonl"));

    assert_eq!(activity_sessions.len(), 1);
    assert_eq!(activity_metrics.len(), 1);
    assert_eq!(activity_intervals.len(), 1);
    assert_eq!(activity_labels.len(), 1);

    assert_eq!(activity_sessions[0]["session_id"], "activity-session-1");
    assert_eq!(activity_sessions[0]["source"], "official_app");
    assert_eq!(activity_sessions[0]["activity_type"], "running");
    assert_eq!(
        activity_sessions[0]["external_activity_type_code"],
        "RUN-42"
    );
    assert_eq!(
        activity_sessions[0]["external_activity_type_name"],
        "Morning Run"
    );
    assert_eq!(activity_sessions[0]["custom_label"], "morning run");
    assert_eq!(activity_sessions[0]["detection_method"], "official_capture");
    assert_eq!(activity_sessions[0]["sync_status"], "synced");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(
            activity_sessions[0]["provenance_json"].as_str().unwrap()
        )
        .unwrap(),
        serde_json::json!({
            "source": "synthetic.activity.export",
            "session_kind": "run_like",
            "status": "pre_device",
        })
    );

    assert_eq!(activity_metrics[0]["metric_id"], "activity-metric-1");
    assert_eq!(
        activity_metrics[0]["activity_session_id"],
        "activity-session-1"
    );
    assert_eq!(activity_metrics[0]["metric_name"], "heart_rate");
    assert_eq!(activity_metrics[0]["unit"], "bpm");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(
            activity_metrics[0]["quality_flags_json"].as_str().unwrap()
        )
        .unwrap(),
        serde_json::json!(["steady", "trusted"])
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(
            activity_metrics[0]["provenance_json"].as_str().unwrap()
        )
        .unwrap(),
        serde_json::json!({
            "source": "synthetic.activity.export",
            "metric_kind": "derived",
            "status": "pre_device",
        })
    );

    assert_eq!(activity_intervals[0]["interval_id"], "activity-interval-1");
    assert_eq!(
        activity_intervals[0]["activity_session_id"],
        "activity-session-1"
    );
    assert_eq!(activity_intervals[0]["interval_type"], "work");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(
            activity_intervals[0]["metadata_json"].as_str().unwrap()
        )
        .unwrap(),
        serde_json::json!({
            "source": "synthetic.activity.export",
            "interval_kind": "lap",
            "status": "pre_device",
        })
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(
            activity_intervals[0]["provenance_json"].as_str().unwrap()
        )
        .unwrap(),
        serde_json::json!({
            "source": "synthetic.activity.export",
            "interval_kind": "lap",
            "status": "pre_device",
        })
    );

    assert_eq!(activity_labels[0]["label_id"], "activity-label-1");
    assert_eq!(
        activity_labels[0]["activity_session_id"],
        "activity-session-1"
    );
    assert_eq!(activity_labels[0]["label_type"], "user");
    assert_eq!(activity_labels[0]["value"], "easy run");
    assert_eq!(activity_labels[0]["source"], "manual");
    assert_eq!(activity_labels[0]["confidence"], 0.93);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(
            activity_labels[0]["provenance_json"].as_str().unwrap()
        )
        .unwrap(),
        serde_json::json!({
            "source": "synthetic.activity.export",
            "label_kind": "synthetic",
            "status": "pre_device",
        })
    );

    let lint = lint_privacy_path(&export_dir).unwrap();

    assert!(lint.pass, "{:?}", lint.issues);
    assert!(lint.input_valid);
    assert!(lint.files_readable);
    assert!(lint.scan_coverage_ready);
    assert!(lint.auth_tokens_clear);
    assert!(lint.debug_tokens_clear);
    assert!(lint.private_api_clear);
    assert!(lint.direct_identifiers_clear);
    assert!(lint.privacy_ready);
    assert_eq!(
        lint.files
            .iter()
            .filter(|file| file.path.starts_with("data/activity_"))
            .count(),
        8
    );
    for path in [
        "data/activity_sessions.jsonl",
        "data/activity_sessions.csv",
        "data/activity_metrics.jsonl",
        "data/activity_metrics.csv",
        "data/activity_intervals.jsonl",
        "data/activity_intervals.csv",
        "data/activity_labels.jsonl",
        "data/activity_labels.csv",
    ] {
        let file = lint
            .files
            .iter()
            .find(|file| file.path == path)
            .unwrap_or_else(|| panic!("missing lint report for {path}: {:?}", lint.files));
        assert!(file.scanned, "{path} was not scanned: {:?}", lint.files);
        assert!(
            file.findings.is_empty(),
            "{path} leaked findings: {:?}",
            file.findings
        );
    }
}

#[test]
fn privacy_lint_rejects_tokens_private_api_material_and_identifiers() {
    let tempdir = tempfile::tempdir().unwrap();
    let path = tempdir.path().join("leaky.log");
    fs::write(
        &path,
        "Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ1c2VyIn0.signaturevaluewithenoughpadding\n\
         POST metrics-service/v1/metrics userId: 123 strap_serial: ABC123 token=debug-secret\n\
         email=test@example.com mac=aa:bb:cc:dd:ee:ff X-WHOOP-Strap-Id: ABC123\n",
    )
    .unwrap();

    let lint = lint_privacy_path(&path).unwrap();

    assert!(!lint.pass);
    assert!(lint.input_valid);
    assert!(lint.files_readable);
    assert!(lint.scan_coverage_ready);
    assert!(!lint.auth_tokens_clear);
    assert!(!lint.debug_tokens_clear);
    assert!(!lint.private_api_clear);
    assert!(!lint.direct_identifiers_clear);
    assert!(!lint.privacy_ready);
    assert_rule(&lint, "authorization_header");
    assert_rule(&lint, "bearer_token");
    assert_rule(&lint, "debug_query_token");
    assert_rule(&lint, "private_whoop_api_material");
    assert_rule(&lint, "direct_identifier");
    assert_rule(&lint, "email");
    assert_rule(&lint, "mac_address");
    assert_next_action(&lint, "authorization_header");
    assert_next_action(&lint, "debug_query_token");
    assert_next_action(&lint, "private_whoop_api_material");
    assert!(
        lint.next_actions
            .iter()
            .any(|action| action.action.contains("Bull-owned pseudonyms")),
        "missing identifier remediation action: {:?}",
        lint.next_actions
    );
}

#[test]
fn privacy_lint_scans_zip_entries() {
    let tempdir = tempfile::tempdir().unwrap();
    let zip_path = tempdir.path().join("bad.bullbundle.zip");
    let file = File::create(&zip_path).unwrap();
    let mut zip = ZipWriter::new(file);
    let options = FileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file("data/debug_events.jsonl", options).unwrap();
    zip.write_all(br#"{"data_json":"ws://127.0.0.1/bull-debug/stream?token=secret"}"#)
        .unwrap();
    zip.finish().unwrap();

    let lint = lint_privacy_path(&zip_path).unwrap();

    assert!(!lint.pass);
    assert!(lint.input_valid);
    assert!(lint.files_readable);
    assert!(lint.scan_coverage_ready);
    assert!(lint.auth_tokens_clear);
    assert!(!lint.debug_tokens_clear);
    assert!(lint.private_api_clear);
    assert!(lint.direct_identifiers_clear);
    assert!(!lint.privacy_ready);
    assert!(
        lint.issues
            .iter()
            .any(|issue| issue.contains("data/debug_events.jsonl"))
    );
    assert_rule(&lint, "debug_query_token");
    assert_next_action(&lint, "debug_query_token");
}

fn assert_rule(report: &bull_core::privacy_lint::PrivacyLintReport, rule: &str) {
    assert!(
        report
            .files
            .iter()
            .flat_map(|file| file.findings.iter())
            .any(|finding| finding.rule == rule),
        "missing privacy finding rule {rule}: {:?}",
        report.issues
    );
}

fn assert_next_action(report: &bull_core::privacy_lint::PrivacyLintReport, reason: &str) {
    assert!(
        report
            .next_actions
            .iter()
            .any(|action| action.reason == reason && !action.action.is_empty()),
        "missing privacy next action {reason}: {:?}",
        report.next_actions
    );
}

fn read_jsonl_values(path: &Path) -> Vec<serde_json::Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
