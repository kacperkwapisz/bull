use rusqlite::Connection;

use bull_core::{
    storage_check::{StorageCheckOptions, check_storage_database},
    store::known_tables,
};

#[test]
fn storage_check_passes_fresh_database_with_self_test() {
    let tempdir = tempfile::tempdir().unwrap();
    let db = tempdir.path().join("bull.sqlite");

    let report = check_storage_database(StorageCheckOptions {
        database_path: &db,
        run_self_test: true,
    })
    .unwrap();

    assert!(report.pass, "{:?}", report.issues);
    assert!(report.schema_version_valid);
    assert!(report.foreign_keys_valid);
    assert!(report.integrity_valid);
    assert!(report.tables_present);
    assert!(report.required_columns_present);
    assert!(report.row_counts_ready);
    assert!(report.self_test_ready);
    assert!(report.storage_ready);
    assert!(report.next_actions.is_empty());
    assert_eq!(report.actual_schema_version, report.expected_schema_version);
    assert!(report.foreign_keys_enabled);
    assert_eq!(report.integrity_check, "ok");
    assert_eq!(report.tables.len(), known_tables().len());
    assert!(report.tables.iter().all(|table| table.exists));
    assert!(
        report
            .tables
            .iter()
            .all(|table| table.missing_columns.is_empty())
    );

    for table_name in [
        "activity_sessions",
        "activity_metrics",
        "daily_activity_metrics",
        "hourly_activity_metrics",
        "daily_recovery_metrics",
        "metric_provenance",
        "metric_debug_features",
        "step_counter_samples",
        "activity_intervals",
        "activity_labels",
        "external_sleep_sessions",
        "external_sleep_stages",
        "sleep_correction_labels",
    ] {
        let table = report
            .tables
            .iter()
            .find(|table| table.table == table_name)
            .unwrap();
        assert!(table.exists, "{table_name}");
        assert_eq!(table.row_count, Some(0), "{table_name}");
        assert!(table.missing_columns.is_empty(), "{table_name}");
    }

    let self_test = report.self_test.unwrap();
    assert!(self_test.ran);
    assert!(self_test.raw_inserted);
    assert!(self_test.raw_idempotent);
    assert!(self_test.decoded_inserted);
    assert!(self_test.query_roundtrip);
    assert!(self_test.foreign_key_rejected);
    assert!(self_test.next_actions.is_empty());
}

#[test]
fn storage_check_can_run_without_mutating_self_test_rows() {
    let tempdir = tempfile::tempdir().unwrap();
    let db = tempdir.path().join("bull.sqlite");

    let report = check_storage_database(StorageCheckOptions {
        database_path: &db,
        run_self_test: false,
    })
    .unwrap();

    assert!(report.pass, "{:?}", report.issues);
    assert!(report.self_test_ready);
    assert!(report.storage_ready);
    assert!(report.next_actions.is_empty());
    assert!(report.self_test.is_none());
    let raw_table = report
        .tables
        .iter()
        .find(|table| table.table == "raw_evidence")
        .unwrap();
    assert_eq!(raw_table.row_count, Some(0));
}

#[test]
fn storage_check_reports_missing_required_columns_after_partial_migration() {
    let tempdir = tempfile::tempdir().unwrap();
    let db = tempdir.path().join("bull.sqlite");
    let conn = Connection::open(&db).unwrap();
    conn.execute(
        "CREATE TABLE raw_evidence (evidence_id TEXT PRIMARY KEY)",
        [],
    )
    .unwrap();
    drop(conn);

    let report = check_storage_database(StorageCheckOptions {
        database_path: &db,
        run_self_test: false,
    })
    .unwrap();

    assert!(!report.pass);
    assert!(report.schema_version_valid);
    assert!(report.foreign_keys_valid);
    assert!(report.integrity_valid);
    assert!(report.tables_present);
    assert!(!report.required_columns_present);
    assert!(report.row_counts_ready);
    assert!(report.self_test_ready);
    assert!(!report.storage_ready);
    let raw_table = report
        .tables
        .iter()
        .find(|table| table.table == "raw_evidence")
        .unwrap();
    assert!(raw_table.exists);
    assert!(
        raw_table
            .missing_columns
            .iter()
            .any(|column| column == "payload_hex")
    );
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.contains("raw_evidence: missing column payload_hex"))
    );
    assert!(
        raw_table.next_actions.iter().any(|action| {
            action.scope == "raw_evidence.payload_hex" && action.reason == "missing_column"
        }),
        "{:?}",
        raw_table.next_actions
    );
    assert!(
        report.next_actions.iter().any(|action| {
            action.scope == "raw_evidence.payload_hex" && action.reason == "missing_column"
        }),
        "{:?}",
        report.next_actions
    );
}
