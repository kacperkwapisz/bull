use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fmt::Write as _,
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use bull_core::{
    BullError,
    bridge::{BRIDGE_REQUEST_SCHEMA, BridgeError, BridgeRequest, handle_bridge_request},
    capture_import::{CaptureSqliteImportOptions, ensure_database_parent, import_capture_sqlite},
    export::ExportManifest,
    local_health_validation::{
        LocalHealthValidationManifestScaffoldOptions,
        local_health_validation_manifest_runbook_markdown, review_local_health_validation_manifest,
        scaffold_local_health_validation_manifest,
    },
    report::write_json_report,
    store::BullStore,
    tool_args::{args, flag, path_value, value},
};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

const MANIFEST_SCHEMA: &str = "bull.local-health-validation-manifest.v1";
const REPORT_SCHEMA: &str = "bull.local-health-validation-suite-report.v1";
const LABEL_POLICY: &str = "official_whoop_values_are_validation_labels_not_inputs";

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

fn run() -> bull_core::BullResult<()> {
    let args = args();
    let mut database = resolve_validation_database(
        path_value(&args, "--database")?,
        raw_export_bundle_path_value(&args)?,
    )?;
    if flag(&args, "--scaffold-manifest") || flag(&args, "--print-manifest-template") {
        let manifest_id = value(&args, "--manifest-id")?
            .unwrap_or_else(|| "local-health-capture-validation-scaffold".to_string());
        let timezone = value(&args, "--timezone")?
            .or_else(|| env::var("TZ").ok())
            .unwrap_or_else(|| "UTC".to_string());
        let date_key_override = value(&args, "--date-key")?;
        let output = path_value(&args, "--output")?;
        let markdown_output = path_value(&args, "--markdown-output")?;
        let review_output = path_value(&args, "--review-output")?;
        let raw_export_window = database
            .source
            .raw_export_manifest
            .as_ref()
            .and_then(|manifest| {
                non_empty_string(manifest.time_window_start.as_deref())
                    .zip(non_empty_string(manifest.time_window_end.as_deref()))
            });
        let scaffold = scaffold_local_health_validation_manifest(
            &LocalHealthValidationManifestScaffoldOptions {
                database_path: database.path.clone(),
                manifest_id,
                timezone,
                date_key: date_key_override,
                database_source_kind: Some(database.source.kind.clone()),
                start: raw_export_window
                    .as_ref()
                    .map(|(start, _end)| start.clone()),
                end: raw_export_window.as_ref().map(|(_start, end)| end.clone()),
                window_source: raw_export_window
                    .as_ref()
                    .map(|_| "raw_export_manifest".to_string()),
                raw_export_bundle_path: database
                    .source
                    .kind
                    .starts_with("raw_export")
                    .then(|| PathBuf::from(&database.source.input_path)),
            },
        )?;
        if let Some(path) = markdown_output.as_deref() {
            write_markdown_text(
                &local_health_validation_manifest_runbook_markdown(&scaffold),
                path,
            )?;
        }
        if let Some(path) = review_output.as_deref() {
            write_json_file(&review_local_health_validation_manifest(&scaffold), path)?;
        }
        write_json_report(&scaffold, output.as_deref())?;
        return Ok(());
    }
    let manifest_path = path_value(&args, "--manifest")?
        .ok_or_else(|| BullError::message("--manifest is required"))?;
    let output = path_value(&args, "--output")?;
    let markdown_output = path_value(&args, "--markdown-output")?;
    let review_output = path_value(&args, "--review-output")?;
    let manifest_value = read_manifest_value(&manifest_path)?;
    if let Some(path) = review_output.as_deref() {
        write_json_file(
            &review_local_health_validation_manifest(&manifest_value),
            path,
        )?;
    }
    let manifest = parse_manifest(manifest_value)?;
    let manifest_root = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let report = run_manifest(
        &database.path.display().to_string(),
        database.source.clone(),
        manifest_root,
        &manifest,
    );

    if let Some(path) = markdown_output.as_deref() {
        write_markdown_report(&report, path)?;
    }
    write_json_report(&report, output.as_deref())?;
    if report.pass {
        Ok(())
    } else {
        database.cleanup();
        std::process::exit(1);
    }
}

struct ResolvedValidationDatabase {
    path: PathBuf,
    cleanup_path: Option<PathBuf>,
    source: LocalHealthValidationDatabaseSource,
}

impl ResolvedValidationDatabase {
    fn cleanup(&mut self) {
        if let Some(path) = self.cleanup_path.take() {
            let _ = fs::remove_file(path);
        }
    }
}

impl Drop for ResolvedValidationDatabase {
    fn drop(&mut self) {
        self.cleanup();
    }
}

fn raw_export_bundle_path_value(args: &[String]) -> bull_core::BullResult<Option<PathBuf>> {
    let raw_export_bundle = path_value(args, "--raw-export-bundle")?;
    let bundle = path_value(args, "--bundle")?;
    if raw_export_bundle.is_some() && bundle.is_some() {
        return Err(BullError::message(
            "use only one of --raw-export-bundle or --bundle",
        ));
    }
    Ok(raw_export_bundle.or(bundle))
}

fn resolve_validation_database(
    database_path: Option<PathBuf>,
    raw_export_bundle_path: Option<PathBuf>,
) -> bull_core::BullResult<ResolvedValidationDatabase> {
    match (database_path, raw_export_bundle_path) {
        (Some(database_path), None) => Ok(ResolvedValidationDatabase {
            source: LocalHealthValidationDatabaseSource {
                kind: "direct_database".to_string(),
                input_path: database_path.display().to_string(),
                resolved_database_path: database_path.display().to_string(),
                archive_entry: None,
                temporary_extracted_database: false,
                raw_export_manifest: None,
                sqlite_audit: None,
                case_packet_evidence_summary: None,
                case_packet_evidence: Vec::new(),
            },
            path: database_path,
            cleanup_path: None,
        }),
        (None, Some(bundle_path)) => resolve_raw_export_bundle_database(&bundle_path),
        (Some(_), Some(_)) => Err(BullError::message(
            "use either --database or --raw-export-bundle, not both",
        )),
        (None, None) => Err(BullError::message(
            "--database or --raw-export-bundle is required",
        )),
    }
}

fn resolve_raw_export_bundle_database(
    bundle_path: &Path,
) -> bull_core::BullResult<ResolvedValidationDatabase> {
    if bundle_path.is_dir() {
        let database_path = bundle_path.join("data").join("bull.sqlite");
        if database_path.is_file() {
            let raw_export_manifest = Some(audit_raw_export_directory_manifest(
                bundle_path,
                &database_path,
            ));
            let sqlite_audit = Some(audit_validation_sqlite(
                &database_path,
                raw_export_manifest.as_ref(),
            ));
            return Ok(ResolvedValidationDatabase {
                source: LocalHealthValidationDatabaseSource {
                    kind: "raw_export_directory".to_string(),
                    input_path: bundle_path.display().to_string(),
                    resolved_database_path: database_path.display().to_string(),
                    archive_entry: None,
                    temporary_extracted_database: false,
                    raw_export_manifest,
                    sqlite_audit,
                    case_packet_evidence_summary: None,
                    case_packet_evidence: Vec::new(),
                },
                path: database_path,
                cleanup_path: None,
            });
        }
        return Err(BullError::message(format!(
            "raw export bundle {} is missing data/bull.sqlite",
            bundle_path.display()
        )));
    }
    if bundle_path.is_file() {
        let extracted = extract_raw_export_sqlite_from_zip(bundle_path)?;
        let raw_export_manifest = Some(audit_raw_export_zip_manifest(
            bundle_path,
            &extracted.archive_entry,
            &extracted.path,
        ));
        let sqlite_audit = Some(audit_validation_sqlite(
            &extracted.path,
            raw_export_manifest.as_ref(),
        ));
        return Ok(ResolvedValidationDatabase {
            source: LocalHealthValidationDatabaseSource {
                kind: "raw_export_zip".to_string(),
                input_path: bundle_path.display().to_string(),
                resolved_database_path: extracted.path.display().to_string(),
                archive_entry: Some(extracted.archive_entry),
                temporary_extracted_database: true,
                raw_export_manifest,
                sqlite_audit,
                case_packet_evidence_summary: None,
                case_packet_evidence: Vec::new(),
            },
            path: extracted.path.clone(),
            cleanup_path: Some(extracted.path),
        });
    }
    Err(BullError::message(format!(
        "raw export bundle {} does not exist",
        bundle_path.display()
    )))
}

fn audit_validation_sqlite(
    path: &Path,
    raw_export_manifest: Option<&LocalHealthValidationRawExportManifestAudit>,
) -> LocalHealthValidationSqliteAudit {
    let connection = match Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(connection) => connection,
        Err(error) => {
            return LocalHealthValidationSqliteAudit {
                ok: false,
                storage_schema_version: None,
                table_counts: BTreeMap::new(),
                raw_evidence_time_window_count: None,
                decoded_frames_time_window_count: None,
                issues: vec![format!("sqlite_open_failed:{error}")],
            };
        }
    };
    let mut issues = Vec::new();
    let storage_schema_version = match connection.query_row(
        "SELECT MAX(version) FROM bull_schema_migrations",
        [],
        |row| row.get::<_, Option<i64>>(0),
    ) {
        Ok(version) => version,
        Err(error) => {
            issues.push(format!("sqlite_schema_version_query_failed:{error}"));
            None
        }
    };
    if storage_schema_version.is_none() {
        issues.push("sqlite_schema_version_missing".to_string());
    }

    let mut table_counts = BTreeMap::new();
    for table in [
        "raw_evidence",
        "decoded_frames",
        "daily_activity_metrics",
        "hourly_activity_metrics",
        "daily_recovery_metrics",
        "metric_provenance",
    ] {
        match sqlite_table_exists(&connection, table) {
            Ok(true) => {
                match connection.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get::<_, i64>(0)
                }) {
                    Ok(count) => {
                        table_counts.insert(table.to_string(), count);
                    }
                    Err(error) => issues.push(format!("sqlite_table_count_failed:{table}:{error}")),
                }
            }
            Ok(false) => issues.push(format!("sqlite_required_table_missing:{table}")),
            Err(error) => issues.push(format!("sqlite_table_lookup_failed:{table}:{error}")),
        }
    }

    let (raw_evidence_time_window_count, decoded_frames_time_window_count) =
        sqlite_packet_time_window_counts(
            &connection,
            &table_counts,
            raw_export_manifest,
            &mut issues,
        );

    LocalHealthValidationSqliteAudit {
        ok: issues.is_empty(),
        storage_schema_version,
        table_counts,
        raw_evidence_time_window_count,
        decoded_frames_time_window_count,
        issues,
    }
}

fn sqlite_packet_time_window_counts(
    connection: &Connection,
    table_counts: &BTreeMap<String, i64>,
    raw_export_manifest: Option<&LocalHealthValidationRawExportManifestAudit>,
    issues: &mut Vec<String>,
) -> (Option<i64>, Option<i64>) {
    let Some(raw_export_manifest) = raw_export_manifest else {
        return (None, None);
    };
    let Some(start) = raw_export_manifest.time_window_start.as_deref() else {
        return (None, None);
    };
    let Some(end) = raw_export_manifest.time_window_end.as_deref() else {
        return (None, None);
    };
    if raw_export_manifest.time_window_start_unix_ms.is_none()
        || raw_export_manifest.time_window_end_unix_ms.is_none()
    {
        return (None, None);
    }

    let raw_count = table_counts
        .contains_key("raw_evidence")
        .then(|| sqlite_raw_evidence_time_window_count(connection, start, end, issues));
    let decoded_count = (table_counts.contains_key("raw_evidence")
        && table_counts.contains_key("decoded_frames"))
    .then(|| sqlite_decoded_frames_time_window_count(connection, start, end, issues));

    (raw_count.flatten(), decoded_count.flatten())
}

fn sqlite_raw_evidence_time_window_count(
    connection: &Connection,
    start: &str,
    end: &str,
    issues: &mut Vec<String>,
) -> Option<i64> {
    match query_raw_evidence_time_window_count(connection, start, end) {
        Ok(count) => Some(count),
        Err(error) => {
            issues.push(format!(
                "sqlite_time_window_count_failed:raw_evidence:{error}"
            ));
            None
        }
    }
}

fn sqlite_decoded_frames_time_window_count(
    connection: &Connection,
    start: &str,
    end: &str,
    issues: &mut Vec<String>,
) -> Option<i64> {
    match query_decoded_frames_time_window_count(connection, start, end) {
        Ok(count) => Some(count),
        Err(error) => {
            issues.push(format!(
                "sqlite_time_window_count_failed:decoded_frames:{error}"
            ));
            None
        }
    }
}

fn query_raw_evidence_time_window_count(
    connection: &Connection,
    start: &str,
    end: &str,
) -> rusqlite::Result<i64> {
    connection.query_row(
        r#"
        SELECT COUNT(*)
        FROM raw_evidence
        WHERE captured_at >= ?1 AND captured_at < ?2
        "#,
        [start, end],
        |row| row.get::<_, i64>(0),
    )
}

fn query_decoded_frames_time_window_count(
    connection: &Connection,
    start: &str,
    end: &str,
) -> rusqlite::Result<i64> {
    connection.query_row(
        r#"
        SELECT COUNT(*)
        FROM decoded_frames
        INNER JOIN raw_evidence ON raw_evidence.evidence_id = decoded_frames.evidence_id
        WHERE raw_evidence.captured_at >= ?1 AND raw_evidence.captured_at < ?2
        "#,
        [start, end],
        |row| row.get::<_, i64>(0),
    )
}

fn query_observed_capture_session_ids(
    connection: &Connection,
    start: &str,
    end: &str,
) -> rusqlite::Result<Vec<String>> {
    let mut statement = connection.prepare(
        r#"
        SELECT DISTINCT capture_session_id
        FROM raw_evidence
        WHERE captured_at >= ?1
          AND captured_at < ?2
          AND capture_session_id IS NOT NULL
          AND TRIM(capture_session_id) != ''
        ORDER BY capture_session_id
        "#,
    )?;
    let rows = statement.query_map([start, end], |row| row.get::<_, String>(0))?;
    rows.collect()
}

fn query_capture_session_raw_evidence_time_window_count(
    connection: &Connection,
    start: &str,
    end: &str,
    capture_session_ids: &[String],
) -> rusqlite::Result<i64> {
    let session_id_list = capture_session_sql_list(capture_session_ids);
    connection.query_row(
        &format!(
            r#"
            SELECT COUNT(*)
            FROM raw_evidence
            WHERE captured_at >= ?1
              AND captured_at < ?2
              AND capture_session_id IN ({session_id_list})
            "#
        ),
        [start, end],
        |row| row.get::<_, i64>(0),
    )
}

fn query_capture_session_decoded_frames_time_window_count(
    connection: &Connection,
    start: &str,
    end: &str,
    capture_session_ids: &[String],
) -> rusqlite::Result<i64> {
    let session_id_list = capture_session_sql_list(capture_session_ids);
    connection.query_row(
        &format!(
            r#"
            SELECT COUNT(*)
            FROM decoded_frames
            INNER JOIN raw_evidence ON raw_evidence.evidence_id = decoded_frames.evidence_id
            WHERE raw_evidence.captured_at >= ?1
              AND raw_evidence.captured_at < ?2
              AND raw_evidence.capture_session_id IN ({session_id_list})
            "#
        ),
        [start, end],
        |row| row.get::<_, i64>(0),
    )
}

fn query_decoded_packet_family_counts(
    connection: &Connection,
    start: &str,
    end: &str,
    capture_session_ids: Option<&[String]>,
) -> rusqlite::Result<BTreeMap<String, i64>> {
    let session_clause = capture_session_ids
        .filter(|ids| !ids.is_empty())
        .map(|ids| {
            format!(
                " AND raw_evidence.capture_session_id IN ({})",
                capture_session_sql_list(ids)
            )
        })
        .unwrap_or_default();
    let mut statement = connection.prepare(&format!(
        r#"
        SELECT decoded_frames.packet_type_name, decoded_frames.parsed_payload_json
        FROM decoded_frames
        INNER JOIN raw_evidence ON raw_evidence.evidence_id = decoded_frames.evidence_id
        WHERE raw_evidence.captured_at >= ?1
          AND raw_evidence.captured_at < ?2
          {session_clause}
        "#
    ))?;
    let rows = statement.query_map([start, end], |row| {
        Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut counts = BTreeMap::new();
    for row in rows {
        let (packet_type_name, parsed_payload_json) = row?;
        let family =
            decoded_packet_family(packet_type_name.as_deref(), parsed_payload_json.as_str());
        *counts.entry(family).or_insert(0) += 1;
    }
    Ok(counts)
}

fn query_raw_evidence_time_bounds(
    connection: &Connection,
    start: &str,
    end: &str,
    capture_session_ids: Option<&[String]>,
) -> rusqlite::Result<Option<LocalHealthValidationEvidenceTimeBounds>> {
    let session_clause = capture_session_ids
        .filter(|ids| !ids.is_empty())
        .map(|ids| {
            format!(
                " AND capture_session_id IN ({})",
                capture_session_sql_list(ids)
            )
        })
        .unwrap_or_default();
    query_time_bounds(
        connection,
        &format!(
            r#"
            SELECT MIN(captured_at), MAX(captured_at)
            FROM raw_evidence
            WHERE captured_at >= ?1
              AND captured_at < ?2
              {session_clause}
            "#
        ),
        start,
        end,
    )
}

fn query_decoded_frames_time_bounds(
    connection: &Connection,
    start: &str,
    end: &str,
    capture_session_ids: Option<&[String]>,
) -> rusqlite::Result<Option<LocalHealthValidationEvidenceTimeBounds>> {
    let session_clause = capture_session_ids
        .filter(|ids| !ids.is_empty())
        .map(|ids| {
            format!(
                " AND raw_evidence.capture_session_id IN ({})",
                capture_session_sql_list(ids)
            )
        })
        .unwrap_or_default();
    query_time_bounds(
        connection,
        &format!(
            r#"
            SELECT MIN(raw_evidence.captured_at), MAX(raw_evidence.captured_at)
            FROM decoded_frames
            INNER JOIN raw_evidence ON raw_evidence.evidence_id = decoded_frames.evidence_id
            WHERE raw_evidence.captured_at >= ?1
              AND raw_evidence.captured_at < ?2
              {session_clause}
            "#
        ),
        start,
        end,
    )
}

fn query_time_bounds(
    connection: &Connection,
    sql: &str,
    start: &str,
    end: &str,
) -> rusqlite::Result<Option<LocalHealthValidationEvidenceTimeBounds>> {
    let window_duration_ms = rfc3339_duration_ms(start, end);
    let window_start_unix_ms = parse_rfc3339_utc_unix_ms(start);
    let window_end_unix_ms = parse_rfc3339_utc_unix_ms(end);
    connection.query_row(sql, [start, end], |row| {
        let first_captured_at = row.get::<_, Option<String>>(0)?;
        let last_captured_at = row.get::<_, Option<String>>(1)?;
        Ok(first_captured_at
            .zip(last_captured_at)
            .map(|(first_captured_at, last_captured_at)| {
                evidence_time_bounds_from_first_last(
                    &first_captured_at,
                    &last_captured_at,
                    start,
                    end,
                    window_duration_ms,
                    window_start_unix_ms,
                    window_end_unix_ms,
                )
            }))
    })
}

fn evidence_time_bounds_from_timestamps<'a, I>(
    timestamps: I,
    start: &str,
    end: &str,
) -> Option<LocalHealthValidationEvidenceTimeBounds>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut first_captured_at = None;
    let mut last_captured_at = None;
    for timestamp in timestamps {
        if first_captured_at.is_none() {
            first_captured_at = Some(timestamp);
        }
        last_captured_at = Some(timestamp);
    }
    let first_captured_at = first_captured_at?;
    let last_captured_at = last_captured_at?;
    let window_duration_ms = rfc3339_duration_ms(start, end);
    let window_start_unix_ms = parse_rfc3339_utc_unix_ms(start);
    let window_end_unix_ms = parse_rfc3339_utc_unix_ms(end);
    Some(evidence_time_bounds_from_first_last(
        first_captured_at,
        last_captured_at,
        start,
        end,
        window_duration_ms,
        window_start_unix_ms,
        window_end_unix_ms,
    ))
}

fn evidence_time_bounds_from_first_last(
    first_captured_at: &str,
    last_captured_at: &str,
    _start: &str,
    _end: &str,
    window_duration_ms: Option<i64>,
    window_start_unix_ms: Option<i64>,
    window_end_unix_ms: Option<i64>,
) -> LocalHealthValidationEvidenceTimeBounds {
    let first_captured_at_unix_ms = parse_rfc3339_utc_unix_ms(first_captured_at);
    let last_captured_at_unix_ms = parse_rfc3339_utc_unix_ms(last_captured_at);
    let span_ms = rfc3339_duration_ms(first_captured_at, last_captured_at);
    let coverage_ratio = span_ms
        .zip(window_duration_ms)
        .map(|(span, duration)| span as f64 / duration as f64);
    let first_offset_from_case_start_ms = first_captured_at_unix_ms
        .zip(window_start_unix_ms)
        .and_then(|(captured_at, window_start)| {
            (captured_at >= window_start).then_some(captured_at - window_start)
        });
    let last_offset_before_case_end_ms =
        last_captured_at_unix_ms
            .zip(window_end_unix_ms)
            .and_then(|(captured_at, window_end)| {
                (window_end >= captured_at).then_some(window_end - captured_at)
            });
    LocalHealthValidationEvidenceTimeBounds {
        first_captured_at: first_captured_at.to_string(),
        last_captured_at: last_captured_at.to_string(),
        span_ms,
        coverage_ratio,
        first_offset_from_case_start_ms,
        last_offset_before_case_end_ms,
    }
}

fn rfc3339_duration_ms(start: &str, end: &str) -> Option<i64> {
    let (start, end) = parse_rfc3339_utc_unix_ms(start).zip(parse_rfc3339_utc_unix_ms(end))?;
    (end >= start).then_some(end - start)
}

fn decoded_packet_family(packet_type_name: Option<&str>, parsed_payload_json: &str) -> String {
    let parsed_payload = serde_json::from_str::<Value>(parsed_payload_json).ok();
    let packet_k = parsed_payload
        .as_ref()
        .and_then(|payload| payload.get("packet_k"))
        .and_then(Value::as_u64);
    let domain = parsed_payload
        .as_ref()
        .and_then(|payload| str_field(payload, &["domain"]));
    let body_summary_kind = parsed_payload
        .as_ref()
        .and_then(|payload| payload.get("body_summary"))
        .and_then(|body| str_field(body, &["kind"]));
    if let Some(packet_k) = packet_k {
        if let Some(domain) = domain.as_deref().filter(|value| !value.trim().is_empty()) {
            return format!("K{packet_k}/{domain}");
        }
        if let Some(kind) = body_summary_kind
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            return format!("K{packet_k}/{kind}");
        }
        return format!("K{packet_k}");
    }
    packet_type_name
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("unknown")
        .to_string()
}

fn capture_session_sql_list(capture_session_ids: &[String]) -> String {
    capture_session_ids
        .iter()
        .map(|session_id| sql_string_literal(session_id))
        .collect::<Vec<_>>()
        .join(",")
}

fn sqlite_table_exists(connection: &Connection, table_name: &str) -> bull_core::BullResult<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
            [table_name],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value != 0)
        .map_err(|error| BullError::message(format!("cannot inspect SQLite tables: {error}")))
}

fn audit_raw_export_directory_manifest(
    bundle_path: &Path,
    database_path: &Path,
) -> LocalHealthValidationRawExportManifestAudit {
    let manifest_path = bundle_path.join("manifest.json");
    let manifest_raw = match fs::read_to_string(&manifest_path) {
        Ok(raw) => raw,
        Err(error) => {
            return LocalHealthValidationRawExportManifestAudit {
                present: false,
                ok: false,
                manifest_path: manifest_path.display().to_string(),
                archive_entry: None,
                schema_version: None,
                official_labels_are_labels: None,
                time_window_start: None,
                time_window_end: None,
                time_window_start_unix_ms: None,
                time_window_end_unix_ms: None,
                data_families: Vec::new(),
                sqlite_file_declared: false,
                sqlite_kind: None,
                expected_sha256: None,
                actual_sha256: None,
                sha256_match: None,
                issues: vec![format!("manifest_json_missing_or_unreadable:{error}")],
            };
        }
    };
    let actual_sha256 = file_sha256_hex(database_path).ok();
    raw_export_manifest_audit_from_raw(
        &manifest_raw,
        manifest_path.display().to_string(),
        None,
        actual_sha256,
    )
}

fn audit_raw_export_zip_manifest(
    bundle_path: &Path,
    sqlite_archive_entry: &str,
    extracted_database_path: &Path,
) -> LocalHealthValidationRawExportManifestAudit {
    let prefix = raw_export_zip_entry_prefix(sqlite_archive_entry);
    let manifest_archive_entry = format!("{prefix}manifest.json");
    let manifest_raw = (|| -> bull_core::BullResult<String> {
        let file = File::open(bundle_path).map_err(|source| BullError::io(bundle_path, source))?;
        let mut archive = ZipArchive::new(file).map_err(|error| {
            BullError::message(format!(
                "raw export bundle {} is not a readable zip: {error}",
                bundle_path.display()
            ))
        })?;
        let mut entry = archive.by_name(&manifest_archive_entry).map_err(|error| {
            BullError::message(format!(
                "cannot open {manifest_archive_entry} from raw export zip {}: {error}",
                bundle_path.display()
            ))
        })?;
        let mut raw = String::new();
        io::Read::read_to_string(&mut entry, &mut raw).map_err(|source| {
            BullError::io(
                format!("{}:{manifest_archive_entry}", bundle_path.display()),
                source,
            )
        })?;
        Ok(raw)
    })();
    let manifest_raw = match manifest_raw {
        Ok(raw) => raw,
        Err(error) => {
            return LocalHealthValidationRawExportManifestAudit {
                present: false,
                ok: false,
                manifest_path: bundle_path.display().to_string(),
                archive_entry: Some(manifest_archive_entry),
                schema_version: None,
                official_labels_are_labels: None,
                time_window_start: None,
                time_window_end: None,
                time_window_start_unix_ms: None,
                time_window_end_unix_ms: None,
                data_families: Vec::new(),
                sqlite_file_declared: false,
                sqlite_kind: None,
                expected_sha256: None,
                actual_sha256: None,
                sha256_match: None,
                issues: vec![format!("manifest_json_missing_or_unreadable:{error}")],
            };
        }
    };
    let actual_sha256 = file_sha256_hex(extracted_database_path).ok();
    raw_export_manifest_audit_from_raw(
        &manifest_raw,
        bundle_path.display().to_string(),
        Some(manifest_archive_entry),
        actual_sha256,
    )
}

fn raw_export_manifest_audit_from_raw(
    manifest_raw: &str,
    manifest_path: String,
    archive_entry: Option<String>,
    actual_sha256: Option<String>,
) -> LocalHealthValidationRawExportManifestAudit {
    let manifest = match serde_json::from_str::<ExportManifest>(manifest_raw) {
        Ok(manifest) => manifest,
        Err(error) => {
            return LocalHealthValidationRawExportManifestAudit {
                present: true,
                ok: false,
                manifest_path,
                archive_entry,
                schema_version: None,
                official_labels_are_labels: None,
                time_window_start: None,
                time_window_end: None,
                time_window_start_unix_ms: None,
                time_window_end_unix_ms: None,
                data_families: Vec::new(),
                sqlite_file_declared: false,
                sqlite_kind: None,
                expected_sha256: None,
                actual_sha256,
                sha256_match: None,
                issues: vec![format!("manifest_json_invalid:{error}")],
            };
        }
    };
    let mut issues = Vec::new();
    if manifest.schema_version != "bull.export.v1" {
        issues.push(format!(
            "manifest_schema_version_unexpected:{}",
            manifest.schema_version
        ));
    }
    let time_window_start = Some(manifest.time_window.start.clone());
    let time_window_end = Some(manifest.time_window.end.clone());
    let time_window_start_unix_ms = parse_rfc3339_utc_unix_ms(&manifest.time_window.start);
    let time_window_end_unix_ms = parse_rfc3339_utc_unix_ms(&manifest.time_window.end);
    if time_window_start_unix_ms.is_none() {
        issues.push("raw_export_time_window_start_invalid".to_string());
    }
    if time_window_end_unix_ms.is_none() {
        issues.push("raw_export_time_window_end_invalid".to_string());
    }
    if let Some((start, end)) = time_window_start_unix_ms.zip(time_window_end_unix_ms)
        && end <= start
    {
        issues.push("raw_export_time_window_not_increasing".to_string());
    }
    if !manifest
        .data_families
        .iter()
        .any(|family| family == "sqlite")
    {
        issues.push("sqlite_family_not_declared".to_string());
    }
    if manifest
        .data_families
        .iter()
        .any(|family| family == "calibration_labels")
        && !manifest.official_labels_are_labels
    {
        issues.push("official_labels_are_labels_not_true_for_calibration_labels".to_string());
    }

    let sqlite_files = manifest
        .files
        .iter()
        .filter(|file| file.path == "data/bull.sqlite")
        .collect::<Vec<_>>();
    if sqlite_files.is_empty() {
        issues.push("sqlite_manifest_file_missing".to_string());
    }
    if sqlite_files.len() > 1 {
        issues.push("sqlite_manifest_file_duplicate".to_string());
    }
    let sqlite_file = sqlite_files.first().copied();
    let sqlite_kind = sqlite_file.and_then(|file| file.kind.clone());
    if let Some(kind) = sqlite_kind.as_deref()
        && kind != "sqlite"
    {
        issues.push(format!("sqlite_manifest_kind_unexpected:{kind}"));
    }
    let expected_sha256 = sqlite_file.map(|file| file.sha256.clone());
    let sha256_match = expected_sha256
        .as_deref()
        .zip(actual_sha256.as_deref())
        .map(|(expected, actual)| expected == actual);
    if expected_sha256
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        issues.push("sqlite_manifest_sha256_missing".to_string());
    }
    if actual_sha256.is_none() {
        issues.push("sqlite_actual_sha256_unavailable".to_string());
    }
    if sha256_match == Some(false) {
        issues.push("sqlite_manifest_sha256_mismatch".to_string());
    }

    LocalHealthValidationRawExportManifestAudit {
        present: true,
        ok: issues.is_empty(),
        manifest_path,
        archive_entry,
        schema_version: Some(manifest.schema_version),
        official_labels_are_labels: Some(manifest.official_labels_are_labels),
        time_window_start,
        time_window_end,
        time_window_start_unix_ms,
        time_window_end_unix_ms,
        data_families: manifest.data_families,
        sqlite_file_declared: sqlite_file.is_some(),
        sqlite_kind,
        expected_sha256,
        actual_sha256,
        sha256_match,
        issues,
    }
}

fn raw_export_zip_entry_prefix(sqlite_archive_entry: &str) -> String {
    sqlite_archive_entry
        .strip_suffix("data/bull.sqlite")
        .unwrap_or("")
        .to_string()
}

fn file_sha256_hex(path: &Path) -> bull_core::BullResult<String> {
    let bytes = fs::read(path).map_err(|source| BullError::io(path, source))?;
    Ok(sha256_hex(&bytes))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

struct ExtractedRawExportSqlite {
    path: PathBuf,
    archive_entry: String,
}

fn extract_raw_export_sqlite_from_zip(
    bundle_path: &Path,
) -> bull_core::BullResult<ExtractedRawExportSqlite> {
    let file = File::open(bundle_path).map_err(|source| BullError::io(bundle_path, source))?;
    let mut archive = ZipArchive::new(file).map_err(|error| {
        BullError::message(format!(
            "raw export bundle {} is not a readable zip: {error}",
            bundle_path.display()
        ))
    })?;
    let entry_name = raw_export_sqlite_zip_entry_name(&archive).ok_or_else(|| {
        BullError::message(format!(
            "raw export zip {} is missing data/bull.sqlite",
            bundle_path.display()
        ))
    })?;
    let mut entry = archive.by_name(&entry_name).map_err(|error| {
        BullError::message(format!(
            "cannot open {entry_name} from raw export zip {}: {error}",
            bundle_path.display()
        ))
    })?;
    let extracted_path = unique_validation_temp_path("raw-export-bundle");
    let mut output =
        File::create(&extracted_path).map_err(|source| BullError::io(&extracted_path, source))?;
    io::copy(&mut entry, &mut output).map_err(|source| BullError::io(&extracted_path, source))?;
    Ok(ExtractedRawExportSqlite {
        path: extracted_path,
        archive_entry: entry_name,
    })
}

fn raw_export_sqlite_zip_entry_name<R: io::Read + io::Seek>(
    archive: &ZipArchive<R>,
) -> Option<String> {
    let matches = archive
        .file_names()
        .filter_map(|name| {
            let normalized = name.replace('\\', "/");
            if normalized == "data/bull.sqlite" || normalized.ends_with("/data/bull.sqlite") {
                Some(name.to_string())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if matches.len() == 1 {
        matches.into_iter().next()
    } else {
        None
    }
}

#[derive(Debug, Clone, Deserialize)]
struct LocalHealthValidationManifest {
    schema: String,
    #[serde(default)]
    manifest_id: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default, alias = "start")]
    default_start: Option<String>,
    #[serde(default, alias = "end")]
    default_end: Option<String>,
    #[serde(default, alias = "date_key")]
    default_date_key: Option<String>,
    #[serde(default, alias = "timezone")]
    default_timezone: Option<String>,
    #[serde(default, alias = "capture_session_id")]
    default_capture_session_id: Option<String>,
    #[serde(default, alias = "capture_session_ids")]
    default_capture_session_ids: Vec<String>,
    #[serde(default, alias = "min_owned_captures")]
    default_min_owned_captures: Option<usize>,
    #[serde(default, alias = "profile_weight_kg")]
    default_profile_weight_kg: Option<f64>,
    #[serde(default, alias = "profile_age_years")]
    default_profile_age_years: Option<u32>,
    #[serde(default, alias = "profile_sex")]
    default_profile_sex: Option<String>,
    #[serde(default, alias = "resting_hr_bpm")]
    default_resting_hr_bpm: Option<f64>,
    #[serde(default, alias = "max_hr_bpm")]
    default_max_hr_bpm: Option<f64>,
    #[serde(default, alias = "label_provenance")]
    default_label_provenance: Option<Value>,
    #[serde(default)]
    capture_sqlite_imports: Vec<LocalHealthValidationCaptureSqliteImport>,
    #[serde(default)]
    cases: Vec<LocalHealthValidationCase>,
}

#[derive(Debug, Clone, Deserialize)]
struct LocalHealthValidationCaptureSqliteImport {
    id: String,
    #[serde(alias = "capture_sqlite_path")]
    path: String,
    session_id: String,
    #[serde(default)]
    device_model: Option<String>,
    #[serde(default)]
    sensitivity: Option<String>,
    #[serde(default)]
    parser_version: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct LocalHealthValidationCase {
    id: String,
    report: String,
    #[serde(default)]
    start: String,
    #[serde(default)]
    end: String,
    #[serde(default)]
    date_key: Option<String>,
    #[serde(default)]
    timezone: Option<String>,
    #[serde(default)]
    capture_kind: Option<String>,
    #[serde(default)]
    capture_session_id: Option<String>,
    #[serde(default)]
    capture_session_ids: Vec<String>,
    #[serde(default)]
    min_owned_captures: Option<usize>,
    #[serde(default)]
    require_trusted_evidence: bool,
    #[serde(default)]
    max_candidate_fields: Option<usize>,
    #[serde(default)]
    manual_step_delta: Option<i64>,
    #[serde(default)]
    official_whoop_step_delta: Option<i64>,
    #[serde(default)]
    step_delta_tolerance: Option<i64>,
    #[serde(default)]
    sample_rate_hz: Option<f64>,
    #[serde(default)]
    peak_threshold_i16: Option<f64>,
    #[serde(default)]
    min_peak_spacing_samples: Option<usize>,
    #[serde(default)]
    profile_weight_kg: Option<f64>,
    #[serde(default)]
    profile_age_years: Option<u32>,
    #[serde(default)]
    profile_sex: Option<String>,
    #[serde(default)]
    resting_hr_bpm: Option<f64>,
    #[serde(default)]
    max_hr_bpm: Option<f64>,
    #[serde(default)]
    min_heart_rate_samples: Option<usize>,
    #[serde(default)]
    min_sample_count: Option<usize>,
    #[serde(default)]
    official_whoop_active_kcal: Option<f64>,
    #[serde(default)]
    official_whoop_resting_kcal: Option<f64>,
    #[serde(default)]
    official_whoop_total_kcal: Option<f64>,
    #[serde(default)]
    energy_tolerance_kcal: Option<f64>,
    #[serde(default)]
    energy_relative_tolerance: Option<f64>,
    #[serde(default)]
    official_whoop_resting_hr_bpm: Option<f64>,
    #[serde(default)]
    rhr_tolerance_bpm: Option<f64>,
    #[serde(default)]
    official_whoop_hrv_rmssd_ms: Option<f64>,
    #[serde(default)]
    hrv_tolerance_ms: Option<f64>,
    #[serde(default)]
    official_whoop_respiratory_rate_rpm: Option<f64>,
    #[serde(default)]
    respiratory_rate_tolerance_rpm: Option<f64>,
    #[serde(default)]
    official_whoop_oxygen_saturation_percent: Option<f64>,
    #[serde(default)]
    oxygen_saturation_tolerance_percent: Option<f64>,
    #[serde(default)]
    official_whoop_skin_temperature_delta_c: Option<f64>,
    #[serde(default)]
    temperature_tolerance_c: Option<f64>,
    #[serde(default)]
    min_rr_intervals_to_compute: Option<usize>,
    #[serde(default)]
    write_metric: bool,
    #[serde(default)]
    label_provenance: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
struct LocalHealthValidationSuiteReport {
    schema: String,
    generated_by: String,
    pass: bool,
    manifest_schema: String,
    manifest_id: Option<String>,
    notes: Option<String>,
    database_path: String,
    database_source: LocalHealthValidationDatabaseSource,
    capture_sqlite_imports: Vec<LocalHealthValidationCaptureSqliteImportReport>,
    label_policy: String,
    case_count: usize,
    ok_case_count: usize,
    passing_case_count: usize,
    failing_case_count: usize,
    readiness_summary: LocalHealthValidationReadinessSummary,
    metric_records: Vec<LocalHealthValidationMetricRecord>,
    cases: Vec<LocalHealthValidationCaseReport>,
    issues: Vec<String>,
    next_actions: Vec<LocalHealthValidationNextAction>,
}

#[derive(Debug, Clone, Serialize)]
struct LocalHealthValidationDatabaseSource {
    kind: String,
    input_path: String,
    resolved_database_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    archive_entry: Option<String>,
    temporary_extracted_database: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    raw_export_manifest: Option<LocalHealthValidationRawExportManifestAudit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sqlite_audit: Option<LocalHealthValidationSqliteAudit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    case_packet_evidence_summary: Option<LocalHealthValidationCasePacketEvidenceSummary>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    case_packet_evidence: Vec<LocalHealthValidationCasePacketEvidenceAudit>,
}

#[derive(Debug, Clone, Serialize)]
struct LocalHealthValidationRawExportManifestAudit {
    present: bool,
    ok: bool,
    manifest_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    archive_entry: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    schema_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    official_labels_are_labels: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    time_window_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    time_window_end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    time_window_start_unix_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    time_window_end_unix_ms: Option<i64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    data_families: Vec<String>,
    sqlite_file_declared: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    sqlite_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actual_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha256_match: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct LocalHealthValidationSqliteAudit {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    storage_schema_version: Option<i64>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    table_counts: BTreeMap<String, i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    raw_evidence_time_window_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    decoded_frames_time_window_count: Option<i64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    issues: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
struct LocalHealthValidationCasePacketEvidenceSummary {
    case_count: usize,
    decoded_packet_evidence_case_count: usize,
    raw_only_packet_evidence_case_count: usize,
    no_packet_evidence_case_count: usize,
    outside_raw_export_time_window_case_count: usize,
    query_failed_case_count: usize,
    declared_capture_session_case_count: usize,
    declared_capture_session_with_evidence_case_count: usize,
    declared_capture_session_missing_evidence_case_count: usize,
    declared_capture_session_partial_evidence_case_count: usize,
    declared_capture_session_query_failed_case_count: usize,
    case_window_raw_evidence_count_sum: i64,
    case_window_decoded_frame_count_sum: i64,
    capture_session_raw_evidence_count_sum: i64,
    capture_session_decoded_frame_count_sum: i64,
    decoded_evidence_zero_span_case_count: usize,
    decoded_evidence_zero_coverage_case_count: usize,
    decoded_evidence_too_sparse_for_capture_acceptance_case_count: usize,
    capture_session_decoded_evidence_too_sparse_for_capture_acceptance_case_count: usize,
    packet_family_unrelated_for_capture_acceptance_case_count: usize,
    capture_session_packet_family_unrelated_for_capture_acceptance_case_count: usize,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    packet_family_counts: BTreeMap<String, i64>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    relevant_packet_family_counts: BTreeMap<String, i64>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    capture_session_packet_family_counts: BTreeMap<String, i64>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    capture_session_relevant_packet_family_counts: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Serialize)]
struct LocalHealthValidationCasePacketEvidenceAudit {
    case_id: String,
    report: String,
    start: String,
    end: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    case_window_duration_ms: Option<i64>,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    raw_evidence_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    decoded_frame_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    raw_evidence_time_bounds: Option<LocalHealthValidationEvidenceTimeBounds>,
    #[serde(skip_serializing_if = "Option::is_none")]
    decoded_frame_time_bounds: Option<LocalHealthValidationEvidenceTimeBounds>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    packet_family_counts: BTreeMap<String, i64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    capture_acceptance_required_packet_family_prefixes: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    relevant_packet_family_counts: BTreeMap<String, i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capture_session_status: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    expected_capture_session_ids: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    observed_capture_session_ids: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    missing_capture_session_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capture_session_raw_evidence_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capture_session_decoded_frame_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capture_session_raw_evidence_time_bounds: Option<LocalHealthValidationEvidenceTimeBounds>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capture_session_decoded_frame_time_bounds: Option<LocalHealthValidationEvidenceTimeBounds>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    capture_session_packet_family_counts: BTreeMap<String, i64>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    capture_session_relevant_packet_family_counts: BTreeMap<String, i64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct LocalHealthValidationEvidenceTimeBounds {
    first_captured_at: String,
    last_captured_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    span_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    coverage_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_offset_from_case_start_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_offset_before_case_end_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct LocalHealthValidationCaptureSqliteImportReport {
    id: String,
    capture_sqlite_path: String,
    session_id: String,
    ok: bool,
    import_ready: bool,
    raw_import_completed: bool,
    decode_pass: bool,
    source_frame_count: usize,
    raw_inserted: usize,
    raw_existing: usize,
    frames_inserted: usize,
    frames_existing: usize,
    parse_failed_count: usize,
    issues: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct LocalHealthValidationCaseReport {
    id: String,
    report: String,
    method: Option<String>,
    ok: bool,
    pass: bool,
    label_policy_valid: bool,
    issues: Vec<String>,
    readiness: LocalHealthValidationCaseReadiness,
    metric_records: Vec<LocalHealthValidationMetricRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<BridgeError>,
}

#[derive(Debug, Clone, Serialize)]
struct LocalHealthValidationMetricRecord {
    case_id: String,
    report: String,
    method: Option<String>,
    metric_family: String,
    metric_name: String,
    unit: String,
    source_kind: String,
    pass: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    official_label_value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    manual_label_value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_packet_count: Option<usize>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    input_counts: BTreeMap<String, usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    confidence: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    algorithm_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    algorithm_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label_policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    promotion_status: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    issues: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    quality_flags: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    blockers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct LocalHealthValidationReadinessSummary {
    case_count: usize,
    acceptance_ready_case_count: usize,
    capture_acceptance_ready_case_count: usize,
    missing_packet_evidence_case_count: usize,
    missing_or_invalid_official_label_case_count: usize,
    manual_label_missing_case_count: usize,
    unavailable_status_case_count: usize,
    capture_session_declared_case_count: usize,
    capture_session_required_case_count: usize,
    capture_session_missing_evidence_case_count: usize,
    capture_session_sparse_evidence_case_count: usize,
    capture_session_unrelated_packet_family_case_count: usize,
}

#[derive(Debug, Clone, Serialize)]
struct LocalHealthValidationCaseReadiness {
    normalized_report: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capture_kind: Option<String>,
    capture_session_status: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    expected_capture_session_ids: Vec<String>,
    capture_session_raw_evidence_count: usize,
    capture_session_decoded_frame_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    capture_session_raw_evidence_time_bounds: Option<LocalHealthValidationEvidenceTimeBounds>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capture_session_decoded_frame_time_bounds: Option<LocalHealthValidationEvidenceTimeBounds>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    capture_session_packet_family_counts: BTreeMap<String, usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    capture_acceptance_required_packet_family_prefixes: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    capture_session_relevant_packet_family_counts: BTreeMap<String, usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    missing_capture_session_ids: Vec<String>,
    evidence_status: String,
    official_label_status: String,
    manual_label_status: String,
    acceptance_ready: bool,
    capture_acceptance_ready: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    source_kinds: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    promotion_statuses: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    input_counts: BTreeMap<String, usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    blockers: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    missing: Vec<String>,
}

#[derive(Debug, Clone)]
struct CaptureSessionEvidenceReadiness {
    status: String,
    expected_capture_session_ids: Vec<String>,
    raw_evidence_count: usize,
    decoded_frame_count: usize,
    raw_evidence_time_bounds: Option<LocalHealthValidationEvidenceTimeBounds>,
    decoded_frame_time_bounds: Option<LocalHealthValidationEvidenceTimeBounds>,
    packet_family_counts: BTreeMap<String, usize>,
    missing_capture_session_ids: Vec<String>,
    issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct LocalHealthValidationNextAction {
    case_id: String,
    scope: String,
    reason: String,
    action: String,
}

fn read_manifest_value(path: &Path) -> bull_core::BullResult<Value> {
    let raw = fs::read_to_string(path).map_err(|source| BullError::io(path, source))?;
    serde_json::from_str(&raw)
        .map_err(|source| BullError::message(format!("invalid validation manifest: {source}")))
}

fn parse_manifest(value: Value) -> bull_core::BullResult<LocalHealthValidationManifest> {
    serde_json::from_value(value)
        .map_err(|source| BullError::message(format!("invalid validation manifest: {source}")))
}

fn run_manifest(
    database_path: &str,
    mut database_source: LocalHealthValidationDatabaseSource,
    manifest_root: &Path,
    manifest: &LocalHealthValidationManifest,
) -> LocalHealthValidationSuiteReport {
    let mut issues = Vec::new();
    if manifest.schema != MANIFEST_SCHEMA {
        issues.push(format!(
            "manifest_schema_mismatch:{}",
            manifest.schema.trim()
        ));
    }
    if manifest.cases.is_empty() {
        issues.push("no_validation_cases".to_string());
    }
    if let Some(raw_export_manifest) = &database_source.raw_export_manifest
        && !raw_export_manifest.ok
    {
        for issue in &raw_export_manifest.issues {
            issues.push(format!("database_source:{issue}"));
        }
    }
    if let Some(sqlite_audit) = &database_source.sqlite_audit
        && !sqlite_audit.ok
    {
        for issue in &sqlite_audit.issues {
            issues.push(format!("database_source:{issue}"));
        }
    }

    let capture_sqlite_imports = run_capture_sqlite_imports(
        database_path,
        manifest_root,
        &manifest.capture_sqlite_imports,
    );
    for import in &capture_sqlite_imports {
        for issue in &import.issues {
            issues.push(format!("capture_sqlite_import:{}:{issue}", import.id));
        }
    }

    let resolved_cases = manifest
        .cases
        .iter()
        .map(|case| case_with_manifest_defaults(manifest, case))
        .collect::<Vec<_>>();
    let database_source_case_window_issues =
        raw_export_case_window_issues(&database_source, &resolved_cases);
    for issue in &database_source_case_window_issues {
        issues.push(format!("database_source:{issue}"));
    }
    let database_source_packet_evidence_issues =
        raw_export_packet_evidence_issues(&database_source, &resolved_cases);
    for issue in &database_source_packet_evidence_issues {
        issues.push(format!("database_source:{issue}"));
    }
    let database_source_case_packet_evidence =
        raw_export_case_packet_evidence_audits(database_path, &database_source, &resolved_cases);
    let database_source_case_packet_evidence_issues = database_source_case_packet_evidence
        .iter()
        .flat_map(|audit| audit.issues.iter().cloned())
        .collect::<Vec<_>>();
    for issue in &database_source_case_packet_evidence_issues {
        issues.push(format!("database_source:{issue}"));
    }
    database_source.case_packet_evidence_summary =
        case_packet_evidence_summary_for(&database_source_case_packet_evidence);
    database_source.case_packet_evidence = database_source_case_packet_evidence;
    let cases = resolved_cases
        .iter()
        .map(|case| run_case(database_path, case))
        .collect::<Vec<_>>();
    let ok_case_count = cases.iter().filter(|case| case.ok).count();
    let passing_case_count = cases.iter().filter(|case| case.pass).count();
    let failing_case_count = cases.len().saturating_sub(passing_case_count);
    let metric_records = cases
        .iter()
        .flat_map(|case| case.metric_records.iter().cloned())
        .collect::<Vec<_>>();
    let readiness_summary = readiness_summary_for_cases(&cases);
    for case in &cases {
        for issue in &case.issues {
            issues.push(format!("{}:{issue}", case.id));
        }
    }
    issues.sort();
    issues.dedup();
    let next_actions = next_actions_for_report(
        &cases,
        &capture_sqlite_imports,
        &database_source,
        &database_source_case_window_issues,
        &database_source_packet_evidence_issues,
        &database_source_case_packet_evidence_issues,
    );

    LocalHealthValidationSuiteReport {
        schema: REPORT_SCHEMA.to_string(),
        generated_by: "bull-local-health-validation-suite".to_string(),
        pass: issues.is_empty() && cases.iter().all(|case| case.ok && case.pass),
        manifest_schema: manifest.schema.clone(),
        manifest_id: manifest.manifest_id.clone(),
        notes: manifest.notes.clone(),
        database_path: database_path.to_string(),
        database_source,
        capture_sqlite_imports,
        label_policy: LABEL_POLICY.to_string(),
        case_count: cases.len(),
        ok_case_count,
        passing_case_count,
        failing_case_count,
        readiness_summary,
        metric_records,
        cases,
        issues,
        next_actions,
    }
}

fn write_markdown_report(
    report: &LocalHealthValidationSuiteReport,
    path: &Path,
) -> bull_core::BullResult<()> {
    write_markdown_text(&markdown_report(report), path)
}

fn write_json_file<T: Serialize>(report: &T, path: &Path) -> bull_core::BullResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| BullError::io(parent, source))?;
    }
    let json = serde_json::to_string_pretty(report)
        .map_err(|source| BullError::message(format!("cannot serialize report: {source}")))?;
    fs::write(path, json.as_bytes()).map_err(|source| BullError::io(path, source))
}

fn write_markdown_text(markdown: &str, path: &Path) -> bull_core::BullResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| BullError::io(parent, source))?;
    }
    fs::write(path, markdown.as_bytes()).map_err(|source| BullError::io(path, source))
}

fn markdown_report(report: &LocalHealthValidationSuiteReport) -> String {
    let mut markdown = String::new();
    let manifest_id = report.manifest_id.as_deref().unwrap_or("unnamed");
    let outcome = if report.pass { "pass" } else { "fail" };
    let _ = writeln!(markdown, "# Local Health Validation Report");
    let _ = writeln!(markdown);
    let _ = writeln!(
        markdown,
        "- Manifest: {}",
        markdown_escape_inline(manifest_id)
    );
    let _ = writeln!(markdown, "- Outcome: `{outcome}`");
    let _ = writeln!(
        markdown,
        "- Cases: {} total, {} ok, {} passing, {} failing",
        report.case_count,
        report.ok_case_count,
        report.passing_case_count,
        report.failing_case_count
    );
    let _ = writeln!(
        markdown,
        "- Database source: `{}`",
        markdown_escape_inline(&report.database_source.kind)
    );
    let _ = writeln!(
        markdown,
        "- Database path: `{}`",
        markdown_escape_inline(&report.database_path)
    );
    let _ = writeln!(markdown, "- Label policy: `{}`", report.label_policy);
    if let Some(notes) = non_empty_string(report.notes.as_deref()) {
        let _ = writeln!(markdown, "- Notes: {}", markdown_escape_inline(&notes));
    }

    let summary = &report.readiness_summary;
    let _ = writeln!(markdown);
    let _ = writeln!(markdown, "## Readiness");
    let _ = writeln!(markdown);
    let _ = writeln!(markdown, "| Check | Count |");
    let _ = writeln!(markdown, "| --- | ---: |");
    let _ = writeln!(
        markdown,
        "| Acceptance-ready cases | {} |",
        summary.acceptance_ready_case_count
    );
    let _ = writeln!(
        markdown,
        "| Capture-acceptance-ready cases | {} |",
        summary.capture_acceptance_ready_case_count
    );
    let _ = writeln!(
        markdown,
        "| Missing packet evidence | {} |",
        summary.missing_packet_evidence_case_count
    );
    let _ = writeln!(
        markdown,
        "| Missing/invalid official labels | {} |",
        summary.missing_or_invalid_official_label_case_count
    );
    let _ = writeln!(
        markdown,
        "| Missing manual labels | {} |",
        summary.manual_label_missing_case_count
    );
    let _ = writeln!(
        markdown,
        "| Unavailable-status audits | {} |",
        summary.unavailable_status_case_count
    );
    let _ = writeln!(
        markdown,
        "| Declared capture-session cases | {} |",
        summary.capture_session_declared_case_count
    );
    let _ = writeln!(
        markdown,
        "| Capture session required | {} |",
        summary.capture_session_required_case_count
    );
    let _ = writeln!(
        markdown,
        "| Capture session missing evidence | {} |",
        summary.capture_session_missing_evidence_case_count
    );
    let _ = writeln!(
        markdown,
        "| Capture session sparse evidence | {} |",
        summary.capture_session_sparse_evidence_case_count
    );
    let _ = writeln!(
        markdown,
        "| Capture session wrong packet family | {} |",
        summary.capture_session_unrelated_packet_family_case_count
    );

    append_markdown_database_source(&mut markdown, &report.database_source);
    append_markdown_next_actions(&mut markdown, &report.next_actions);
    append_markdown_case_matrix(&mut markdown, &report.cases);
    append_markdown_metric_records(&mut markdown, &report.metric_records);
    append_markdown_issues(&mut markdown, &report.issues);
    markdown
}

fn append_markdown_database_source(
    markdown: &mut String,
    database_source: &LocalHealthValidationDatabaseSource,
) {
    let _ = writeln!(markdown);
    let _ = writeln!(markdown, "## Evidence Source");
    let _ = writeln!(markdown);
    let _ = writeln!(
        markdown,
        "- Input: `{}`",
        markdown_escape_inline(&database_source.input_path)
    );
    let _ = writeln!(
        markdown,
        "- Resolved database: `{}`",
        markdown_escape_inline(&database_source.resolved_database_path)
    );
    if let Some(archive_entry) = database_source.archive_entry.as_deref() {
        let _ = writeln!(
            markdown,
            "- Archive entry: `{}`",
            markdown_escape_inline(archive_entry)
        );
    }
    let _ = writeln!(
        markdown,
        "- Temporary extracted database: `{}`",
        database_source.temporary_extracted_database
    );
    if let Some(manifest) = &database_source.raw_export_manifest {
        let _ = writeln!(
            markdown,
            "- Raw Export manifest: present `{}`, ok `{}`",
            manifest.present, manifest.ok
        );
        if let (Some(start), Some(end)) = (
            non_empty_string(manifest.time_window_start.as_deref()),
            non_empty_string(manifest.time_window_end.as_deref()),
        ) {
            let _ = writeln!(
                markdown,
                "- Raw Export window: `{}` to `{}`",
                markdown_escape_inline(&start),
                markdown_escape_inline(&end)
            );
        }
    }
    if let Some(sqlite_audit) = &database_source.sqlite_audit {
        let schema = sqlite_audit
            .storage_schema_version
            .map(|version| version.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let _ = writeln!(
            markdown,
            "- SQLite audit: ok `{}`, schema `{}`",
            sqlite_audit.ok, schema
        );
        if let Some(count) = sqlite_audit.raw_evidence_time_window_count {
            let _ = writeln!(markdown, "- Raw evidence rows in export window: `{count}`");
        }
        if let Some(count) = sqlite_audit.decoded_frames_time_window_count {
            let _ = writeln!(markdown, "- Decoded frames in export window: `{count}`");
        }
    }
    if let Some(summary) = &database_source.case_packet_evidence_summary {
        let _ = writeln!(markdown);
        let _ = writeln!(markdown, "### Case Packet Evidence");
        let _ = writeln!(markdown);
        let _ = writeln!(markdown, "| Check | Count |");
        let _ = writeln!(markdown, "| --- | ---: |");
        let _ = writeln!(
            markdown,
            "| Cases with decoded packet evidence | {} |",
            summary.decoded_packet_evidence_case_count
        );
        let _ = writeln!(
            markdown,
            "| Cases with raw-only packet evidence | {} |",
            summary.raw_only_packet_evidence_case_count
        );
        let _ = writeln!(
            markdown,
            "| Cases with no packet evidence | {} |",
            summary.no_packet_evidence_case_count
        );
        let _ = writeln!(
            markdown,
            "| Cases outside Raw Export window | {} |",
            summary.outside_raw_export_time_window_case_count
        );
        let _ = writeln!(
            markdown,
            "| Sparse decoded evidence blockers | {} |",
            summary.decoded_evidence_too_sparse_for_capture_acceptance_case_count
                + summary
                    .capture_session_decoded_evidence_too_sparse_for_capture_acceptance_case_count
        );
        let _ = writeln!(
            markdown,
            "| Wrong packet-family blockers | {} |",
            summary.packet_family_unrelated_for_capture_acceptance_case_count
                + summary.capture_session_packet_family_unrelated_for_capture_acceptance_case_count
        );
        append_markdown_counts(markdown, "Packet Families", &summary.packet_family_counts);
        append_markdown_counts(
            markdown,
            "Relevant Packet Families",
            &summary.relevant_packet_family_counts,
        );
        append_markdown_counts(
            markdown,
            "Capture Session Packet Families",
            &summary.capture_session_packet_family_counts,
        );
    }
}

fn append_markdown_next_actions(
    markdown: &mut String,
    actions: &[LocalHealthValidationNextAction],
) {
    let _ = writeln!(markdown);
    let _ = writeln!(markdown, "## Next Actions");
    let _ = writeln!(markdown);
    if actions.is_empty() {
        let _ = writeln!(markdown, "No next actions reported.");
        return;
    }
    let _ = writeln!(markdown, "| Case | Scope | Reason | Action |");
    let _ = writeln!(markdown, "| --- | --- | --- | --- |");
    for action in actions {
        let _ = writeln!(
            markdown,
            "| {} | {} | {} | {} |",
            markdown_table_cell(&action.case_id),
            markdown_table_cell(&action.scope),
            markdown_table_cell(&action.reason),
            markdown_table_cell(&action.action)
        );
    }
}

fn append_markdown_case_matrix(markdown: &mut String, cases: &[LocalHealthValidationCaseReport]) {
    let _ = writeln!(markdown);
    let _ = writeln!(markdown, "## Cases");
    let _ = writeln!(markdown);
    if cases.is_empty() {
        let _ = writeln!(markdown, "No validation cases were defined.");
        return;
    }
    let _ = writeln!(
        markdown,
        "| Case | Report | Pass | Evidence | Capture Session | Source | Promotion | Blockers |"
    );
    let _ = writeln!(
        markdown,
        "| --- | --- | --- | --- | --- | --- | --- | --- |"
    );
    for case in cases {
        let readiness = &case.readiness;
        let source_kinds = markdown_join(&readiness.source_kinds);
        let promotion_statuses = markdown_join(&readiness.promotion_statuses);
        let blockers = markdown_join(&readiness.blockers);
        let _ = writeln!(
            markdown,
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            markdown_table_cell(&case.id),
            markdown_table_cell(&case.report),
            markdown_table_cell(if case.pass { "yes" } else { "no" }),
            markdown_table_cell(&readiness.evidence_status),
            markdown_table_cell(&readiness.capture_session_status),
            markdown_table_cell(&source_kinds),
            markdown_table_cell(&promotion_statuses),
            markdown_table_cell(&blockers)
        );
    }
}

fn append_markdown_metric_records(
    markdown: &mut String,
    records: &[LocalHealthValidationMetricRecord],
) {
    let _ = writeln!(markdown);
    let _ = writeln!(markdown, "## Metric Records");
    let _ = writeln!(markdown);
    if records.is_empty() {
        let _ = writeln!(markdown, "No metric records were emitted.");
        return;
    }
    let _ = writeln!(
        markdown,
        "| Case | Metric | Source | Local | Official Label | Confidence | Promotion | Algorithm | Blockers |"
    );
    let _ = writeln!(
        markdown,
        "| --- | --- | --- | --- | --- | ---: | --- | --- | --- |"
    );
    for record in records {
        let metric = format!(
            "{}.{} ({})",
            record.metric_family, record.metric_name, record.unit
        );
        let local_value = record
            .local_value
            .as_ref()
            .map(markdown_value)
            .unwrap_or_else(|| "--".to_string());
        let official_label = record
            .official_label_value
            .as_ref()
            .map(markdown_value)
            .unwrap_or_else(|| "--".to_string());
        let confidence = record
            .confidence
            .map(|value| format!("{value:.3}"))
            .unwrap_or_else(|| "--".to_string());
        let promotion = record
            .promotion_status
            .as_deref()
            .unwrap_or("--")
            .to_string();
        let algorithm = match (
            record.algorithm_id.as_deref(),
            record.algorithm_version.as_deref(),
        ) {
            (Some(id), Some(version)) => format!("{id}@{version}"),
            (Some(id), None) => id.to_string(),
            (None, Some(version)) => version.to_string(),
            (None, None) => "--".to_string(),
        };
        let blockers = markdown_join(&record.blockers);
        let _ = writeln!(
            markdown,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            markdown_table_cell(&record.case_id),
            markdown_table_cell(&metric),
            markdown_table_cell(&record.source_kind),
            markdown_table_cell(&local_value),
            markdown_table_cell(&official_label),
            markdown_table_cell(&confidence),
            markdown_table_cell(&promotion),
            markdown_table_cell(&algorithm),
            markdown_table_cell(&blockers)
        );
    }
}

fn append_markdown_issues(markdown: &mut String, issues: &[String]) {
    if issues.is_empty() {
        return;
    }
    let _ = writeln!(markdown);
    let _ = writeln!(markdown, "## Issues");
    let _ = writeln!(markdown);
    for issue in issues {
        let _ = writeln!(markdown, "- {}", markdown_escape_inline(issue));
    }
}

fn append_markdown_counts(markdown: &mut String, title: &str, counts: &BTreeMap<String, i64>) {
    if counts.is_empty() {
        return;
    }
    let _ = writeln!(markdown);
    let _ = writeln!(markdown, "### {title}");
    let _ = writeln!(markdown);
    let _ = writeln!(markdown, "| Name | Count |");
    let _ = writeln!(markdown, "| --- | ---: |");
    for (name, count) in counts {
        let _ = writeln!(markdown, "| {} | {} |", markdown_table_cell(name), count);
    }
}

fn markdown_value(value: &Value) -> String {
    match value {
        Value::Null => "--".to_string(),
        Value::String(value) => value.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        _ => serde_json::to_string(value).unwrap_or_else(|_| "<json>".to_string()),
    }
}

fn markdown_join(values: &[String]) -> String {
    if values.is_empty() {
        "--".to_string()
    } else {
        values.join(", ")
    }
}

fn markdown_escape_inline(value: &str) -> String {
    value.replace('\n', " ").replace('\r', " ")
}

fn markdown_table_cell(value: &str) -> String {
    markdown_escape_inline(value).replace('|', "\\|")
}

fn raw_export_case_window_issues(
    database_source: &LocalHealthValidationDatabaseSource,
    cases: &[LocalHealthValidationCase],
) -> Vec<String> {
    let Some(raw_export_manifest) = &database_source.raw_export_manifest else {
        return Vec::new();
    };
    let Some(export_start) = raw_export_manifest.time_window_start_unix_ms else {
        return Vec::new();
    };
    let Some(export_end) = raw_export_manifest.time_window_end_unix_ms else {
        return Vec::new();
    };
    cases
        .iter()
        .filter_map(|case| {
            if case.start.trim().is_empty() || case.end.trim().is_empty() {
                return None;
            }
            let (case_start, case_end) =
                parse_rfc3339_utc_unix_ms(&case.start).zip(parse_rfc3339_utc_unix_ms(&case.end))?;
            if case_start < export_start || case_end > export_end {
                Some(format!(
                    "case_window_outside_raw_export_time_window:{}",
                    case.id
                ))
            } else {
                None
            }
        })
        .collect()
}

fn raw_export_packet_evidence_issues(
    database_source: &LocalHealthValidationDatabaseSource,
    cases: &[LocalHealthValidationCase],
) -> Vec<String> {
    if !cases.iter().any(|case| {
        let normalized_report = normalized_report(&case.report);
        counts_for_capture_acceptance(normalized_report.as_deref())
    }) {
        return Vec::new();
    }
    let Some(sqlite_audit) = &database_source.sqlite_audit else {
        return Vec::new();
    };
    let Some(raw_count) = sqlite_audit.raw_evidence_time_window_count else {
        return Vec::new();
    };
    let Some(decoded_count) = sqlite_audit.decoded_frames_time_window_count else {
        return Vec::new();
    };
    if raw_count == 0 && decoded_count == 0 {
        vec!["no_packet_evidence_in_raw_export_time_window".to_string()]
    } else {
        Vec::new()
    }
}

fn raw_export_case_packet_evidence_audits(
    database_path: &str,
    database_source: &LocalHealthValidationDatabaseSource,
    cases: &[LocalHealthValidationCase],
) -> Vec<LocalHealthValidationCasePacketEvidenceAudit> {
    let Some(raw_export_manifest) = &database_source.raw_export_manifest else {
        return Vec::new();
    };
    if database_source
        .sqlite_audit
        .as_ref()
        .is_some_and(|audit| !audit.ok)
    {
        return Vec::new();
    }
    if !cases.iter().any(|case| {
        let normalized_report = normalized_report(&case.report);
        counts_for_capture_acceptance(normalized_report.as_deref())
    }) {
        return Vec::new();
    }
    let packet_cases = cases
        .iter()
        .filter(|case| {
            let normalized_report = normalized_report(&case.report);
            counts_for_capture_acceptance(normalized_report.as_deref())
        })
        .collect::<Vec<_>>();
    let connection = match Connection::open_with_flags(
        Path::new(database_path),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) {
        Ok(connection) => connection,
        Err(error) => {
            return packet_cases
                .iter()
                .map(|case| LocalHealthValidationCasePacketEvidenceAudit {
                    case_id: case.id.clone(),
                    report: case.report.clone(),
                    start: case.start.clone(),
                    end: case.end.clone(),
                    case_window_duration_ms: rfc3339_duration_ms(&case.start, &case.end),
                    status: "query_failed".to_string(),
                    raw_evidence_count: None,
                    decoded_frame_count: None,
                    raw_evidence_time_bounds: None,
                    decoded_frame_time_bounds: None,
                    packet_family_counts: BTreeMap::new(),
                    capture_acceptance_required_packet_family_prefixes:
                        capture_acceptance_required_packet_family_prefixes_for_case(case),
                    relevant_packet_family_counts: BTreeMap::new(),
                    capture_session_status: None,
                    expected_capture_session_ids: expected_capture_session_ids(case),
                    observed_capture_session_ids: Vec::new(),
                    missing_capture_session_ids: Vec::new(),
                    capture_session_raw_evidence_count: None,
                    capture_session_decoded_frame_count: None,
                    capture_session_raw_evidence_time_bounds: None,
                    capture_session_decoded_frame_time_bounds: None,
                    capture_session_packet_family_counts: BTreeMap::new(),
                    capture_session_relevant_packet_family_counts: BTreeMap::new(),
                    issues: vec![format!(
                        "case_window_packet_evidence_query_failed:{}:{error}",
                        case.id
                    )],
                })
                .collect();
        }
    };

    packet_cases
        .iter()
        .filter_map(|case| {
            let (case_start, case_end) =
                parse_rfc3339_utc_unix_ms(&case.start).zip(parse_rfc3339_utc_unix_ms(&case.end))?;
            let (export_start, export_end) = raw_export_manifest
                .time_window_start_unix_ms
                .zip(raw_export_manifest.time_window_end_unix_ms)?;
            if case_start < export_start || case_end > export_end {
                return Some(LocalHealthValidationCasePacketEvidenceAudit {
                    case_id: case.id.clone(),
                    report: case.report.clone(),
                    start: case.start.clone(),
                    end: case.end.clone(),
                    case_window_duration_ms: rfc3339_duration_ms(&case.start, &case.end),
                    status: "outside_raw_export_time_window".to_string(),
                    raw_evidence_count: None,
                    decoded_frame_count: None,
                    raw_evidence_time_bounds: None,
                    decoded_frame_time_bounds: None,
                    packet_family_counts: BTreeMap::new(),
                    capture_acceptance_required_packet_family_prefixes:
                        capture_acceptance_required_packet_family_prefixes_for_case(case),
                    relevant_packet_family_counts: BTreeMap::new(),
                    capture_session_status: None,
                    expected_capture_session_ids: expected_capture_session_ids(case),
                    observed_capture_session_ids: Vec::new(),
                    missing_capture_session_ids: Vec::new(),
                    capture_session_raw_evidence_count: None,
                    capture_session_decoded_frame_count: None,
                    capture_session_raw_evidence_time_bounds: None,
                    capture_session_decoded_frame_time_bounds: None,
                    capture_session_packet_family_counts: BTreeMap::new(),
                    capture_session_relevant_packet_family_counts: BTreeMap::new(),
                    issues: Vec::new(),
                });
            }
            let raw_count =
                match query_raw_evidence_time_window_count(&connection, &case.start, &case.end) {
                    Ok(count) => count,
                    Err(error) => {
                        return Some(LocalHealthValidationCasePacketEvidenceAudit {
                            case_id: case.id.clone(),
                            report: case.report.clone(),
                            start: case.start.clone(),
                            end: case.end.clone(),
                            case_window_duration_ms: rfc3339_duration_ms(&case.start, &case.end),
                            status: "query_failed".to_string(),
                            raw_evidence_count: None,
                            decoded_frame_count: None,
                            raw_evidence_time_bounds: None,
                            decoded_frame_time_bounds: None,
                            packet_family_counts: BTreeMap::new(),
                            capture_acceptance_required_packet_family_prefixes:
                                capture_acceptance_required_packet_family_prefixes_for_case(case),
                            relevant_packet_family_counts: BTreeMap::new(),
                            capture_session_status: None,
                            expected_capture_session_ids: expected_capture_session_ids(case),
                            observed_capture_session_ids: Vec::new(),
                            missing_capture_session_ids: Vec::new(),
                            capture_session_raw_evidence_count: None,
                            capture_session_decoded_frame_count: None,
                            capture_session_raw_evidence_time_bounds: None,
                            capture_session_decoded_frame_time_bounds: None,
                            capture_session_packet_family_counts: BTreeMap::new(),
                            capture_session_relevant_packet_family_counts: BTreeMap::new(),
                            issues: vec![format!(
                                "case_window_packet_evidence_query_failed:{}:{error}",
                                case.id
                            )],
                        });
                    }
                };
            let decoded_count =
                match query_decoded_frames_time_window_count(&connection, &case.start, &case.end) {
                    Ok(count) => count,
                    Err(error) => {
                        return Some(LocalHealthValidationCasePacketEvidenceAudit {
                            case_id: case.id.clone(),
                            report: case.report.clone(),
                            start: case.start.clone(),
                            end: case.end.clone(),
                            case_window_duration_ms: rfc3339_duration_ms(&case.start, &case.end),
                            status: "query_failed".to_string(),
                            raw_evidence_count: Some(raw_count),
                            decoded_frame_count: None,
                            raw_evidence_time_bounds: None,
                            decoded_frame_time_bounds: None,
                            packet_family_counts: BTreeMap::new(),
                            capture_acceptance_required_packet_family_prefixes:
                                capture_acceptance_required_packet_family_prefixes_for_case(case),
                            relevant_packet_family_counts: BTreeMap::new(),
                            capture_session_status: None,
                            expected_capture_session_ids: expected_capture_session_ids(case),
                            observed_capture_session_ids: Vec::new(),
                            missing_capture_session_ids: Vec::new(),
                            capture_session_raw_evidence_count: None,
                            capture_session_decoded_frame_count: None,
                            capture_session_raw_evidence_time_bounds: None,
                            capture_session_decoded_frame_time_bounds: None,
                            capture_session_packet_family_counts: BTreeMap::new(),
                            capture_session_relevant_packet_family_counts: BTreeMap::new(),
                            issues: vec![format!(
                                "case_window_packet_evidence_query_failed:{}:{error}",
                                case.id
                            )],
                        });
                    }
                };
            let mut issues = Vec::new();
            let raw_evidence_time_bounds =
                match query_raw_evidence_time_bounds(&connection, &case.start, &case.end, None) {
                    Ok(bounds) => bounds,
                    Err(error) => {
                        issues.push(format!(
                            "case_window_time_bounds_query_failed:{}:{error}",
                            case.id
                        ));
                        None
                    }
                };
            let decoded_frame_time_bounds =
                match query_decoded_frames_time_bounds(&connection, &case.start, &case.end, None) {
                    Ok(bounds) => bounds,
                    Err(error) => {
                        issues.push(format!(
                            "case_window_time_bounds_query_failed:{}:{error}",
                            case.id
                        ));
                        None
                    }
                };
            let packet_family_counts =
                match query_decoded_packet_family_counts(&connection, &case.start, &case.end, None)
                {
                    Ok(counts) => counts,
                    Err(error) => {
                        issues.push(format!(
                            "case_window_packet_family_query_failed:{}:{error}",
                            case.id
                        ));
                        BTreeMap::new()
                    }
                };
            let capture_session_evidence =
                raw_export_case_capture_session_evidence(&connection, case);
            if raw_count == 0 && decoded_count == 0 {
                Some(local_health_validation_case_packet_evidence_audit(
                    case,
                    "no_packet_evidence",
                    Some(raw_count),
                    Some(decoded_count),
                    raw_evidence_time_bounds,
                    decoded_frame_time_bounds,
                    packet_family_counts,
                    capture_session_evidence,
                    [
                        issues,
                        vec![format!("case_window_no_packet_evidence:{}", case.id)],
                    ]
                    .concat(),
                ))
            } else if decoded_count == 0 {
                Some(local_health_validation_case_packet_evidence_audit(
                    case,
                    "raw_only_packet_evidence",
                    Some(raw_count),
                    Some(decoded_count),
                    raw_evidence_time_bounds,
                    decoded_frame_time_bounds,
                    packet_family_counts,
                    capture_session_evidence,
                    [
                        issues,
                        vec![format!(
                            "case_window_no_decoded_packet_evidence:{}",
                            case.id
                        )],
                    ]
                    .concat(),
                ))
            } else {
                Some(local_health_validation_case_packet_evidence_audit(
                    case,
                    "decoded_packet_evidence",
                    Some(raw_count),
                    Some(decoded_count),
                    raw_evidence_time_bounds,
                    decoded_frame_time_bounds,
                    packet_family_counts,
                    capture_session_evidence,
                    issues,
                ))
            }
        })
        .collect()
}

fn case_packet_evidence_summary_for(
    audits: &[LocalHealthValidationCasePacketEvidenceAudit],
) -> Option<LocalHealthValidationCasePacketEvidenceSummary> {
    if audits.is_empty() {
        return None;
    }

    let mut summary = LocalHealthValidationCasePacketEvidenceSummary {
        case_count: audits.len(),
        ..Default::default()
    };
    for audit in audits {
        match audit.status.as_str() {
            "decoded_packet_evidence" => summary.decoded_packet_evidence_case_count += 1,
            "raw_only_packet_evidence" => summary.raw_only_packet_evidence_case_count += 1,
            "no_packet_evidence" => summary.no_packet_evidence_case_count += 1,
            "outside_raw_export_time_window" => {
                summary.outside_raw_export_time_window_case_count += 1
            }
            "query_failed" => summary.query_failed_case_count += 1,
            _ => {}
        }

        if !audit.expected_capture_session_ids.is_empty() {
            summary.declared_capture_session_case_count += 1;
            match audit.capture_session_status.as_deref() {
                Some("declared_with_evidence") => {
                    summary.declared_capture_session_with_evidence_case_count += 1
                }
                Some("declared_missing_evidence") => {
                    summary.declared_capture_session_missing_evidence_case_count += 1
                }
                Some("declared_partial_evidence") => {
                    summary.declared_capture_session_partial_evidence_case_count += 1
                }
                Some("query_failed") => {
                    summary.declared_capture_session_query_failed_case_count += 1
                }
                _ => {}
            }
        }

        summary.case_window_raw_evidence_count_sum += audit.raw_evidence_count.unwrap_or(0);
        summary.case_window_decoded_frame_count_sum += audit.decoded_frame_count.unwrap_or(0);
        summary.capture_session_raw_evidence_count_sum +=
            audit.capture_session_raw_evidence_count.unwrap_or(0);
        summary.capture_session_decoded_frame_count_sum +=
            audit.capture_session_decoded_frame_count.unwrap_or(0);

        if audit.decoded_frame_count.unwrap_or(0) > 0
            && let Some(bounds) = &audit.decoded_frame_time_bounds
        {
            if bounds.span_ms == Some(0) {
                summary.decoded_evidence_zero_span_case_count += 1;
            }
            if bounds
                .coverage_ratio
                .is_some_and(|coverage| coverage == 0.0)
            {
                summary.decoded_evidence_zero_coverage_case_count += 1;
            }
        }

        if packet_evidence_issue_prefix_present(
            &audit.issues,
            "case_window_decoded_evidence_too_sparse_for_capture_acceptance:",
        ) {
            summary.decoded_evidence_too_sparse_for_capture_acceptance_case_count += 1;
        }
        if packet_evidence_issue_prefix_present(
            &audit.issues,
            "case_window_capture_session_decoded_evidence_too_sparse_for_capture_acceptance:",
        ) {
            summary
                .capture_session_decoded_evidence_too_sparse_for_capture_acceptance_case_count += 1;
        }
        if packet_evidence_issue_prefix_present(
            &audit.issues,
            "case_window_packet_family_unrelated_for_capture_acceptance:",
        ) {
            summary.packet_family_unrelated_for_capture_acceptance_case_count += 1;
        }
        if packet_evidence_issue_prefix_present(
            &audit.issues,
            "case_window_capture_session_packet_family_unrelated_for_capture_acceptance:",
        ) {
            summary.capture_session_packet_family_unrelated_for_capture_acceptance_case_count += 1;
        }

        add_packet_family_counts(
            &mut summary.packet_family_counts,
            &audit.packet_family_counts,
        );
        add_packet_family_counts(
            &mut summary.relevant_packet_family_counts,
            &audit.relevant_packet_family_counts,
        );
        add_packet_family_counts(
            &mut summary.capture_session_packet_family_counts,
            &audit.capture_session_packet_family_counts,
        );
        add_packet_family_counts(
            &mut summary.capture_session_relevant_packet_family_counts,
            &audit.capture_session_relevant_packet_family_counts,
        );
    }

    Some(summary)
}

fn packet_evidence_issue_prefix_present(issues: &[String], prefix: &str) -> bool {
    issues.iter().any(|issue| issue.starts_with(prefix))
}

fn add_packet_family_counts(target: &mut BTreeMap<String, i64>, source: &BTreeMap<String, i64>) {
    for (family, count) in source {
        *target.entry(family.clone()).or_insert(0) += *count;
    }
}

#[derive(Debug, Clone)]
struct RawExportCaseCaptureSessionEvidenceAudit {
    status: Option<String>,
    expected_capture_session_ids: Vec<String>,
    observed_capture_session_ids: Vec<String>,
    missing_capture_session_ids: Vec<String>,
    raw_evidence_count: Option<i64>,
    decoded_frame_count: Option<i64>,
    raw_evidence_time_bounds: Option<LocalHealthValidationEvidenceTimeBounds>,
    decoded_frame_time_bounds: Option<LocalHealthValidationEvidenceTimeBounds>,
    packet_family_counts: BTreeMap<String, i64>,
    issues: Vec<String>,
}

fn local_health_validation_case_packet_evidence_audit(
    case: &LocalHealthValidationCase,
    status: &str,
    raw_evidence_count: Option<i64>,
    decoded_frame_count: Option<i64>,
    raw_evidence_time_bounds: Option<LocalHealthValidationEvidenceTimeBounds>,
    decoded_frame_time_bounds: Option<LocalHealthValidationEvidenceTimeBounds>,
    packet_family_counts: BTreeMap<String, i64>,
    capture_session_evidence: RawExportCaseCaptureSessionEvidenceAudit,
    mut issues: Vec<String>,
) -> LocalHealthValidationCasePacketEvidenceAudit {
    issues.extend(capture_session_evidence.issues);
    let capture_acceptance_required_packet_family_prefixes =
        capture_acceptance_required_packet_family_prefixes_for_case(case);
    let relevant_packet_family_counts = relevant_packet_family_counts_for_capture_acceptance(
        &capture_acceptance_required_packet_family_prefixes,
        &packet_family_counts,
    );
    let capture_session_relevant_packet_family_counts =
        relevant_packet_family_counts_for_capture_acceptance(
            &capture_acceptance_required_packet_family_prefixes,
            &capture_session_evidence.packet_family_counts,
        );
    let decoded_frame_count_value = decoded_frame_count.unwrap_or(0);
    if decoded_frame_count_value > 0
        && decoded_frame_time_bounds
            .as_ref()
            .and_then(|bounds| bounds.span_ms)
            .is_some_and(|span_ms| span_ms == 0)
    {
        issues.push(format!(
            "case_window_decoded_evidence_too_sparse_for_capture_acceptance:{}",
            case.id
        ));
    }
    if decoded_frame_count_value > 0
        && !capture_acceptance_required_packet_family_prefixes.is_empty()
        && relevant_packet_family_counts.is_empty()
    {
        issues.push(format!(
            "case_window_packet_family_unrelated_for_capture_acceptance:{}",
            case.id
        ));
    }
    if capture_session_evidence.status.as_deref() == Some("declared_with_evidence")
        && capture_session_evidence.decoded_frame_count.unwrap_or(0) > 0
    {
        if capture_session_evidence
            .decoded_frame_time_bounds
            .as_ref()
            .and_then(|bounds| bounds.span_ms)
            .is_some_and(|span_ms| span_ms == 0)
        {
            issues.push(format!(
                "case_window_capture_session_decoded_evidence_too_sparse_for_capture_acceptance:{}",
                case.id
            ));
        }
        if !capture_acceptance_required_packet_family_prefixes.is_empty()
            && capture_session_relevant_packet_family_counts.is_empty()
        {
            issues.push(format!(
                "case_window_capture_session_packet_family_unrelated_for_capture_acceptance:{}",
                case.id
            ));
        }
    }
    LocalHealthValidationCasePacketEvidenceAudit {
        case_id: case.id.clone(),
        report: case.report.clone(),
        start: case.start.clone(),
        end: case.end.clone(),
        case_window_duration_ms: rfc3339_duration_ms(&case.start, &case.end),
        status: status.to_string(),
        raw_evidence_count,
        decoded_frame_count,
        raw_evidence_time_bounds,
        decoded_frame_time_bounds,
        packet_family_counts,
        capture_acceptance_required_packet_family_prefixes,
        relevant_packet_family_counts,
        capture_session_status: capture_session_evidence.status,
        expected_capture_session_ids: capture_session_evidence.expected_capture_session_ids,
        observed_capture_session_ids: capture_session_evidence.observed_capture_session_ids,
        missing_capture_session_ids: capture_session_evidence.missing_capture_session_ids,
        capture_session_raw_evidence_count: capture_session_evidence.raw_evidence_count,
        capture_session_decoded_frame_count: capture_session_evidence.decoded_frame_count,
        capture_session_raw_evidence_time_bounds: capture_session_evidence.raw_evidence_time_bounds,
        capture_session_decoded_frame_time_bounds: capture_session_evidence
            .decoded_frame_time_bounds,
        capture_session_packet_family_counts: capture_session_evidence.packet_family_counts,
        capture_session_relevant_packet_family_counts,
        issues,
    }
}

fn raw_export_case_capture_session_evidence(
    connection: &Connection,
    case: &LocalHealthValidationCase,
) -> RawExportCaseCaptureSessionEvidenceAudit {
    let expected_capture_session_ids = expected_capture_session_ids(case);
    if expected_capture_session_ids.is_empty() {
        return RawExportCaseCaptureSessionEvidenceAudit {
            status: None,
            expected_capture_session_ids,
            observed_capture_session_ids: Vec::new(),
            missing_capture_session_ids: Vec::new(),
            raw_evidence_count: None,
            decoded_frame_count: None,
            raw_evidence_time_bounds: None,
            decoded_frame_time_bounds: None,
            packet_family_counts: BTreeMap::new(),
            issues: Vec::new(),
        };
    }

    let mut issues = Vec::new();
    let observed_capture_session_ids =
        match query_observed_capture_session_ids(connection, &case.start, &case.end) {
            Ok(session_ids) => session_ids,
            Err(error) => {
                issues.push(format!(
                    "case_window_capture_session_query_failed:{}:{error}",
                    case.id
                ));
                Vec::new()
            }
        };
    let raw_evidence_count = match query_capture_session_raw_evidence_time_window_count(
        connection,
        &case.start,
        &case.end,
        &expected_capture_session_ids,
    ) {
        Ok(count) => Some(count),
        Err(error) => {
            issues.push(format!(
                "case_window_capture_session_query_failed:{}:{error}",
                case.id
            ));
            None
        }
    };
    let decoded_frame_count = match query_capture_session_decoded_frames_time_window_count(
        connection,
        &case.start,
        &case.end,
        &expected_capture_session_ids,
    ) {
        Ok(count) => Some(count),
        Err(error) => {
            issues.push(format!(
                "case_window_capture_session_query_failed:{}:{error}",
                case.id
            ));
            None
        }
    };
    let raw_evidence_time_bounds = match query_raw_evidence_time_bounds(
        connection,
        &case.start,
        &case.end,
        Some(&expected_capture_session_ids),
    ) {
        Ok(bounds) => bounds,
        Err(error) => {
            issues.push(format!(
                "case_window_capture_session_time_bounds_query_failed:{}:{error}",
                case.id
            ));
            None
        }
    };
    let decoded_frame_time_bounds = match query_decoded_frames_time_bounds(
        connection,
        &case.start,
        &case.end,
        Some(&expected_capture_session_ids),
    ) {
        Ok(bounds) => bounds,
        Err(error) => {
            issues.push(format!(
                "case_window_capture_session_time_bounds_query_failed:{}:{error}",
                case.id
            ));
            None
        }
    };
    let packet_family_counts = match query_decoded_packet_family_counts(
        connection,
        &case.start,
        &case.end,
        Some(&expected_capture_session_ids),
    ) {
        Ok(counts) => counts,
        Err(error) => {
            issues.push(format!(
                "case_window_capture_session_packet_family_query_failed:{}:{error}",
                case.id
            ));
            BTreeMap::new()
        }
    };

    let observed = observed_capture_session_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let missing_capture_session_ids = expected_capture_session_ids
        .iter()
        .filter(|session_id| !observed.contains(*session_id))
        .cloned()
        .collect::<Vec<_>>();
    let status = if !issues.is_empty() {
        "query_failed"
    } else if missing_capture_session_ids.is_empty() {
        "declared_with_evidence"
    } else if missing_capture_session_ids.len() == expected_capture_session_ids.len() {
        "declared_missing_evidence"
    } else {
        "declared_partial_evidence"
    };

    RawExportCaseCaptureSessionEvidenceAudit {
        status: Some(status.to_string()),
        expected_capture_session_ids,
        observed_capture_session_ids,
        missing_capture_session_ids,
        raw_evidence_count,
        decoded_frame_count,
        raw_evidence_time_bounds,
        decoded_frame_time_bounds,
        packet_family_counts,
        issues,
    }
}

fn case_with_manifest_defaults(
    manifest: &LocalHealthValidationManifest,
    case: &LocalHealthValidationCase,
) -> LocalHealthValidationCase {
    let mut resolved = case.clone();
    if resolved.start.trim().is_empty()
        && let Some(start) = non_empty_string(manifest.default_start.as_deref())
    {
        resolved.start = start;
    }
    if resolved.end.trim().is_empty()
        && let Some(end) = non_empty_string(manifest.default_end.as_deref())
    {
        resolved.end = end;
    }
    if resolved
        .date_key
        .as_deref()
        .is_none_or(|date_key| date_key.trim().is_empty())
    {
        resolved.date_key = non_empty_string(manifest.default_date_key.as_deref());
    }
    if resolved
        .timezone
        .as_deref()
        .is_none_or(|timezone| timezone.trim().is_empty())
    {
        resolved.timezone = non_empty_string(manifest.default_timezone.as_deref());
    }
    if resolved
        .capture_session_id
        .as_deref()
        .is_none_or(|capture_session_id| capture_session_id.trim().is_empty())
        && let Some(capture_session_id) =
            non_empty_string(manifest.default_capture_session_id.as_deref())
    {
        resolved.capture_session_id = Some(capture_session_id);
    }
    if resolved.capture_session_ids.is_empty() {
        resolved.capture_session_ids = manifest
            .default_capture_session_ids
            .iter()
            .filter_map(|id| non_empty_string(Some(id.as_str())))
            .collect();
    }
    if resolved.min_owned_captures.is_none() {
        resolved.min_owned_captures = manifest.default_min_owned_captures;
    }
    if resolved.profile_weight_kg.is_none() {
        resolved.profile_weight_kg = manifest.default_profile_weight_kg;
    }
    if resolved.profile_age_years.is_none() {
        resolved.profile_age_years = manifest.default_profile_age_years;
    }
    if resolved
        .profile_sex
        .as_deref()
        .is_none_or(|profile_sex| profile_sex.trim().is_empty())
    {
        resolved.profile_sex = non_empty_string(manifest.default_profile_sex.as_deref());
    }
    if resolved.resting_hr_bpm.is_none() {
        resolved.resting_hr_bpm = manifest.default_resting_hr_bpm;
    }
    if resolved.max_hr_bpm.is_none() {
        resolved.max_hr_bpm = manifest.default_max_hr_bpm;
    }
    if resolved.label_provenance.is_none() {
        resolved.label_provenance = manifest.default_label_provenance.clone();
    }
    resolved
}

fn non_empty_string(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn run_capture_sqlite_imports(
    database_path: &str,
    manifest_root: &Path,
    imports: &[LocalHealthValidationCaptureSqliteImport],
) -> Vec<LocalHealthValidationCaptureSqliteImportReport> {
    imports
        .iter()
        .map(|import| run_capture_sqlite_import(database_path, manifest_root, import))
        .collect()
}

fn run_capture_sqlite_import(
    database_path: &str,
    manifest_root: &Path,
    import: &LocalHealthValidationCaptureSqliteImport,
) -> LocalHealthValidationCaptureSqliteImportReport {
    let mut issues = validate_capture_sqlite_import(import);
    let capture_sqlite_path = resolve_manifest_path(manifest_root, &import.path);
    let parser_version = import.parser_version.clone().unwrap_or_else(|| {
        format!(
            "bull-core/{}",
            option_env!("CARGO_PKG_VERSION").unwrap_or("unknown")
        )
    });
    let device_model = import
        .device_model
        .clone()
        .unwrap_or_else(|| "WHOOP 5.0 Bull".to_string());
    let sensitivity = import
        .sensitivity
        .clone()
        .unwrap_or_else(|| "user-owned-capture".to_string());

    if let Err(error) = ensure_database_parent(Path::new(database_path)) {
        issues.push(format!("capture_sqlite_database_parent_failed:{error}"));
    }

    if !issues.is_empty() {
        return capture_sqlite_import_error_report(
            import,
            &capture_sqlite_path,
            issues,
            Some("capture sqlite import declaration invalid".to_string()),
        );
    }

    let store = match BullStore::open(Path::new(database_path)) {
        Ok(store) => store,
        Err(error) => {
            return capture_sqlite_import_error_report(
                import,
                &capture_sqlite_path,
                vec!["capture_sqlite_database_open_failed".to_string()],
                Some(error.to_string()),
            );
        }
    };

    match import_capture_sqlite(
        &store,
        CaptureSqliteImportOptions {
            source_database_path: &capture_sqlite_path,
            target_database_path: Path::new(database_path),
            session_id: &import.session_id,
            device_model: &device_model,
            sensitivity: &sensitivity,
            parser_version: &parser_version,
        },
    ) {
        Ok(report) => {
            let mut issues = Vec::new();
            if report.source_frame_count == 0 {
                issues.push("capture_sqlite_import_empty".to_string());
            }
            if !report.raw_import_completed {
                issues.push("capture_sqlite_raw_import_incomplete".to_string());
            }
            LocalHealthValidationCaptureSqliteImportReport {
                id: import.id.clone(),
                capture_sqlite_path: capture_sqlite_path.display().to_string(),
                session_id: import.session_id.clone(),
                ok: true,
                import_ready: issues.is_empty(),
                raw_import_completed: report.raw_import_completed,
                decode_pass: report.decode_pass,
                source_frame_count: report.source_frame_count,
                raw_inserted: report.raw_inserted,
                raw_existing: report.raw_existing,
                frames_inserted: report.frames_inserted,
                frames_existing: report.frames_existing,
                parse_failed_count: report.parse_failed_count,
                issues,
                error: None,
            }
        }
        Err(error) => capture_sqlite_import_error_report(
            import,
            &capture_sqlite_path,
            vec!["capture_sqlite_import_failed".to_string()],
            Some(error.to_string()),
        ),
    }
}

fn validate_capture_sqlite_import(
    import: &LocalHealthValidationCaptureSqliteImport,
) -> Vec<String> {
    let mut issues = Vec::new();
    if import.id.trim().is_empty() {
        issues.push("capture_sqlite_import_id_required".to_string());
    }
    if import.path.trim().is_empty() {
        issues.push("capture_sqlite_import_path_required".to_string());
    }
    if import.session_id.trim().is_empty() {
        issues.push("capture_sqlite_import_session_id_required".to_string());
    }
    issues
}

fn capture_sqlite_import_error_report(
    import: &LocalHealthValidationCaptureSqliteImport,
    capture_sqlite_path: &Path,
    issues: Vec<String>,
    error: Option<String>,
) -> LocalHealthValidationCaptureSqliteImportReport {
    LocalHealthValidationCaptureSqliteImportReport {
        id: import.id.clone(),
        capture_sqlite_path: capture_sqlite_path.display().to_string(),
        session_id: import.session_id.clone(),
        ok: false,
        import_ready: false,
        raw_import_completed: false,
        decode_pass: false,
        source_frame_count: 0,
        raw_inserted: 0,
        raw_existing: 0,
        frames_inserted: 0,
        frames_existing: 0,
        parse_failed_count: 0,
        issues,
        error,
    }
}

fn resolve_manifest_path(manifest_root: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        manifest_root.join(path)
    }
}

fn run_case(
    database_path: &str,
    case: &LocalHealthValidationCase,
) -> LocalHealthValidationCaseReport {
    let mut issues = validate_case(case);
    let capture_session_evidence = capture_session_evidence_for_case(database_path, case);
    issues.extend(capture_session_evidence.issues.iter().cloned());
    let (bridge_database_path, scoped_database_path) =
        match scoped_database_path_for_case(database_path, case, &capture_session_evidence) {
            Ok(Some(path)) => (path.display().to_string(), Some(path)),
            Ok(None) => (database_path.to_string(), None),
            Err(error) => {
                issues.push(format!("capture_session_scope_failed:{error}"));
                (database_path.to_string(), None)
            }
        };
    let Some(method) = case_method(&case.report) else {
        issues.push(format!("unsupported_report:{}", case.report));
        return LocalHealthValidationCaseReport {
            id: case.id.clone(),
            report: case.report.clone(),
            method: None,
            ok: false,
            pass: false,
            label_policy_valid: label_policy_valid(case),
            readiness: readiness_for_case(
                case,
                None,
                false,
                false,
                &issues,
                &[],
                None,
                &capture_session_evidence,
            ),
            issues,
            metric_records: Vec::new(),
            result: None,
            error: None,
        };
    };

    let response = handle_bridge_request(BridgeRequest {
        schema: BRIDGE_REQUEST_SCHEMA.to_string(),
        request_id: format!("local-health-validation-suite:{}", case.id),
        method: method.to_string(),
        args: case_args(&bridge_database_path, case, &capture_session_evidence),
    });
    if let Some(path) = scoped_database_path {
        if let Err(error) = persist_scoped_formatted_metric_writes(database_path, &path) {
            issues.push(format!("scoped_metric_write_persist_failed:{error}"));
        }
        let _ = fs::remove_file(path);
    }
    let ok = response.ok;
    let result = response.result;
    let pass = result
        .as_ref()
        .and_then(|value| value.get("pass"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !ok {
        issues.push("bridge_method_failed".to_string());
    }
    if ok && !pass {
        issues.push("case_report_not_passed".to_string());
    }
    let metric_records = result
        .as_ref()
        .map(|value| metric_records_for_case(&case.id, &case.report, Some(method), value))
        .unwrap_or_default();
    let readiness = readiness_for_case(
        case,
        Some(method),
        ok,
        pass,
        &issues,
        &metric_records,
        result.as_ref(),
        &capture_session_evidence,
    );

    LocalHealthValidationCaseReport {
        id: case.id.clone(),
        report: case.report.clone(),
        method: Some(method.to_string()),
        ok,
        pass,
        label_policy_valid: label_policy_valid(case),
        issues,
        readiness,
        metric_records,
        result,
        error: response.error,
    }
}

fn metric_records_for_case(
    case_id: &str,
    report: &str,
    method: Option<&str>,
    result: &Value,
) -> Vec<LocalHealthValidationMetricRecord> {
    match str_field(result, &["schema"]).as_deref() {
        Some("bull.step-packet-discovery-report.v1") => {
            vec![step_discovery_metric_record(
                case_id, report, method, result,
            )]
        }
        Some("bull.step-capture-validation-report.v1") => {
            vec![step_validation_metric_record(
                case_id, report, method, result,
            )]
        }
        Some("bull.raw-motion-step-estimate-report.v1") => {
            vec![raw_motion_step_metric_record(
                case_id, report, method, result,
            )]
        }
        Some("bull.step-counter-daily-rollup-report.v1") => {
            vec![step_counter_rollup_metric_record(
                case_id,
                report,
                method,
                result,
                "daily_rollup",
            )]
        }
        Some("bull.step-counter-hourly-rollup-report.v1") => {
            vec![step_counter_rollup_metric_record(
                case_id,
                report,
                method,
                result,
                "hourly_rollup",
            )]
        }
        Some("bull.activity-unavailable-daily-status-report.v1") => {
            activity_unavailable_status_metric_records(case_id, report, method, result)
        }
        Some("bull.energy-capture-validation-report.v1") => {
            energy_validation_metric_records(case_id, report, method, result)
        }
        Some("bull.energy-daily-rollup-report.v1") => {
            energy_rollup_metric_records(case_id, report, method, result, "daily_rollup")
        }
        Some("bull.energy-unavailable-daily-status-report.v1") => {
            energy_unavailable_status_metric_records(case_id, report, method, result)
        }
        Some("bull.energy-hourly-rollup-report.v1") => {
            energy_rollup_metric_records(case_id, report, method, result, "hourly_rollup")
        }
        Some("bull.resting-heart-rate-capture-validation-report.v1") => {
            vec![rhr_validation_metric_record(
                case_id, report, method, result,
            )]
        }
        Some("bull.resting-heart-rate-daily-rollup-report.v1") => {
            vec![rhr_rollup_metric_record(case_id, report, method, result)]
        }
        Some("bull.hrv-capture-validation-report.v1") => {
            vec![hrv_validation_metric_record(
                case_id, report, method, result,
            )]
        }
        Some("bull.respiratory-rate-capture-validation-report.v1") => {
            vec![respiratory_rate_validation_metric_record(
                case_id, report, method, result,
            )]
        }
        Some("bull.oxygen-saturation-capture-validation-report.v1") => {
            vec![oxygen_saturation_validation_metric_record(
                case_id, report, method, result,
            )]
        }
        Some("bull.temperature-capture-validation-report.v1") => {
            vec![temperature_validation_metric_record(
                case_id, report, method, result,
            )]
        }
        Some("bull.recovery-sensor-discovery-report.v1") => {
            recovery_sensor_metric_records(case_id, report, method, result)
        }
        Some("bull.recovery-sensor-daily-rollup-report.v1") => {
            recovery_sensor_daily_rollup_metric_records(case_id, report, method, result)
        }
        Some("bull.recovery-unavailable-daily-status-report.v1") => {
            recovery_unavailable_status_metric_records(case_id, report, method, result)
        }
        _ => Vec::new(),
    }
}

fn step_discovery_metric_record(
    case_id: &str,
    report: &str,
    method: Option<&str>,
    result: &Value,
) -> LocalHealthValidationMetricRecord {
    let local_value = bool_field(result, &["explicit_step_counter_found"])
        .filter(|explicit_found| *explicit_found)
        .map(Value::Bool);
    let candidate_field_count = usize_field(result, &["candidate_field_count"]).unwrap_or_default();
    let monotonic_counter_candidate_count =
        usize_field(result, &["monotonic_counter_candidate_count"]).unwrap_or_default();
    let promotion_status = if local_value.is_some() {
        "device_counter_candidate"
    } else if monotonic_counter_candidate_count > 0 {
        "unnamed_monotonic_counter_candidate"
    } else if candidate_field_count > 0 {
        "step_like_candidate_no_counter"
    } else {
        "no_decoded_step_counter"
    };
    metric_record(
        case_id,
        report,
        method,
        "activity",
        "step_counter_presence",
        "boolean",
        source_kind_for_value("device_counter", &local_value),
        bool_field(result, &["pass"]).unwrap_or(false),
        local_value,
        None,
        None,
        usize_field(result, &["decoded_frame_count"]),
        input_counts(&[
            (
                "decoded_frame_count",
                usize_field(result, &["decoded_frame_count"]),
            ),
            (
                "inspected_frame_count",
                usize_field(result, &["inspected_frame_count"]),
            ),
            (
                "candidate_field_count",
                usize_field(result, &["candidate_field_count"]),
            ),
            (
                "emitted_candidate_field_count",
                usize_field(result, &["emitted_candidate_field_count"]),
            ),
            (
                "monotonic_counter_candidate_count",
                usize_field(result, &["monotonic_counter_candidate_count"]),
            ),
            (
                "emitted_monotonic_counter_sample_count",
                usize_field(result, &["emitted_monotonic_counter_sample_count"]),
            ),
            (
                "counter_delta_candidate_count",
                usize_field(result, &["counter_delta_candidate_count"]),
            ),
            (
                "monotonic_counter_delta_candidate_count",
                usize_field(result, &["monotonic_counter_delta_candidate_count"]),
            ),
            (
                "selected_counter_delta_rank",
                usize_field(result, &["selected_counter_delta", "rank"]),
            ),
        ]),
        None,
        Some("bull.steps.step_packet_discovery.v0".to_string()),
        Some("0.1.0".to_string()),
        None,
        Some(promotion_status.to_string()),
        string_array(result, &["issues"]),
        selected_counter_delta_quality_flags(result),
        string_array(result, &["issues"]),
    )
}

fn step_validation_metric_record(
    case_id: &str,
    report: &str,
    method: Option<&str>,
    result: &Value,
) -> LocalHealthValidationMetricRecord {
    let local_value = selected_explicit_counter_delta_value(result);
    let promotion_status = if string_array(result, &["issues"]).iter().any(|issue| {
        issue == "matching_counter_delta_requires_parser_mapping"
            || issue == "unnamed_monotonic_counter_candidates_found"
    }) {
        Some("parser_mapping_required".to_string())
    } else {
        None
    };
    metric_record(
        case_id,
        report,
        method,
        "activity",
        "steps",
        "steps",
        source_kind_for_value("device_counter", &local_value),
        bool_field(result, &["pass"]).unwrap_or(false),
        local_value,
        cloned_non_null(result, &["official_whoop_step_delta"]),
        cloned_non_null(result, &["manual_step_delta"]),
        usize_field(result, &["decoded_frame_count"]),
        input_counts(&[
            (
                "decoded_frame_count",
                usize_field(result, &["decoded_frame_count"]),
            ),
            (
                "counter_candidate_count",
                usize_field(result, &["counter_candidate_count"]),
            ),
            (
                "counter_delta_candidate_count",
                usize_field(result, &["counter_delta_candidate_count"]),
            ),
            (
                "monotonic_counter_candidate_count",
                usize_field(result, &["monotonic_counter_candidate_count"]),
            ),
            (
                "matching_counter_delta_count",
                usize_field(result, &["matching_counter_delta_count"]),
            ),
            (
                "selected_counter_delta_rank",
                usize_field(result, &["selected_counter_delta", "rank"]),
            ),
        ]),
        None,
        Some("bull.steps.device_counter.v0".to_string()),
        Some("0.1.0".to_string()),
        str_field(result, &["label_policy"]),
        promotion_status,
        string_array(result, &["issues"]),
        selected_counter_delta_quality_flags(result),
        string_array(result, &["issues"]),
    )
}

fn step_counter_rollup_metric_record(
    case_id: &str,
    report: &str,
    method: Option<&str>,
    result: &Value,
    promotion_status: &str,
) -> LocalHealthValidationMetricRecord {
    let local_value = cloned_non_null(result, &["steps"]);
    metric_record(
        case_id,
        report,
        method,
        "activity",
        "steps",
        "steps",
        source_kind_for_value("device_counter", &local_value),
        bool_field(result, &["pass"]).unwrap_or(false),
        local_value,
        None,
        None,
        usize_field(result, &["sample_count"]),
        input_counts(&[
            ("sample_count", usize_field(result, &["sample_count"])),
            (
                "cadence_sample_count",
                usize_field(result, &["cadence_sample_count"]),
            ),
            (
                "activity_state_sample_count",
                usize_field(result, &["activity_state_sample_count"]),
            ),
            (
                "usable_segment_count",
                usize_field(result, &["usable_segment_count"]),
            ),
        ]),
        f64_field(result, &["confidence"]),
        Some("bull.steps.device_counter.v0".to_string()),
        Some("0.1.0".to_string()),
        None,
        Some(promotion_status.to_string()),
        string_array(result, &["issues"]),
        string_array(result, &["quality_flags"]),
        Vec::new(),
    )
}

fn raw_motion_step_metric_record(
    case_id: &str,
    report: &str,
    method: Option<&str>,
    result: &Value,
) -> LocalHealthValidationMetricRecord {
    let local_value = cloned_non_null(result, &["estimated_steps"]);
    metric_record(
        case_id,
        report,
        method,
        "activity",
        "steps",
        "steps",
        source_kind_for_value(
            &str_field(result, &["source_kind_if_promoted"])
                .unwrap_or_else(|| "local_estimate".to_string()),
            &local_value,
        ),
        bool_field(result, &["pass"]).unwrap_or(false),
        local_value,
        cloned_non_null(result, &["official_whoop_step_delta"]),
        cloned_non_null(result, &["manual_step_delta"]),
        usize_field(result, &["decoded_frame_count"]),
        input_counts(&[
            (
                "decoded_frame_count",
                usize_field(result, &["decoded_frame_count"]),
            ),
            (
                "candidate_frame_count",
                usize_field(result, &["candidate_frame_count"]),
            ),
            (
                "trusted_candidate_frame_count",
                usize_field(result, &["trusted_candidate_frame_count"]),
            ),
        ]),
        f64_field(result, &["confidence"]),
        str_field(result, &["algorithm_id"]),
        str_field(result, &["algorithm_version"]),
        str_field(result, &["label_policy"]),
        str_field(result, &["promotion_status"]),
        string_array(result, &["issues"]),
        string_array(result, &["quality_flags"]),
        Vec::new(),
    )
}

fn energy_validation_metric_records(
    case_id: &str,
    report: &str,
    method: Option<&str>,
    result: &Value,
) -> Vec<LocalHealthValidationMetricRecord> {
    let input_counts = input_counts(&[
        (
            "heart_rate_sample_count",
            usize_field(result, &["heart_rate_sample_count"]),
        ),
        (
            "motion_sample_count",
            usize_field(result, &["motion_sample_count"]),
        ),
    ]);
    let input_packet_count = summed_input_count(&input_counts);
    [
        (
            "active_kcal",
            "kcal",
            "local_active_kcal",
            "official_whoop_active_kcal",
            "active_kcal_within_tolerance",
        ),
        (
            "resting_kcal",
            "kcal",
            "local_resting_kcal",
            "official_whoop_resting_kcal",
            "resting_kcal_within_tolerance",
        ),
        (
            "total_kcal",
            "kcal",
            "local_total_kcal",
            "official_whoop_total_kcal",
            "total_kcal_within_tolerance",
        ),
    ]
    .into_iter()
    .map(
        |(metric_name, unit, local_field, official_field, tolerance_field)| {
            let local_value = cloned_non_null(result, &[local_field]);
            metric_record(
                case_id,
                report,
                method,
                "activity",
                metric_name,
                unit,
                source_kind_for_value("local_estimate", &local_value),
                bool_field(result, &[tolerance_field])
                    .or_else(|| bool_field(result, &["pass"]))
                    .unwrap_or(false),
                local_value,
                cloned_non_null(result, &[official_field]),
                None,
                input_packet_count,
                input_counts.clone(),
                f64_field(result, &["confidence"]),
                str_field(result, &["algorithm_id"]),
                str_field(result, &["algorithm_version"]),
                str_field(result, &["label_policy"]),
                None,
                string_array(result, &["issues"]),
                string_array(result, &["energy_rollup", "quality_flags"]),
                Vec::new(),
            )
        },
    )
    .collect()
}

fn energy_rollup_metric_records(
    case_id: &str,
    report: &str,
    method: Option<&str>,
    result: &Value,
    promotion_status: &str,
) -> Vec<LocalHealthValidationMetricRecord> {
    let input_counts = input_counts(&[
        (
            "heart_rate_sample_count",
            usize_field(result, &["heart_rate_sample_count"]),
        ),
        (
            "motion_sample_count",
            usize_field(result, &["motion_sample_count"]),
        ),
    ]);
    let input_packet_count = summed_input_count(&input_counts);
    let mut record_input_counts = input_counts.clone();
    for (field, value) in [
        (
            "step_metric_count",
            usize_field(result, &["step_metric_count"]),
        ),
        ("step_count", usize_field(result, &["step_count"])),
    ] {
        if let Some(value) = value {
            record_input_counts.insert(field.to_string(), value);
        }
    }
    [
        ("active_kcal", "kcal"),
        ("resting_kcal", "kcal"),
        ("total_kcal", "kcal"),
    ]
    .into_iter()
    .map(|(metric_name, unit)| {
        let local_value = cloned_non_null(result, &[metric_name]);
        metric_record(
            case_id,
            report,
            method,
            "activity",
            metric_name,
            unit,
            source_kind_for_value("local_estimate", &local_value),
            bool_field(result, &["pass"]).unwrap_or(false),
            local_value,
            None,
            None,
            input_packet_count,
            record_input_counts.clone(),
            f64_field(result, &["confidence"]),
            Some("bull.energy.local_estimate.v0".to_string()),
            Some("0.1.0".to_string()),
            None,
            Some(promotion_status.to_string()),
            string_array(result, &["issues"]),
            string_array(result, &["quality_flags"]),
            Vec::new(),
        )
    })
    .collect()
}

fn energy_unavailable_status_metric_records(
    case_id: &str,
    report: &str,
    method: Option<&str>,
    result: &Value,
) -> Vec<LocalHealthValidationMetricRecord> {
    result
        .get("statuses")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|status| {
            let metric_name =
                str_field(status, &["metric_id"]).unwrap_or_else(|| "energy_kcal".to_string());
            metric_record(
                case_id,
                report,
                method,
                "activity",
                &metric_name,
                "kcal",
                "unavailable".to_string(),
                false,
                None,
                None,
                None,
                usize_field(status, &["heart_rate_sample_count"]),
                input_counts(&[
                    (
                        "heart_rate_sample_count",
                        usize_field(status, &["heart_rate_sample_count"]),
                    ),
                    (
                        "motion_sample_count",
                        usize_field(status, &["motion_sample_count"]),
                    ),
                    (
                        "available_metric_count",
                        usize_field(status, &["available_metric_count"]),
                    ),
                ]),
                Some(0.0),
                Some("bull.energy.unavailable_status.v0".to_string()),
                Some("0.1.0".to_string()),
                None,
                str_field(status, &["promotion_status"]),
                Vec::new(),
                string_array(status, &["quality_flags"]),
                string_array(status, &["blocker_reasons"]),
            )
        })
        .collect()
}

fn rhr_validation_metric_record(
    case_id: &str,
    report: &str,
    method: Option<&str>,
    result: &Value,
) -> LocalHealthValidationMetricRecord {
    let local_value = cloned_non_null(result, &["local_resting_hr_bpm"]);
    metric_record(
        case_id,
        report,
        method,
        "recovery",
        "resting_hr",
        "bpm",
        source_kind_for_value("device_sensor", &local_value),
        bool_field(result, &["resting_hr_within_tolerance"])
            .or_else(|| bool_field(result, &["pass"]))
            .unwrap_or(false),
        local_value,
        cloned_non_null(result, &["official_whoop_resting_hr_bpm"]),
        None,
        usize_field(result, &["sample_count"]),
        input_counts(&[
            ("sample_count", usize_field(result, &["sample_count"])),
            (
                "rollup_sample_count",
                usize_field(result, &["resting_hr_rollup", "sample_count"]),
            ),
        ]),
        f64_field(result, &["confidence"]),
        str_field(result, &["algorithm_id"]),
        str_field(result, &["algorithm_version"]),
        str_field(result, &["label_policy"]),
        None,
        string_array(result, &["issues"]),
        string_array(result, &["resting_hr_rollup", "quality_flags"]),
        Vec::new(),
    )
}

fn rhr_rollup_metric_record(
    case_id: &str,
    report: &str,
    method: Option<&str>,
    result: &Value,
) -> LocalHealthValidationMetricRecord {
    let local_value = cloned_non_null(result, &["resting_hr_bpm"]);
    metric_record(
        case_id,
        report,
        method,
        "recovery",
        "resting_hr",
        "bpm",
        source_kind_for_value("device_sensor", &local_value),
        bool_field(result, &["pass"]).unwrap_or(false),
        local_value,
        None,
        None,
        usize_field(result, &["sample_count"]),
        input_counts(&[
            ("sample_count", usize_field(result, &["sample_count"])),
            (
                "feature_sample_count",
                usize_field(result, &["feature_report", "resting", "sample_count"]),
            ),
            (
                "candidate_frame_count",
                usize_field(result, &["feature_report", "candidate_frame_count"]),
            ),
            (
                "trusted_feature_count",
                usize_field(result, &["feature_report", "trusted_feature_count"]),
            ),
            (
                "motion_sample_count",
                usize_field(
                    result,
                    &[
                        "feature_report",
                        "resting",
                        "provenance",
                        "motion_filter",
                        "motion_sample_count",
                    ],
                ),
            ),
            (
                "selected_heart_rate_sample_count",
                usize_field(
                    result,
                    &[
                        "feature_report",
                        "resting",
                        "provenance",
                        "motion_filter",
                        "selected_heart_rate_sample_count",
                    ],
                ),
            ),
            (
                "matched_heart_rate_sample_count",
                usize_field(
                    result,
                    &[
                        "feature_report",
                        "resting",
                        "provenance",
                        "motion_filter",
                        "matched_heart_rate_sample_count",
                    ],
                ),
            ),
            (
                "low_motion_heart_rate_sample_count",
                usize_field(
                    result,
                    &[
                        "feature_report",
                        "resting",
                        "provenance",
                        "motion_filter",
                        "low_motion_heart_rate_sample_count",
                    ],
                ),
            ),
            (
                "high_motion_heart_rate_sample_count",
                usize_field(
                    result,
                    &[
                        "feature_report",
                        "resting",
                        "provenance",
                        "motion_filter",
                        "high_motion_heart_rate_sample_count",
                    ],
                ),
            ),
            (
                "unmatched_heart_rate_sample_count",
                usize_field(
                    result,
                    &[
                        "feature_report",
                        "resting",
                        "provenance",
                        "motion_filter",
                        "unmatched_heart_rate_sample_count",
                    ],
                ),
            ),
        ]),
        f64_field(result, &["confidence"]),
        Some("bull.resting_heart_rate.device_sensor.v0".to_string()),
        Some("0.1.0".to_string()),
        None,
        Some("daily_rollup".to_string()),
        string_array(result, &["issues"]),
        string_array(result, &["quality_flags"]),
        Vec::new(),
    )
}

fn hrv_validation_metric_record(
    case_id: &str,
    report: &str,
    method: Option<&str>,
    result: &Value,
) -> LocalHealthValidationMetricRecord {
    metric_record(
        case_id,
        report,
        method,
        "recovery",
        "hrv_rmssd",
        "ms",
        "unavailable".to_string(),
        bool_field(result, &["hrv_rmssd_within_tolerance"])
            .or_else(|| bool_field(result, &["pass"]))
            .unwrap_or(false),
        cloned_non_null(result, &["local_hrv_rmssd_ms"]),
        cloned_non_null(result, &["official_whoop_hrv_rmssd_ms"]),
        None,
        usize_field(result, &["hrv_report", "candidate_frame_count"]),
        input_counts(&[
            (
                "candidate_frame_count",
                usize_field(result, &["hrv_report", "candidate_frame_count"]),
            ),
            (
                "rr_interval_count",
                usize_field(result, &["rr_interval_count"]),
            ),
            (
                "trusted_rr_interval_count",
                usize_field(result, &["trusted_rr_interval_count"]),
            ),
            (
                "trusted_feature_count",
                usize_field(result, &["trusted_feature_count"]),
            ),
        ]),
        None,
        str_field(result, &["algorithm_id"]),
        str_field(result, &["algorithm_version"]),
        str_field(result, &["label_policy"]),
        str_field(result, &["promotion_status"]),
        string_array(result, &["issues"]),
        string_array(result, &["quality_flags"]),
        Vec::new(),
    )
}

fn respiratory_rate_validation_metric_record(
    case_id: &str,
    report: &str,
    method: Option<&str>,
    result: &Value,
) -> LocalHealthValidationMetricRecord {
    metric_record(
        case_id,
        report,
        method,
        "recovery",
        "respiratory_rate",
        "rpm",
        "unavailable".to_string(),
        bool_field(result, &["respiratory_rate_within_tolerance"])
            .or_else(|| bool_field(result, &["pass"]))
            .unwrap_or(false),
        cloned_non_null(result, &["local_respiratory_rate_rpm"]),
        cloned_non_null(result, &["official_whoop_respiratory_rate_rpm"]),
        None,
        usize_field(result, &["vital_event_report", "data_packet_frame_count"]),
        input_counts(&[
            (
                "data_packet_frame_count",
                usize_field(result, &["vital_event_report", "data_packet_frame_count"]),
            ),
            ("candidate_count", usize_field(result, &["candidate_count"])),
            (
                "trusted_candidate_count",
                usize_field(result, &["trusted_candidate_count"]),
            ),
        ]),
        None,
        str_field(result, &["decoder_id"]),
        str_field(result, &["decoder_version"]),
        str_field(result, &["label_policy"]),
        str_field(result, &["promotion_status"]),
        string_array(result, &["issues"]),
        string_array(result, &["quality_flags"]),
        Vec::new(),
    )
}

fn oxygen_saturation_validation_metric_record(
    case_id: &str,
    report: &str,
    method: Option<&str>,
    result: &Value,
) -> LocalHealthValidationMetricRecord {
    metric_record(
        case_id,
        report,
        method,
        "recovery",
        "oxygen_saturation",
        "percent",
        str_field(result, &["source_kind"]).unwrap_or_else(|| "unavailable".to_string()),
        bool_field(result, &["oxygen_saturation_within_tolerance"])
            .or_else(|| bool_field(result, &["pass"]))
            .unwrap_or(false),
        cloned_non_null(result, &["local_oxygen_saturation_percent"]),
        cloned_non_null(result, &["official_whoop_oxygen_saturation_percent"]),
        None,
        usize_field(result, &["vital_event_report", "data_packet_frame_count"]),
        input_counts(&[
            (
                "data_packet_frame_count",
                usize_field(result, &["vital_event_report", "data_packet_frame_count"]),
            ),
            (
                "pulse_information_packet_count",
                usize_field(result, &["pulse_information_packet_count"]),
            ),
            ("candidate_count", usize_field(result, &["candidate_count"])),
            (
                "trusted_candidate_count",
                usize_field(result, &["trusted_candidate_count"]),
            ),
        ]),
        None,
        str_field(result, &["decoder_id"]),
        str_field(result, &["decoder_version"]),
        str_field(result, &["label_policy"]),
        str_field(result, &["promotion_status"]),
        string_array(result, &["issues"]),
        string_array(result, &["quality_flags"]),
        Vec::new(),
    )
}

fn temperature_validation_metric_record(
    case_id: &str,
    report: &str,
    method: Option<&str>,
    result: &Value,
) -> LocalHealthValidationMetricRecord {
    metric_record(
        case_id,
        report,
        method,
        "recovery",
        "skin_temperature_delta",
        "celsius",
        str_field(result, &["source_kind"]).unwrap_or_else(|| "unavailable".to_string()),
        bool_field(result, &["skin_temperature_within_tolerance"])
            .or_else(|| bool_field(result, &["pass"]))
            .unwrap_or(false),
        cloned_non_null(result, &["local_skin_temperature_delta_c"]),
        cloned_non_null(result, &["official_whoop_skin_temperature_delta_c"]),
        None,
        usize_field(result, &["vital_event_report", "data_packet_frame_count"]),
        input_counts(&[
            (
                "data_packet_frame_count",
                usize_field(result, &["vital_event_report", "data_packet_frame_count"]),
            ),
            (
                "skin_temperature_input_count",
                usize_field(
                    result,
                    &["vital_event_report", "skin_temperature_input_count"],
                ),
            ),
            ("candidate_count", usize_field(result, &["candidate_count"])),
            (
                "trusted_candidate_count",
                usize_field(result, &["trusted_candidate_count"]),
            ),
        ]),
        None,
        str_field(result, &["decoder_id"]),
        str_field(result, &["decoder_version"]),
        str_field(result, &["label_policy"]),
        str_field(result, &["promotion_status"]),
        string_array(result, &["issues"]),
        string_array(result, &["quality_flags"]),
        Vec::new(),
    )
}

fn recovery_sensor_metric_records(
    case_id: &str,
    report: &str,
    method: Option<&str>,
    result: &Value,
) -> Vec<LocalHealthValidationMetricRecord> {
    result
        .get("widgets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|widget| {
            let metric_id = str_field(widget, &["metric_id"]).unwrap_or_else(|| "unknown".into());
            let (metric_family, metric_name, unit) = recovery_widget_metric_parts(&metric_id);
            metric_record(
                case_id,
                report,
                method,
                metric_family,
                metric_name,
                unit,
                str_field(widget, &["source_kind"]).unwrap_or_else(|| "unavailable".into()),
                bool_field(widget, &["user_visible_value_allowed"]).unwrap_or(false),
                None,
                None,
                None,
                usize_field(widget, &["candidate_count"]),
                input_counts(&[
                    ("candidate_count", usize_field(widget, &["candidate_count"])),
                    (
                        "trusted_candidate_count",
                        usize_field(widget, &["trusted_candidate_count"]),
                    ),
                    (
                        "resolved_metric_input_count",
                        usize_field(widget, &["resolved_metric_input_count"]),
                    ),
                    (
                        "value_semantics_verified_count",
                        usize_field(widget, &["value_semantics_verified_count"]),
                    ),
                ]),
                f64_field(widget, &["confidence"]),
                None,
                None,
                None,
                str_field(widget, &["promotion_status"]),
                Vec::new(),
                string_array(widget, &["quality_flags"]),
                string_array(widget, &["blocker_reasons"]),
            )
        })
        .collect()
}

fn recovery_sensor_daily_rollup_metric_records(
    case_id: &str,
    report: &str,
    method: Option<&str>,
    result: &Value,
) -> Vec<LocalHealthValidationMetricRecord> {
    result
        .get("statuses")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|status| {
            let metric_id = str_field(status, &["metric_id"]).unwrap_or_else(|| "unknown".into());
            let (metric_family, metric_name, unit) = recovery_widget_metric_parts(&metric_id);
            let local_value = cloned_non_null(status, &["local_value"]);
            let source_kind = str_field(status, &["source_kind"])
                .unwrap_or_else(|| source_kind_for_value("device_sensor", &local_value));
            let blockers = string_array(status, &["blocker_reasons"]);
            let pass =
                source_kind == "device_sensor" && local_value.is_some() && blockers.is_empty();
            metric_record(
                case_id,
                report,
                method,
                metric_family,
                metric_name,
                str_field(status, &["unit"]).as_deref().unwrap_or(unit),
                source_kind,
                pass,
                local_value,
                None,
                None,
                usize_field(status, &["candidate_count"]),
                input_counts(&[
                    ("candidate_count", usize_field(status, &["candidate_count"])),
                    (
                        "trusted_candidate_count",
                        usize_field(status, &["trusted_candidate_count"]),
                    ),
                    (
                        "resolved_metric_input_count",
                        usize_field(status, &["resolved_metric_input_count"]),
                    ),
                    (
                        "value_semantics_verified_count",
                        usize_field(status, &["value_semantics_verified_count"]),
                    ),
                ]),
                f64_field(status, &["confidence"]),
                Some("bull.recovery_sensor.device_sensor.v0".to_string()),
                Some("0.1.0".to_string()),
                None,
                str_field(status, &["promotion_status"]),
                Vec::new(),
                string_array(status, &["quality_flags"]),
                blockers,
            )
        })
        .collect()
}

fn recovery_unavailable_status_metric_records(
    case_id: &str,
    report: &str,
    method: Option<&str>,
    result: &Value,
) -> Vec<LocalHealthValidationMetricRecord> {
    result
        .get("statuses")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|status| {
            let metric_id = str_field(status, &["metric_id"]).unwrap_or_else(|| "unknown".into());
            let (metric_family, metric_name, unit) = recovery_widget_metric_parts(&metric_id);
            metric_record(
                case_id,
                report,
                method,
                metric_family,
                metric_name,
                unit,
                "unavailable".to_string(),
                false,
                None,
                None,
                None,
                usize_field(status, &["candidate_count"]),
                input_counts(&[
                    ("candidate_count", usize_field(status, &["candidate_count"])),
                    (
                        "trusted_candidate_count",
                        usize_field(status, &["trusted_candidate_count"]),
                    ),
                    (
                        "resolved_metric_input_count",
                        usize_field(status, &["resolved_metric_input_count"]),
                    ),
                    (
                        "value_semantics_verified_count",
                        usize_field(status, &["value_semantics_verified_count"]),
                    ),
                ]),
                Some(0.0),
                Some("bull.recovery.unavailable_status.v0".to_string()),
                Some("0.1.0".to_string()),
                None,
                str_field(status, &["promotion_status"]),
                Vec::new(),
                string_array(status, &["quality_flags"]),
                string_array(status, &["blocker_reasons"]),
            )
        })
        .collect()
}

fn activity_unavailable_status_metric_records(
    case_id: &str,
    report: &str,
    method: Option<&str>,
    result: &Value,
) -> Vec<LocalHealthValidationMetricRecord> {
    result
        .get("statuses")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|status| {
            metric_record(
                case_id,
                report,
                method,
                "activity",
                "steps",
                "steps",
                "unavailable".to_string(),
                false,
                None,
                None,
                None,
                usize_field(status, &["sample_count"]),
                input_counts(&[
                    ("sample_count", usize_field(status, &["sample_count"])),
                    (
                        "usable_segment_count",
                        usize_field(status, &["usable_segment_count"]),
                    ),
                    (
                        "available_metric_count",
                        usize_field(status, &["available_metric_count"]),
                    ),
                ]),
                Some(0.0),
                Some("bull.activity.unavailable_status.v0".to_string()),
                Some("0.1.0".to_string()),
                None,
                str_field(status, &["promotion_status"]),
                Vec::new(),
                string_array(status, &["quality_flags"]),
                string_array(status, &["blocker_reasons"]),
            )
        })
        .collect()
}

fn metric_record(
    case_id: &str,
    report: &str,
    method: Option<&str>,
    metric_family: &str,
    metric_name: &str,
    unit: &str,
    source_kind: String,
    pass: bool,
    local_value: Option<Value>,
    official_label_value: Option<Value>,
    manual_label_value: Option<Value>,
    input_packet_count: Option<usize>,
    input_counts: BTreeMap<String, usize>,
    confidence: Option<f64>,
    algorithm_id: Option<String>,
    algorithm_version: Option<String>,
    label_policy: Option<String>,
    promotion_status: Option<String>,
    issues: Vec<String>,
    quality_flags: Vec<String>,
    blockers: Vec<String>,
) -> LocalHealthValidationMetricRecord {
    LocalHealthValidationMetricRecord {
        case_id: case_id.to_string(),
        report: report.to_string(),
        method: method.map(str::to_string),
        metric_family: metric_family.to_string(),
        metric_name: metric_name.to_string(),
        unit: unit.to_string(),
        source_kind,
        pass,
        local_value,
        official_label_value,
        manual_label_value,
        input_packet_count,
        input_counts,
        confidence,
        algorithm_id,
        algorithm_version,
        label_policy,
        promotion_status,
        issues,
        quality_flags,
        blockers,
    }
}

fn selected_explicit_counter_delta_value(result: &Value) -> Option<Value> {
    let deltas = result.get("counter_deltas")?.as_array()?;
    deltas
        .iter()
        .filter(|delta| str_field(delta, &["match_kind"]).as_deref() == Some("step_count"))
        .find(|delta| bool_field(delta, &["matches_all_provided_labels"]).unwrap_or(false))
        .or_else(|| {
            deltas
                .iter()
                .find(|delta| str_field(delta, &["match_kind"]).as_deref() == Some("step_count"))
        })
        .and_then(|delta| cloned_non_null(delta, &["delta"]))
}

fn recovery_widget_metric_parts(metric_id: &str) -> (&'static str, &'static str, &'static str) {
    match metric_id {
        "hrv_rmssd_ms" => ("recovery", "hrv_rmssd", "ms"),
        "respiratory_rate_rpm" => ("recovery", "respiratory_rate", "rpm"),
        "oxygen_saturation_percent" => ("recovery", "oxygen_saturation", "percent"),
        "skin_temperature_delta_c" => ("recovery", "skin_temperature_delta", "celsius"),
        _ => ("recovery", "unknown", "unknown"),
    }
}

fn source_kind_for_value(source_kind: &str, local_value: &Option<Value>) -> String {
    if local_value.is_some() {
        source_kind.to_string()
    } else {
        "unavailable".to_string()
    }
}

fn input_counts(fields: &[(&str, Option<usize>)]) -> BTreeMap<String, usize> {
    fields
        .iter()
        .filter_map(|(field, value)| value.map(|value| ((*field).to_string(), value)))
        .collect()
}

fn selected_counter_delta_quality_flags(result: &Value) -> Vec<String> {
    let mut flags = Vec::new();
    if let Some(match_kind) = str_field(result, &["selected_counter_delta", "match_kind"]) {
        flags.push(format!(
            "selected_counter_delta_match_kind:{}",
            sanitize_quality_flag_value(&match_kind)
        ));
    }
    if let Some(json_path) = str_field(result, &["selected_counter_delta", "json_path"]) {
        flags.push(format!(
            "selected_counter_delta_json_path:{}",
            sanitize_quality_flag_value(&json_path)
        ));
    }
    if let Some(reason) = str_field(result, &["selected_counter_delta", "selection_reason"]) {
        flags.push(format!(
            "selected_counter_delta_reason:{}",
            sanitize_quality_flag_value(&reason)
        ));
    }
    flags
}

fn sanitize_quality_flag_value(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | '$') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn summed_input_count(input_counts: &BTreeMap<String, usize>) -> Option<usize> {
    if input_counts.is_empty() {
        None
    } else {
        Some(input_counts.values().copied().sum())
    }
}

fn value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    Some(current)
}

fn cloned_non_null(value: &Value, path: &[&str]) -> Option<Value> {
    let value = value_at(value, path)?;
    if value.is_null() {
        None
    } else {
        Some(value.clone())
    }
}

fn str_field(value: &Value, path: &[&str]) -> Option<String> {
    value_at(value, path)?.as_str().map(str::to_string)
}

fn bool_field(value: &Value, path: &[&str]) -> Option<bool> {
    value_at(value, path)?.as_bool()
}

fn usize_field(value: &Value, path: &[&str]) -> Option<usize> {
    value_at(value, path)?
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
}

fn f64_field(value: &Value, path: &[&str]) -> Option<f64> {
    value_at(value, path)?.as_f64()
}

fn string_array(value: &Value, path: &[&str]) -> Vec<String> {
    value_at(value, path)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn validate_case(case: &LocalHealthValidationCase) -> Vec<String> {
    let mut issues = Vec::new();
    let normalized_report = normalized_report(&case.report);
    if case.id.trim().is_empty() {
        issues.push("case_id_required".to_string());
    }
    if case.start.trim().is_empty() {
        issues.push("start_required".to_string());
    }
    if case.end.trim().is_empty() {
        issues.push("end_required".to_string());
    }
    if matches!(
        normalized_report.as_deref(),
        Some("step-rollup")
            | Some("step-hourly-rollup")
            | Some("activity-unavailable-status")
            | Some("energy-validation")
            | Some("energy-rollup")
            | Some("energy-unavailable-status")
            | Some("energy-hourly-rollup")
            | Some("recovery-sensor-rollup")
            | Some("recovery-unavailable-status")
            | Some("rhr-validation")
            | Some("rhr-rollup")
    ) {
        let issue_suffix = validation_date_issue_suffix(&case.report);
        if case.date_key.as_deref().unwrap_or("").trim().is_empty() {
            issues.push(format!("date_key_required_for_{issue_suffix}"));
        }
        if case.timezone.as_deref().unwrap_or("").trim().is_empty() {
            issues.push(format!("timezone_required_for_{issue_suffix}"));
        }
    }
    if matches!(
        normalized_report.as_deref(),
        Some("step-rollup") | Some("step-hourly-rollup") | Some("activity-unavailable-status")
    ) && parse_rfc3339_utc_unix_ms(&case.start)
        .zip(parse_rfc3339_utc_unix_ms(&case.end))
        .is_none_or(|(start, end)| end <= start)
    {
        issues.push("unix_window_required_for_step_rollup".to_string());
    }
    if requires_capture_session_binding_for_acceptance(normalized_report.as_deref())
        && expected_capture_session_ids(case).is_empty()
    {
        issues.push("capture_session_required_for_acceptance".to_string());
    }
    if case.write_metric && forbids_metric_writes_in_validation_suite(normalized_report.as_deref())
    {
        issues.push("write_metric_not_allowed_for_validation_report".to_string());
    }
    if case.write_metric
        && requires_capture_session_binding_for_metric_write(normalized_report.as_deref())
        && expected_capture_session_ids(case).is_empty()
    {
        issues.push("capture_session_required_for_metric_write".to_string());
    }
    if has_official_labels(case) && !label_policy_valid(case) {
        issues.push("official_label_policy_not_marked".to_string());
    }
    issues
}

fn case_method(report: &str) -> Option<&'static str> {
    match normalized_report(report).as_deref()? {
        "step-discovery" => Some("metrics.step_packet_discovery"),
        "step-validation" => Some("metrics.step_capture_validation"),
        "raw-motion-steps" => Some("metrics.raw_motion_step_estimate"),
        "step-rollup" => Some("metrics.step_counter_daily_rollup"),
        "step-hourly-rollup" => Some("metrics.step_counter_hourly_rollup"),
        "activity-unavailable-status" => Some("metrics.activity_unavailable_daily_status"),
        "energy-validation" => Some("metrics.energy_capture_validation"),
        "energy-rollup" => Some("metrics.energy_daily_rollup"),
        "energy-unavailable-status" => Some("metrics.energy_unavailable_daily_status"),
        "energy-hourly-rollup" => Some("metrics.energy_hourly_rollup"),
        "rhr-rollup" => Some("metrics.resting_hr_daily_rollup"),
        "rhr-validation" => Some("metrics.resting_hr_capture_validation"),
        "hrv-validation" => Some("metrics.hrv_capture_validation"),
        "respiratory-rate-validation" => Some("metrics.respiratory_rate_capture_validation"),
        "oxygen-saturation-validation" => Some("metrics.oxygen_saturation_capture_validation"),
        "temperature-validation" => Some("metrics.temperature_capture_validation"),
        "recovery-sensors" => Some("metrics.recovery_sensor_discovery"),
        "recovery-sensor-rollup" => Some("metrics.recovery_sensor_daily_rollup"),
        "recovery-unavailable-status" => Some("metrics.recovery_unavailable_daily_status"),
        _ => None,
    }
}

fn normalized_report(report: &str) -> Option<String> {
    let normalized = report.trim().replace('_', "-");
    if normalized.is_empty() {
        return None;
    }
    Some(
        match normalized.as_str() {
            "step-discovery"
            | "steps-discovery"
            | "step-packet-discovery"
            | "steps-packet-discovery"
            | "pedometer-discovery" => "step-discovery",
            "step" | "steps" | "step-capture-validation" => "step-validation",
            "raw-motion-step"
            | "raw-motion-steps"
            | "motion-step-estimate"
            | "raw-motion-step-estimate" => "raw-motion-steps",
            "step-rollup"
            | "steps-rollup"
            | "step-counter-daily-rollup"
            | "step-daily-rollup"
            | "daily-step-rollup"
            | "daily-steps-rollup" => "step-rollup",
            "step-hourly-rollup"
            | "steps-hourly-rollup"
            | "step-counter-hourly-rollup"
            | "hourly-step-rollup"
            | "hourly-steps-rollup" => "step-hourly-rollup",
            "activity-unavailable"
            | "activity-unavailable-status"
            | "activity-unavailable-daily-status"
            | "step-unavailable"
            | "step-unavailable-status"
            | "steps-unavailable"
            | "steps-unavailable-status" => "activity-unavailable-status",
            "energy-rollup"
            | "energy-daily-rollup"
            | "daily-energy"
            | "daily-energy-rollup"
            | "daily-calories"
            | "daily-calorie-rollup"
            | "daily-calories-rollup"
            | "calorie-rollup" => "energy-rollup",
            "energy" | "calorie" | "calories" | "energy-capture-validation" => "energy-validation",
            "energy-unavailable"
            | "energy-unavailable-status"
            | "energy-unavailable-daily-status"
            | "calorie-unavailable"
            | "calorie-unavailable-status"
            | "calories-unavailable"
            | "calories-unavailable-status" => "energy-unavailable-status",
            "energy-hourly"
            | "energy-hourly-rollup"
            | "hourly-energy"
            | "hourly-energy-rollup"
            | "hourly-calories"
            | "hourly-calorie-rollup"
            | "energy-hourly-rollup-report" => "energy-hourly-rollup",
            "rhr-rollup"
            | "resting-hr-rollup"
            | "resting-heart-rate-rollup"
            | "resting-hr-daily-rollup"
            | "resting-heart-rate-daily-rollup" => "rhr-rollup",
            "rhr"
            | "rhr-validation"
            | "resting-hr"
            | "resting-heart-rate"
            | "resting-hr-validation"
            | "resting-heart-rate-validation"
            | "resting-hr-capture-validation" => "rhr-validation",
            "hrv-validation" | "hrv-capture-validation" => "hrv-validation",
            "respiratory-rate"
            | "respiratory-rate-validation"
            | "respiratory-rate-capture-validation"
            | "rr-validation" => "respiratory-rate-validation",
            "oxygen-saturation"
            | "oxygen-saturation-validation"
            | "oxygen-saturation-capture-validation"
            | "spo2"
            | "spo2-validation"
            | "spo2-capture-validation" => "oxygen-saturation-validation",
            "temperature"
            | "temperature-validation"
            | "temperature-capture-validation"
            | "skin-temperature"
            | "skin-temperature-validation"
            | "skin-temperature-capture-validation"
            | "temp-validation" => "temperature-validation",
            "recovery-sensor" | "recovery-sensors" | "health-sensors" => "recovery-sensors",
            "recovery-sensor-rollup"
            | "recovery-sensors-rollup"
            | "recovery-sensor-daily-rollup"
            | "recovery-sensors-daily-rollup"
            | "recovery-vitals-rollup"
            | "recovery-vitals-daily-rollup" => "recovery-sensor-rollup",
            "recovery-unavailable"
            | "recovery-unavailable-status"
            | "recovery-unavailable-daily-status"
            | "recovery-widget-status" => "recovery-unavailable-status",
            other => other,
        }
        .to_string(),
    )
}

fn validation_date_issue_suffix(report: &str) -> &'static str {
    match normalized_report(report).as_deref() {
        Some("step-rollup") => "step_rollup",
        Some("step-hourly-rollup") => "step_hourly_rollup",
        Some("activity-unavailable-status") => "activity_unavailable_status",
        Some("energy-rollup") => "energy_rollup",
        Some("energy-unavailable-status") => "energy_unavailable_status",
        Some("energy-hourly-rollup") => "energy_hourly_rollup",
        Some("recovery-sensor-rollup") => "recovery_sensor_rollup",
        Some("recovery-unavailable-status") => "recovery_unavailable_status",
        Some("rhr-rollup") => "rhr_rollup",
        Some("rhr-validation") => "rhr_validation",
        _ => "energy_validation",
    }
}

fn case_args(
    database_path: &str,
    case: &LocalHealthValidationCase,
    capture_session_evidence: &CaptureSessionEvidenceReadiness,
) -> Value {
    let mut object = Map::new();
    object.insert("database_path".to_string(), json!(database_path));
    object.insert("start".to_string(), json!(case.start.as_str()));
    object.insert("end".to_string(), json!(case.end.as_str()));
    if matches!(
        normalized_report(&case.report).as_deref(),
        Some("step-rollup") | Some("step-hourly-rollup") | Some("activity-unavailable-status")
    ) {
        if let Some(start_time_unix_ms) = parse_rfc3339_utc_unix_ms(&case.start) {
            object.insert("start_time_unix_ms".to_string(), json!(start_time_unix_ms));
        }
        if let Some(end_time_unix_ms) = parse_rfc3339_utc_unix_ms(&case.end) {
            object.insert("end_time_unix_ms".to_string(), json!(end_time_unix_ms));
        }
    }
    insert_string(&mut object, "date_key", case.date_key.as_deref());
    insert_string(&mut object, "timezone", case.timezone.as_deref());
    insert_string(&mut object, "capture_kind", case.capture_kind.as_deref());
    insert_usize(&mut object, "min_owned_captures", case.min_owned_captures);
    insert_bool(
        &mut object,
        "require_trusted_evidence",
        case.require_trusted_evidence,
    );
    insert_usize(
        &mut object,
        "max_candidate_fields",
        case.max_candidate_fields,
    );
    insert_i64(&mut object, "manual_step_delta", case.manual_step_delta);
    insert_i64(
        &mut object,
        "official_whoop_step_delta",
        case.official_whoop_step_delta,
    );
    insert_i64(&mut object, "tolerance_steps", case.step_delta_tolerance);
    insert_f64(&mut object, "sample_rate_hz", case.sample_rate_hz);
    insert_f64(&mut object, "peak_threshold_i16", case.peak_threshold_i16);
    insert_usize(
        &mut object,
        "min_peak_spacing_samples",
        case.min_peak_spacing_samples,
    );
    insert_f64(&mut object, "profile_weight_kg", case.profile_weight_kg);
    insert_u32(&mut object, "profile_age_years", case.profile_age_years);
    insert_string(&mut object, "profile_sex", case.profile_sex.as_deref());
    insert_f64(&mut object, "resting_hr_bpm", case.resting_hr_bpm);
    insert_f64(&mut object, "max_hr_bpm", case.max_hr_bpm);
    insert_usize(
        &mut object,
        "min_heart_rate_samples",
        case.min_heart_rate_samples,
    );
    insert_usize(&mut object, "min_sample_count", case.min_sample_count);
    insert_f64(
        &mut object,
        "official_whoop_active_kcal",
        case.official_whoop_active_kcal,
    );
    insert_f64(
        &mut object,
        "official_whoop_resting_kcal",
        case.official_whoop_resting_kcal,
    );
    insert_f64(
        &mut object,
        "official_whoop_total_kcal",
        case.official_whoop_total_kcal,
    );
    insert_f64(&mut object, "tolerance_kcal", case.energy_tolerance_kcal);
    insert_f64(
        &mut object,
        "relative_tolerance_fraction",
        case.energy_relative_tolerance,
    );
    insert_f64(
        &mut object,
        "official_whoop_resting_hr_bpm",
        case.official_whoop_resting_hr_bpm,
    );
    insert_f64(&mut object, "tolerance_bpm", case.rhr_tolerance_bpm);
    insert_f64(
        &mut object,
        "official_whoop_hrv_rmssd_ms",
        case.official_whoop_hrv_rmssd_ms,
    );
    insert_f64(&mut object, "tolerance_ms", case.hrv_tolerance_ms);
    insert_f64(
        &mut object,
        "official_whoop_respiratory_rate_rpm",
        case.official_whoop_respiratory_rate_rpm,
    );
    insert_f64(
        &mut object,
        "tolerance_rpm",
        case.respiratory_rate_tolerance_rpm,
    );
    insert_f64(
        &mut object,
        "official_whoop_oxygen_saturation_percent",
        case.official_whoop_oxygen_saturation_percent,
    );
    insert_f64(
        &mut object,
        "tolerance_percent",
        case.oxygen_saturation_tolerance_percent,
    );
    insert_f64(
        &mut object,
        "official_whoop_skin_temperature_delta_c",
        case.official_whoop_skin_temperature_delta_c,
    );
    insert_f64(&mut object, "tolerance_c", case.temperature_tolerance_c);
    insert_usize(
        &mut object,
        "min_rr_intervals_to_compute",
        case.min_rr_intervals_to_compute,
    );
    insert_bool(
        &mut object,
        "write_metric",
        should_forward_write_metric(case, capture_session_evidence),
    );
    if let Some(label_provenance) = &case.label_provenance {
        object.insert("label_provenance".to_string(), label_provenance.clone());
    }
    Value::Object(object)
}

fn insert_string(object: &mut Map<String, Value>, field: &str, value: Option<&str>) {
    if let Some(value) = value {
        object.insert(field.to_string(), json!(value));
    }
}

fn insert_bool(object: &mut Map<String, Value>, field: &str, value: bool) {
    if value {
        object.insert(field.to_string(), json!(true));
    }
}

fn insert_usize(object: &mut Map<String, Value>, field: &str, value: Option<usize>) {
    if let Some(value) = value {
        object.insert(field.to_string(), json!(value));
    }
}

fn insert_i64(object: &mut Map<String, Value>, field: &str, value: Option<i64>) {
    if let Some(value) = value {
        object.insert(field.to_string(), json!(value));
    }
}

fn insert_u32(object: &mut Map<String, Value>, field: &str, value: Option<u32>) {
    if let Some(value) = value {
        object.insert(field.to_string(), json!(value));
    }
}

fn insert_f64(object: &mut Map<String, Value>, field: &str, value: Option<f64>) {
    if let Some(value) = value {
        object.insert(field.to_string(), json!(value));
    }
}

fn parse_rfc3339_utc_unix_ms(value: &str) -> Option<i64> {
    let value = value.trim();
    let date_time = value.strip_suffix('Z')?;
    let (date, time) = date_time.split_once('T')?;
    let mut date_parts = date.split('-');
    let year = date_parts.next()?.parse::<i32>().ok()?;
    let month = date_parts.next()?.parse::<u32>().ok()?;
    let day = date_parts.next()?.parse::<u32>().ok()?;
    if date_parts.next().is_some() {
        return None;
    }

    let (time_main, fraction) = time.split_once('.').unwrap_or((time, ""));
    let mut time_parts = time_main.split(':');
    let hour = time_parts.next()?.parse::<u32>().ok()?;
    let minute = time_parts.next()?.parse::<u32>().ok()?;
    let second = time_parts.next()?.parse::<u32>().ok()?;
    if time_parts.next().is_some()
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }

    let millis = if fraction.is_empty() {
        0
    } else {
        let digits = fraction
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .take(3)
            .collect::<String>();
        if digits.is_empty() {
            0
        } else {
            format!("{digits:0<3}").parse::<i64>().ok()?
        }
    };

    let days = days_from_civil(year, month, day);
    let seconds = days * 86_400
        + i64::from(hour) * 3_600
        + i64::from(minute) * 60
        + i64::from(second.min(59));
    Some(seconds * 1_000 + millis)
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - i32::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = month as i32;
    let day = day as i32;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    i64::from(era) * 146_097 + i64::from(doe) - 719_468
}

fn has_official_labels(case: &LocalHealthValidationCase) -> bool {
    case.official_whoop_step_delta.is_some()
        || case.official_whoop_active_kcal.is_some()
        || case.official_whoop_resting_kcal.is_some()
        || case.official_whoop_total_kcal.is_some()
        || case.official_whoop_resting_hr_bpm.is_some()
        || case.official_whoop_hrv_rmssd_ms.is_some()
        || case.official_whoop_respiratory_rate_rpm.is_some()
        || case.official_whoop_oxygen_saturation_percent.is_some()
        || case.official_whoop_skin_temperature_delta_c.is_some()
}

fn label_policy_valid(case: &LocalHealthValidationCase) -> bool {
    if !has_official_labels(case) {
        return true;
    }
    case.label_provenance
        .as_ref()
        .and_then(|value| value.get("official_labels_are_labels"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn capture_session_evidence_for_case(
    database_path: &str,
    case: &LocalHealthValidationCase,
) -> CaptureSessionEvidenceReadiness {
    let expected_capture_session_ids = expected_capture_session_ids(case);
    if expected_capture_session_ids.is_empty() {
        return CaptureSessionEvidenceReadiness {
            status: "not_declared".to_string(),
            expected_capture_session_ids,
            raw_evidence_count: 0,
            decoded_frame_count: 0,
            raw_evidence_time_bounds: None,
            decoded_frame_time_bounds: None,
            packet_family_counts: BTreeMap::new(),
            missing_capture_session_ids: Vec::new(),
            issues: Vec::new(),
        };
    }

    let store = match BullStore::open(Path::new(database_path)) {
        Ok(store) => store,
        Err(error) => {
            return CaptureSessionEvidenceReadiness {
                status: "database_error".to_string(),
                expected_capture_session_ids: expected_capture_session_ids.clone(),
                raw_evidence_count: 0,
                decoded_frame_count: 0,
                raw_evidence_time_bounds: None,
                decoded_frame_time_bounds: None,
                packet_family_counts: BTreeMap::new(),
                missing_capture_session_ids: expected_capture_session_ids,
                issues: vec![format!("capture_session_evidence_database_error:{error}")],
            };
        }
    };

    let expected = expected_capture_session_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let raw_rows = match store.raw_evidence_between(&case.start, &case.end) {
        Ok(rows) => rows,
        Err(error) => {
            return CaptureSessionEvidenceReadiness {
                status: "database_error".to_string(),
                expected_capture_session_ids: expected_capture_session_ids.clone(),
                raw_evidence_count: 0,
                decoded_frame_count: 0,
                raw_evidence_time_bounds: None,
                decoded_frame_time_bounds: None,
                packet_family_counts: BTreeMap::new(),
                missing_capture_session_ids: expected_capture_session_ids,
                issues: vec![format!("capture_session_evidence_database_error:{error}")],
            };
        }
    };
    let decoded_rows = match store.decoded_frames_between(&case.start, &case.end) {
        Ok(rows) => rows,
        Err(error) => {
            return CaptureSessionEvidenceReadiness {
                status: "database_error".to_string(),
                expected_capture_session_ids: expected_capture_session_ids.clone(),
                raw_evidence_count: 0,
                decoded_frame_count: 0,
                raw_evidence_time_bounds: None,
                decoded_frame_time_bounds: None,
                packet_family_counts: BTreeMap::new(),
                missing_capture_session_ids: expected_capture_session_ids,
                issues: vec![format!("capture_session_evidence_database_error:{error}")],
            };
        }
    };

    let matching_raw_rows = raw_rows
        .iter()
        .filter(|row| {
            row.capture_session_id
                .as_ref()
                .is_some_and(|session_id| expected.contains(session_id))
        })
        .collect::<Vec<_>>();
    let raw_evidence_ids = matching_raw_rows
        .iter()
        .map(|row| row.evidence_id.as_str())
        .collect::<BTreeSet<_>>();
    let matching_decoded_rows = decoded_rows
        .iter()
        .filter(|row| raw_evidence_ids.contains(row.evidence_id.as_str()))
        .collect::<Vec<_>>();
    let raw_evidence_time_bounds = evidence_time_bounds_from_timestamps(
        matching_raw_rows.iter().map(|row| row.captured_at.as_str()),
        &case.start,
        &case.end,
    );
    let decoded_frame_time_bounds = evidence_time_bounds_from_timestamps(
        matching_decoded_rows
            .iter()
            .map(|row| row.captured_at.as_str()),
        &case.start,
        &case.end,
    );
    let mut packet_family_counts = BTreeMap::new();
    for row in &matching_decoded_rows {
        let family = decoded_packet_family(
            row.packet_type_name.as_deref(),
            row.parsed_payload_json.as_str(),
        );
        *packet_family_counts.entry(family).or_insert(0) += 1;
    }
    let observed_session_ids = matching_raw_rows
        .iter()
        .filter_map(|row| row.capture_session_id.clone())
        .collect::<BTreeSet<_>>();
    let missing_capture_session_ids = expected_capture_session_ids
        .iter()
        .filter(|session_id| !observed_session_ids.contains(*session_id))
        .cloned()
        .collect::<Vec<_>>();
    let status = if missing_capture_session_ids.is_empty() {
        "declared_with_evidence"
    } else if observed_session_ids.is_empty() {
        "declared_missing_evidence"
    } else {
        "declared_partial_evidence"
    }
    .to_string();
    let issues = if missing_capture_session_ids.is_empty() {
        Vec::new()
    } else {
        vec!["capture_session_evidence_missing".to_string()]
    };

    CaptureSessionEvidenceReadiness {
        status,
        expected_capture_session_ids,
        raw_evidence_count: matching_raw_rows.len(),
        decoded_frame_count: matching_decoded_rows.len(),
        raw_evidence_time_bounds,
        decoded_frame_time_bounds,
        packet_family_counts,
        missing_capture_session_ids,
        issues,
    }
}

fn expected_capture_session_ids(case: &LocalHealthValidationCase) -> Vec<String> {
    let mut ids = BTreeSet::new();
    if let Some(id) = case.capture_session_id.as_deref() {
        let trimmed = id.trim();
        if !trimmed.is_empty() {
            ids.insert(trimmed.to_string());
        }
    }
    for id in &case.capture_session_ids {
        let trimmed = id.trim();
        if !trimmed.is_empty() {
            ids.insert(trimmed.to_string());
        }
    }
    ids.into_iter().collect()
}

fn scoped_database_path_for_case(
    database_path: &str,
    case: &LocalHealthValidationCase,
    evidence: &CaptureSessionEvidenceReadiness,
) -> bull_core::BullResult<Option<PathBuf>> {
    if evidence.expected_capture_session_ids.is_empty() {
        return Ok(None);
    }

    let scoped_path = unique_validation_temp_path(&case.id);
    let _ = fs::remove_file(&scoped_path);
    {
        let _ = BullStore::open(&scoped_path)?;
    }
    copy_capture_session_scope(
        database_path,
        &scoped_path,
        case,
        &evidence.expected_capture_session_ids,
    )?;
    Ok(Some(scoped_path))
}

fn copy_capture_session_scope(
    source_database_path: &str,
    scoped_path: &Path,
    case: &LocalHealthValidationCase,
    expected_capture_session_ids: &[String],
) -> bull_core::BullResult<()> {
    let connection = Connection::open(scoped_path).map_err(|error| {
        BullError::message(format!("cannot open scoped validation database: {error}"))
    })?;
    let source_database_literal = sql_string_literal(source_database_path);
    let session_id_list = expected_capture_session_ids
        .iter()
        .map(|session_id| sql_string_literal(session_id))
        .collect::<Vec<_>>()
        .join(", ");
    let start_literal = sql_string_literal(&case.start);
    let end_literal = sql_string_literal(&case.end);
    connection
        .execute_batch(&format!(
            "ATTACH DATABASE {source_database_literal} AS source_db;"
        ))
        .map_err(|error| BullError::message(format!("cannot attach source database: {error}")))?;

    let copy_result = (|| -> bull_core::BullResult<()> {
        connection
            .execute_batch(&format!(
                r#"
                INSERT OR IGNORE INTO capture_sessions
                SELECT *
                FROM source_db.capture_sessions
                WHERE session_id IN ({session_id_list});

                INSERT OR IGNORE INTO raw_evidence
                SELECT *
                FROM source_db.raw_evidence
                WHERE captured_at >= {start_literal}
                  AND captured_at < {end_literal}
                  AND capture_session_id IN ({session_id_list});

                INSERT OR IGNORE INTO decoded_frames
                SELECT decoded_frames.*
                FROM source_db.decoded_frames AS decoded_frames
                INNER JOIN source_db.raw_evidence AS raw_evidence
                    ON raw_evidence.evidence_id = decoded_frames.evidence_id
                WHERE raw_evidence.captured_at >= {start_literal}
                  AND raw_evidence.captured_at < {end_literal}
                  AND raw_evidence.capture_session_id IN ({session_id_list});
                "#
            ))
            .map_err(|error| {
                BullError::message(format!("cannot copy scoped packet evidence: {error}"))
            })?;

        if source_table_exists(&connection, "step_counter_samples")?
            && let Some((start_unix_ms, end_unix_ms)) =
                parse_rfc3339_utc_unix_ms(&case.start).zip(parse_rfc3339_utc_unix_ms(&case.end))
        {
            connection
                .execute_batch(&format!(
                    r#"
                    INSERT OR IGNORE INTO step_counter_samples
                    SELECT *
                    FROM source_db.step_counter_samples
                    WHERE sample_time_unix_ms >= {start_unix_ms}
                      AND sample_time_unix_ms < {end_unix_ms}
                      AND capture_session_id IN ({session_id_list});
                    "#
                ))
                .map_err(|error| {
                    BullError::message(format!("cannot copy scoped step-counter samples: {error}"))
                })?;
        }
        Ok(())
    })();

    let detach_result = connection
        .execute_batch("DETACH DATABASE source_db;")
        .map_err(|error| BullError::message(format!("cannot detach source database: {error}")));
    copy_result.and(detach_result)
}

fn source_table_exists(connection: &Connection, table_name: &str) -> bull_core::BullResult<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM source_db.sqlite_master WHERE type='table' AND name=?1)",
            [table_name],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value != 0)
        .map_err(|error| BullError::message(format!("cannot inspect source tables: {error}")))
}

fn persist_scoped_formatted_metric_writes(
    source_database_path: &str,
    scoped_path: &Path,
) -> bull_core::BullResult<usize> {
    let connection = Connection::open(source_database_path).map_err(|error| {
        BullError::message(format!(
            "cannot open validation database for metric copy: {error}"
        ))
    })?;
    let scoped_database_literal = sql_string_literal(&scoped_path.display().to_string());
    connection
        .execute_batch(&format!(
            "ATTACH DATABASE {scoped_database_literal} AS scoped_db;"
        ))
        .map_err(|error| BullError::message(format!("cannot attach scoped database: {error}")))?;

    let copy_result = (|| -> bull_core::BullResult<usize> {
        let mut changed = 0usize;
        for table in [
            "daily_activity_metrics",
            "hourly_activity_metrics",
            "daily_recovery_metrics",
            "metric_provenance",
        ] {
            changed += connection
                .execute(
                    &format!("INSERT OR REPLACE INTO {table} SELECT * FROM scoped_db.{table};"),
                    [],
                )
                .map_err(|error| {
                    BullError::message(format!("cannot copy scoped {table}: {error}"))
                })?;
        }
        Ok(changed)
    })();

    let detach_result = connection
        .execute_batch("DETACH DATABASE scoped_db;")
        .map_err(|error| BullError::message(format!("cannot detach scoped database: {error}")));
    let changed = copy_result?;
    detach_result?;
    Ok(changed)
}

fn unique_validation_temp_path(token: &str) -> PathBuf {
    let timestamp_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    env::temp_dir().join(format!(
        "bull-local-health-validation-{}-{}-{}.sqlite",
        process::id(),
        sanitize_path_token(token),
        timestamp_nanos
    ))
}

fn sanitize_path_token(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    sanitized.trim_matches('-').chars().take(80).collect()
}

fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn readiness_summary_for_cases(
    cases: &[LocalHealthValidationCaseReport],
) -> LocalHealthValidationReadinessSummary {
    LocalHealthValidationReadinessSummary {
        case_count: cases.len(),
        acceptance_ready_case_count: cases
            .iter()
            .filter(|case| case.readiness.acceptance_ready)
            .count(),
        capture_acceptance_ready_case_count: cases
            .iter()
            .filter(|case| case.readiness.capture_acceptance_ready)
            .count(),
        missing_packet_evidence_case_count: cases
            .iter()
            .filter(|case| case.readiness.evidence_status == "missing_packet_evidence")
            .count(),
        missing_or_invalid_official_label_case_count: cases
            .iter()
            .filter(|case| {
                matches!(
                    case.readiness.official_label_status.as_str(),
                    "official_labels_missing" | "official_label_policy_invalid"
                )
            })
            .count(),
        manual_label_missing_case_count: cases
            .iter()
            .filter(|case| case.readiness.manual_label_status == "manual_label_missing")
            .count(),
        unavailable_status_case_count: cases
            .iter()
            .filter(|case| case.readiness.evidence_status == "unavailable_status_recorded")
            .count(),
        capture_session_declared_case_count: cases
            .iter()
            .filter(|case| case.readiness.capture_session_status != "not_declared")
            .count(),
        capture_session_required_case_count: cases
            .iter()
            .filter(|case| {
                case.readiness.blockers.iter().any(|blocker| {
                    blocker == "capture_session_required_for_acceptance"
                        || blocker == "capture_session_required_for_capture_acceptance"
                })
            })
            .count(),
        capture_session_missing_evidence_case_count: cases
            .iter()
            .filter(|case| {
                matches!(
                    case.readiness.capture_session_status.as_str(),
                    "declared_missing_evidence" | "declared_partial_evidence" | "database_error"
                )
            })
            .count(),
        capture_session_sparse_evidence_case_count: cases
            .iter()
            .filter(|case| {
                case.readiness.blockers.iter().any(|blocker| {
                    blocker == "capture_session_decoded_evidence_too_sparse_for_capture_acceptance"
                })
            })
            .count(),
        capture_session_unrelated_packet_family_case_count: cases
            .iter()
            .filter(|case| {
                case.readiness.blockers.iter().any(|blocker| {
                    blocker == "capture_session_packet_family_unrelated_for_capture_acceptance"
                })
            })
            .count(),
    }
}

fn readiness_for_case(
    case: &LocalHealthValidationCase,
    method: Option<&str>,
    ok: bool,
    pass: bool,
    issues: &[String],
    metric_records: &[LocalHealthValidationMetricRecord],
    result: Option<&Value>,
    capture_session_evidence: &CaptureSessionEvidenceReadiness,
) -> LocalHealthValidationCaseReadiness {
    let normalized_report = normalized_report(&case.report);
    let unavailable_status_case = matches!(
        normalized_report.as_deref(),
        Some("activity-unavailable-status")
            | Some("energy-unavailable-status")
            | Some("recovery-unavailable-status")
    );
    let official_label_status = official_label_status(case, normalized_report.as_deref());
    let manual_label_status = manual_label_status(case, normalized_report.as_deref());

    let mut source_kinds = BTreeSet::new();
    let mut promotion_statuses = BTreeSet::new();
    let mut input_counts = BTreeMap::new();
    let mut blockers = BTreeSet::new();
    let mut has_non_unavailable_value = false;
    let mut has_any_input_evidence = false;
    let mut has_any_local_value = false;

    for record in metric_records {
        source_kinds.insert(record.source_kind.clone());
        if let Some(promotion_status) = &record.promotion_status {
            promotion_statuses.insert(promotion_status.clone());
        }
        if let Some(local_value) = &record.local_value {
            if !local_value.is_null() {
                has_any_local_value = true;
                if record.source_kind != "unavailable" {
                    has_non_unavailable_value = true;
                }
            }
        }
        if let Some(input_packet_count) = record.input_packet_count {
            if input_packet_count > 0 {
                has_any_input_evidence = true;
            }
            *input_counts
                .entry("input_packet_count".to_string())
                .or_insert(0) += input_packet_count;
        }
        for (key, value) in &record.input_counts {
            if *value > 0 {
                has_any_input_evidence = true;
            }
            *input_counts.entry(key.clone()).or_insert(0) += *value;
        }
        blockers.extend(record.blockers.iter().cloned());
        blockers.extend(record.issues.iter().cloned());
    }

    if let Some(result) = result {
        blockers.extend(string_array(result, &["issues"]));
        if result
            .get("next_actions")
            .and_then(Value::as_array)
            .is_some_and(|actions| !actions.is_empty())
            && !pass
        {
            blockers.insert("metric_report_next_actions_present".to_string());
        }
    }

    let evidence_status = if method.is_none() || !ok {
        "bridge_error"
    } else if unavailable_status_case && pass {
        "unavailable_status_recorded"
    } else if pass && has_non_unavailable_value {
        "ready"
    } else if has_any_input_evidence || has_any_local_value {
        "candidate_evidence_present"
    } else {
        "missing_packet_evidence"
    }
    .to_string();

    let mut missing = BTreeSet::new();
    if evidence_status == "missing_packet_evidence" {
        missing.insert("packet_evidence".to_string());
    }
    if official_label_status == "official_labels_missing" {
        missing.insert("official_labels".to_string());
    }
    if official_label_status == "official_label_policy_invalid" {
        missing.insert("official_label_policy".to_string());
    }
    if manual_label_status == "manual_label_missing" {
        missing.insert("manual_labels".to_string());
    }
    if !ok {
        missing.insert("bridge_result".to_string());
    }
    if matches!(
        capture_session_evidence.status.as_str(),
        "declared_missing_evidence" | "declared_partial_evidence" | "database_error"
    ) {
        missing.insert("capture_session_evidence".to_string());
    }
    let capture_session_binding_required =
        requires_capture_session_binding_for_acceptance(normalized_report.as_deref());
    if capture_session_binding_required && capture_session_evidence.status == "not_declared" {
        missing.insert("capture_session_id".to_string());
        blockers.insert("capture_session_required_for_acceptance".to_string());
    }
    let capture_acceptance_eligible = counts_for_capture_acceptance(normalized_report.as_deref());
    if capture_acceptance_eligible && capture_session_evidence.status == "not_declared" {
        missing.insert("capture_session_id".to_string());
        blockers.insert("capture_session_required_for_capture_acceptance".to_string());
    }
    let capture_session_packet_span_sufficient =
        capture_session_packet_span_sufficient_for_capture_acceptance(capture_session_evidence);
    if capture_acceptance_eligible
        && capture_session_evidence.status == "declared_with_evidence"
        && !capture_session_packet_span_sufficient
    {
        missing.insert("capture_session_packet_span".to_string());
        blockers.insert(
            "capture_session_decoded_evidence_too_sparse_for_capture_acceptance".to_string(),
        );
    }
    let capture_acceptance_required_packet_family_prefixes =
        capture_acceptance_required_packet_family_prefixes_for_normalized_report(
            normalized_report.as_deref(),
        );
    let capture_session_relevant_packet_family_counts =
        relevant_packet_family_counts_for_capture_acceptance(
            &capture_acceptance_required_packet_family_prefixes,
            &capture_session_evidence.packet_family_counts,
        );
    let capture_session_packet_family_relevant = capture_acceptance_required_packet_family_prefixes
        .is_empty()
        || !capture_session_relevant_packet_family_counts.is_empty();
    if capture_acceptance_eligible
        && capture_session_evidence.status == "declared_with_evidence"
        && capture_session_evidence.decoded_frame_count > 0
        && !capture_session_packet_family_relevant
    {
        missing.insert("capture_session_relevant_packet_family".to_string());
        blockers
            .insert("capture_session_packet_family_unrelated_for_capture_acceptance".to_string());
    }
    if !pass && !unavailable_status_case {
        missing.insert("passing_metric_report".to_string());
    }
    for issue in issues {
        if issue == "official_label_policy_not_marked" {
            missing.insert("official_label_policy".to_string());
        }
    }
    let has_validation_only_promotion_status = promotion_statuses
        .iter()
        .any(|status| status.starts_with("validation_only"));
    if has_validation_only_promotion_status {
        missing.insert("metric_promotion".to_string());
        blockers.insert("validation_only_promotion_status".to_string());
    }

    let source_kinds = source_kinds.into_iter().collect::<Vec<_>>();
    let acceptance_ready = ok
        && pass
        && !unavailable_status_case
        && !source_kinds.iter().any(|source| source == "unavailable")
        && !has_validation_only_promotion_status
        && !(capture_session_binding_required && capture_session_evidence.status == "not_declared")
        && !matches!(
            capture_session_evidence.status.as_str(),
            "declared_missing_evidence" | "declared_partial_evidence" | "database_error"
        )
        && official_label_status != "official_labels_missing"
        && official_label_status != "official_label_policy_invalid"
        && manual_label_status != "manual_label_missing";
    let capture_acceptance_ready = acceptance_ready
        && capture_acceptance_eligible
        && capture_session_evidence.status == "declared_with_evidence"
        && capture_session_packet_span_sufficient
        && capture_session_packet_family_relevant;

    LocalHealthValidationCaseReadiness {
        normalized_report,
        capture_kind: case.capture_kind.clone(),
        capture_session_status: capture_session_evidence.status.clone(),
        expected_capture_session_ids: capture_session_evidence
            .expected_capture_session_ids
            .clone(),
        capture_session_raw_evidence_count: capture_session_evidence.raw_evidence_count,
        capture_session_decoded_frame_count: capture_session_evidence.decoded_frame_count,
        capture_session_raw_evidence_time_bounds: capture_session_evidence
            .raw_evidence_time_bounds
            .clone(),
        capture_session_decoded_frame_time_bounds: capture_session_evidence
            .decoded_frame_time_bounds
            .clone(),
        capture_session_packet_family_counts: capture_session_evidence.packet_family_counts.clone(),
        capture_acceptance_required_packet_family_prefixes,
        capture_session_relevant_packet_family_counts,
        missing_capture_session_ids: capture_session_evidence.missing_capture_session_ids.clone(),
        evidence_status,
        official_label_status,
        manual_label_status,
        acceptance_ready,
        capture_acceptance_ready,
        source_kinds,
        promotion_statuses: promotion_statuses.into_iter().collect(),
        input_counts,
        blockers: blockers.into_iter().collect(),
        missing: missing.into_iter().collect(),
    }
}

fn capture_session_packet_span_sufficient_for_capture_acceptance(
    evidence: &CaptureSessionEvidenceReadiness,
) -> bool {
    evidence
        .decoded_frame_time_bounds
        .as_ref()
        .and_then(|bounds| bounds.span_ms)
        .is_some_and(|span_ms| span_ms > 0)
}

fn required_capture_packet_family_prefixes(
    normalized_report: Option<&str>,
) -> &'static [&'static str] {
    match normalized_report {
        Some("step-discovery") | Some("step-validation") | Some("raw-motion-steps") => {
            &["K10", "K11", "K21"]
        }
        Some("energy-validation") => &["K2", "K10", "K11", "K18", "K21", "K24"],
        Some("rhr-validation") => &["K2", "K10", "K18", "K24"],
        Some("hrv-validation") => &["K17", "K18", "K24", "EVENT"],
        Some("respiratory-rate-validation") => &["K18", "K24", "EVENT"],
        Some("oxygen-saturation-validation") => &["K2", "K17", "K18", "K24", "EVENT"],
        Some("temperature-validation") => &["K18", "K24", "EVENT"],
        Some("recovery-sensors") => &["K2", "K17", "K18", "K24", "EVENT"],
        _ => &[],
    }
}

fn capture_acceptance_required_packet_family_prefixes_for_case(
    case: &LocalHealthValidationCase,
) -> Vec<String> {
    let normalized_report = normalized_report(&case.report);
    capture_acceptance_required_packet_family_prefixes_for_normalized_report(
        normalized_report.as_deref(),
    )
}

fn capture_acceptance_required_packet_family_prefixes_for_normalized_report(
    normalized_report: Option<&str>,
) -> Vec<String> {
    required_capture_packet_family_prefixes(normalized_report)
        .iter()
        .map(|prefix| (*prefix).to_string())
        .collect()
}

fn relevant_packet_family_counts_for_capture_acceptance<T: Copy>(
    required_prefixes: &[String],
    packet_family_counts: &BTreeMap<String, T>,
) -> BTreeMap<String, T> {
    if required_prefixes.is_empty() {
        return BTreeMap::new();
    }
    packet_family_counts
        .iter()
        .filter(|(family, _count)| {
            required_prefixes
                .iter()
                .any(|required| packet_family_matches_prefix(family, required))
        })
        .map(|(family, count)| (family.clone(), *count))
        .collect()
}

fn packet_family_matches_prefix(family: &str, required_prefix: &str) -> bool {
    family
        .strip_prefix(required_prefix)
        .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with('/'))
}

fn official_label_status(
    case: &LocalHealthValidationCase,
    normalized_report: Option<&str>,
) -> String {
    if has_official_labels(case) {
        if label_policy_valid(case) {
            "official_labels_valid"
        } else {
            "official_label_policy_invalid"
        }
    } else if requires_official_labels(normalized_report) {
        "official_labels_missing"
    } else {
        "not_required"
    }
    .to_string()
}

fn manual_label_status(
    case: &LocalHealthValidationCase,
    normalized_report: Option<&str>,
) -> String {
    if requires_manual_labels(normalized_report) {
        if case.manual_step_delta.is_some() {
            "manual_label_present"
        } else {
            "manual_label_missing"
        }
    } else {
        "not_required"
    }
    .to_string()
}

fn requires_official_labels(normalized_report: Option<&str>) -> bool {
    matches!(
        normalized_report,
        Some("step-validation")
            | Some("raw-motion-steps")
            | Some("energy-validation")
            | Some("rhr-validation")
            | Some("hrv-validation")
            | Some("respiratory-rate-validation")
            | Some("oxygen-saturation-validation")
            | Some("temperature-validation")
    )
}

fn requires_manual_labels(normalized_report: Option<&str>) -> bool {
    matches!(
        normalized_report,
        Some("step-validation") | Some("raw-motion-steps")
    )
}

fn requires_capture_session_binding_for_acceptance(normalized_report: Option<&str>) -> bool {
    matches!(
        normalized_report,
        Some("step-validation")
            | Some("raw-motion-steps")
            | Some("energy-validation")
            | Some("rhr-validation")
            | Some("hrv-validation")
            | Some("respiratory-rate-validation")
            | Some("oxygen-saturation-validation")
            | Some("temperature-validation")
    )
}

fn counts_for_capture_acceptance(normalized_report: Option<&str>) -> bool {
    matches!(
        normalized_report,
        Some("step-discovery")
            | Some("step-validation")
            | Some("raw-motion-steps")
            | Some("energy-validation")
            | Some("rhr-validation")
            | Some("hrv-validation")
            | Some("respiratory-rate-validation")
            | Some("oxygen-saturation-validation")
            | Some("temperature-validation")
    )
}

fn forbids_metric_writes_in_validation_suite(normalized_report: Option<&str>) -> bool {
    matches!(
        normalized_report,
        Some("step-validation")
            | Some("energy-validation")
            | Some("rhr-validation")
            | Some("hrv-validation")
            | Some("respiratory-rate-validation")
            | Some("oxygen-saturation-validation")
            | Some("temperature-validation")
    )
}

fn requires_capture_session_binding_for_metric_write(normalized_report: Option<&str>) -> bool {
    matches!(normalized_report, Some("raw-motion-steps"))
}

fn should_forward_write_metric(
    case: &LocalHealthValidationCase,
    capture_session_evidence: &CaptureSessionEvidenceReadiness,
) -> bool {
    if !case.write_metric {
        return false;
    }
    let normalized_report = normalized_report(&case.report);
    let normalized_report = normalized_report.as_deref();
    if forbids_metric_writes_in_validation_suite(normalized_report) {
        return false;
    }
    if requires_capture_session_binding_for_metric_write(normalized_report)
        && capture_session_evidence.status != "declared_with_evidence"
    {
        return false;
    }
    true
}

fn next_actions_for_report(
    cases: &[LocalHealthValidationCaseReport],
    imports: &[LocalHealthValidationCaptureSqliteImportReport],
    database_source: &LocalHealthValidationDatabaseSource,
    database_source_case_window_issues: &[String],
    database_source_packet_evidence_issues: &[String],
    database_source_case_packet_evidence_issues: &[String],
) -> Vec<LocalHealthValidationNextAction> {
    let mut actions = cases
        .iter()
        .flat_map(|case| {
            let mut actions = Vec::new();
            for issue in &case.issues {
                actions.push(LocalHealthValidationNextAction {
                    case_id: case.id.clone(),
                    scope: "validation_case".to_string(),
                    reason: issue.clone(),
                    action: suite_issue_action(issue).to_string(),
                });
            }
            if let Some(result) = &case.result {
                actions.extend(result_next_actions(&case.id, result));
            }
            for blocker in &case.readiness.blockers {
                if blocker == "capture_session_required_for_capture_acceptance"
                    || blocker
                        == "capture_session_decoded_evidence_too_sparse_for_capture_acceptance"
                    || blocker == "capture_session_packet_family_unrelated_for_capture_acceptance"
                {
                    actions.push(LocalHealthValidationNextAction {
                        case_id: case.id.clone(),
                        scope: "validation_case".to_string(),
                        reason: blocker.clone(),
                        action: suite_issue_action(blocker).to_string(),
                    });
                }
            }
            actions
        })
        .collect::<Vec<_>>();
    for import in imports {
        for issue in &import.issues {
            actions.push(LocalHealthValidationNextAction {
                case_id: import.id.clone(),
                scope: "capture_sqlite_import".to_string(),
                reason: issue.clone(),
                action: suite_issue_action(issue).to_string(),
            });
        }
    }
    if let Some(raw_export_manifest) = &database_source.raw_export_manifest {
        for issue in &raw_export_manifest.issues {
            actions.push(LocalHealthValidationNextAction {
                case_id: "database_source".to_string(),
                scope: "database_source".to_string(),
                reason: issue.clone(),
                action: suite_issue_action(issue).to_string(),
            });
        }
    }
    if let Some(sqlite_audit) = &database_source.sqlite_audit {
        for issue in &sqlite_audit.issues {
            actions.push(LocalHealthValidationNextAction {
                case_id: "database_source".to_string(),
                scope: "database_source".to_string(),
                reason: issue.clone(),
                action: suite_issue_action(issue).to_string(),
            });
        }
    }
    for issue in database_source_case_window_issues {
        actions.push(LocalHealthValidationNextAction {
            case_id: "database_source".to_string(),
            scope: "database_source".to_string(),
            reason: issue.clone(),
            action: suite_issue_action(issue).to_string(),
        });
    }
    for issue in database_source_packet_evidence_issues {
        actions.push(LocalHealthValidationNextAction {
            case_id: "database_source".to_string(),
            scope: "database_source".to_string(),
            reason: issue.clone(),
            action: suite_issue_action(issue).to_string(),
        });
    }
    for issue in database_source_case_packet_evidence_issues {
        actions.push(LocalHealthValidationNextAction {
            case_id: "database_source".to_string(),
            scope: "database_source".to_string(),
            reason: issue.clone(),
            action: suite_issue_action(issue).to_string(),
        });
    }
    actions
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn result_next_actions(case_id: &str, result: &Value) -> Vec<LocalHealthValidationNextAction> {
    result
        .get("next_actions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|action| {
            let reason = action
                .get("reason")
                .or_else(|| action.get("summary"))
                .and_then(Value::as_str)?;
            let scope = action
                .get("scope")
                .or_else(|| action.get("summary"))
                .and_then(Value::as_str)
                .unwrap_or("metric_report");
            let action_text = action
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("Resolve the embedded metric report blocker.");
            Some(LocalHealthValidationNextAction {
                case_id: case_id.to_string(),
                scope: scope.to_string(),
                reason: reason.to_string(),
                action: action_text.to_string(),
            })
        })
        .collect()
}

fn suite_issue_action(issue: &str) -> &'static str {
    match issue {
        issue if issue.starts_with("manifest_json_missing_or_unreadable") => {
            "Regenerate the Raw Export bundle with manifest.json at the bundle root, then rerun validation."
        }
        issue if issue.starts_with("manifest_json_invalid") => {
            "Regenerate manifest.json from bull-raw-export so it matches the Bull Raw Export manifest schema."
        }
        issue if issue.starts_with("manifest_schema_version_unexpected") => {
            "Regenerate the Raw Export bundle with schema_version bull.export.v1."
        }
        "raw_export_time_window_start_invalid" | "raw_export_time_window_end_invalid" => {
            "Regenerate the Raw Export bundle with valid UTC RFC3339 time_window start/end values."
        }
        "raw_export_time_window_not_increasing" => {
            "Regenerate the Raw Export bundle with time_window.end after time_window.start."
        }
        issue if issue.starts_with("case_window_outside_raw_export_time_window") => {
            "Regenerate the Raw Export bundle with a time_window that covers this validation case, or adjust the validation manifest to the exported capture window."
        }
        "no_packet_evidence_in_raw_export_time_window" => {
            "Regenerate the Raw Export bundle from the owned capture window so raw_evidence/decoded_frames contain packet evidence, or adjust validation cases to a bundle with packet data."
        }
        issue if issue.starts_with("case_window_no_packet_evidence") => {
            "Adjust this validation case start/end to the owned capture window, or regenerate the Raw Export bundle so the case window contains raw and decoded packet evidence."
        }
        issue if issue.starts_with("case_window_no_decoded_packet_evidence") => {
            "Re-import or regenerate the Raw Export bundle so packets in this validation case window have decoded_frames rows."
        }
        issue if issue.starts_with("case_window_packet_evidence_query_failed") => {
            "Repair or regenerate data/bull.sqlite so raw and decoded packet evidence can be inspected for each validation case window."
        }
        issue
            if issue.starts_with("case_window_packet_family_query_failed")
                || issue.starts_with("case_window_capture_session_packet_family_query_failed") =>
        {
            "Repair or regenerate data/bull.sqlite so decoded packet families can be inspected for each validation case window."
        }
        issue
            if issue.starts_with("case_window_decoded_evidence_too_sparse_for_capture_acceptance")
                || issue.starts_with(
                    "case_window_capture_session_decoded_evidence_too_sparse_for_capture_acceptance",
                ) =>
        {
            "Regenerate or widen the Raw Export/validation case so decoded packet evidence spans more than one timestamp in the owned capture window."
        }
        issue
            if issue.starts_with("case_window_packet_family_unrelated_for_capture_acceptance")
                || issue.starts_with(
                    "case_window_capture_session_packet_family_unrelated_for_capture_acceptance",
                ) =>
        {
            "Use a validation case window and capture_session_id whose decoded packet families match the metric being accepted, or regenerate the Raw Export bundle with the relevant sensor stream enabled."
        }
        issue
            if issue.starts_with("case_window_time_bounds_query_failed")
                || issue.starts_with("case_window_capture_session_time_bounds_query_failed") =>
        {
            "Repair or regenerate data/bull.sqlite so packet timestamp bounds can be inspected for each validation case window."
        }
        "sqlite_family_not_declared" => {
            "Regenerate the Raw Export bundle with the sqlite family selected so data/bull.sqlite is declared."
        }
        "sqlite_manifest_file_missing" => {
            "Regenerate the Raw Export bundle so manifest.json includes data/bull.sqlite."
        }
        "sqlite_manifest_file_duplicate" => {
            "Regenerate the Raw Export bundle so manifest.json lists data/bull.sqlite exactly once."
        }
        issue if issue.starts_with("sqlite_manifest_kind_unexpected") => {
            "Regenerate the Raw Export bundle so data/bull.sqlite is declared with kind sqlite."
        }
        "sqlite_manifest_sha256_missing" => {
            "Regenerate the Raw Export bundle so data/bull.sqlite has a manifest SHA-256."
        }
        "sqlite_actual_sha256_unavailable" => {
            "Repair data/bull.sqlite permissions or regenerate the Raw Export bundle before validation."
        }
        "sqlite_manifest_sha256_mismatch" => {
            "Regenerate the Raw Export bundle; manifest.json and data/bull.sqlite do not describe the same SQLite snapshot."
        }
        "official_labels_are_labels_not_true_for_calibration_labels" => {
            "Regenerate the Raw Export bundle with official_labels_are_labels=true before using official WHOOP comparison labels."
        }
        issue if issue.starts_with("sqlite_open_failed") => {
            "Regenerate the Raw Export bundle; data/bull.sqlite is not an openable SQLite database."
        }
        issue
            if issue.starts_with("sqlite_schema_version_query_failed")
                || issue == "sqlite_schema_version_missing" =>
        {
            "Regenerate the Raw Export bundle from migrated Bull storage so bull_schema_migrations is present."
        }
        issue if issue.starts_with("sqlite_required_table_missing") => {
            "Regenerate the Raw Export bundle from Bull storage that includes local health evidence and formatted metric tables."
        }
        issue
            if issue.starts_with("sqlite_table_count_failed")
                || issue.starts_with("sqlite_table_lookup_failed")
                || issue.starts_with("sqlite_time_window_count_failed") =>
        {
            "Repair or regenerate data/bull.sqlite so canonical Bull tables can be inspected."
        }
        "official_label_policy_not_marked" => {
            "Set label_provenance.official_labels_are_labels=true; official WHOOP values are validation labels, not inputs."
        }
        "date_key_required_for_step_rollup" => "Add the local date key for this step rollup case.",
        "timezone_required_for_step_rollup" => "Add the local timezone for this step rollup case.",
        "date_key_required_for_step_hourly_rollup" => {
            "Add the local date key for this hourly step rollup case."
        }
        "timezone_required_for_step_hourly_rollup" => {
            "Add the local timezone for this hourly step rollup case."
        }
        "unix_window_required_for_step_rollup" => {
            "Use valid RFC3339 UTC start/end timestamps so the suite can derive step rollup Unix-millisecond windows."
        }
        "date_key_required_for_energy_validation" => {
            "Add the local date key for this energy validation case."
        }
        "timezone_required_for_energy_validation" => {
            "Add the local timezone for this energy validation case."
        }
        "date_key_required_for_energy_rollup" => {
            "Add the local date key for this daily energy rollup case."
        }
        "timezone_required_for_energy_rollup" => {
            "Add the local timezone for this daily energy rollup case."
        }
        "date_key_required_for_energy_hourly_rollup" => {
            "Add the local date key for this hourly energy rollup case."
        }
        "timezone_required_for_energy_hourly_rollup" => {
            "Add the local timezone for this hourly energy rollup case."
        }
        "date_key_required_for_rhr_validation" => {
            "Add the local date key for this RHR validation case."
        }
        "timezone_required_for_rhr_validation" => {
            "Add the local timezone for this RHR validation case."
        }
        "case_report_not_passed" => {
            "Inspect the embedded report issues and capture or label the missing evidence."
        }
        "bridge_method_failed" => {
            "Fix the case arguments so the requested metric bridge method can run."
        }
        "capture_session_evidence_missing" => {
            "Import raw/decoded packet evidence for the declared capture_session_id in this case window, or update the manifest to the owned session that contains the evidence."
        }
        "capture_session_required_for_acceptance" => {
            "Set capture_session_id or capture_session_ids to the owned session used for this labeled validation case."
        }
        "capture_session_required_for_metric_write" => {
            "Set capture_session_id or capture_session_ids before allowing this validation-backed local estimate to write a metric."
        }
        "capture_session_required_for_capture_acceptance" => {
            "Set capture_session_id or capture_session_ids before counting this case as controlled capture acceptance."
        }
        "capture_session_decoded_evidence_too_sparse_for_capture_acceptance" => {
            "Import or regenerate the owned capture so decoded packet evidence from the declared capture_session_id spans more than one timestamp in the validation window."
        }
        "capture_session_packet_family_unrelated_for_capture_acceptance" => {
            "Use a validation case window and capture_session_id whose decoded packet families match the metric being accepted, or regenerate the owned capture with the relevant sensor stream enabled."
        }
        "write_metric_not_allowed_for_validation_report" => {
            "Remove write_metric from this validation-label report; use the matching rollup or unavailable-status case to write metrics."
        }
        "capture_sqlite_import_empty" => {
            "Regenerate the processed capture.sqlite so it contains framed records before using it as validation evidence."
        }
        "capture_sqlite_raw_import_incomplete" => {
            "Repair the target Bull DB or raw_evidence constraints, then rerun the capture.sqlite import."
        }
        "capture_sqlite_import_failed" => {
            "Fix the capture_sqlite_imports path/schema/session declaration, then rerun the validation suite."
        }
        "capture_sqlite_database_open_failed" => {
            "Repair the validation database path or permissions before importing capture.sqlite evidence."
        }
        "capture_sqlite_database_parent_failed" => {
            "Create or repair the validation database parent directory before importing capture.sqlite evidence."
        }
        "capture_sqlite_import_id_required" => {
            "Give every capture_sqlite_imports entry a stable id."
        }
        "capture_sqlite_import_path_required" => {
            "Set capture_sqlite_imports[].path to the processed HCI capture.sqlite file."
        }
        "capture_sqlite_import_session_id_required" => {
            "Set capture_sqlite_imports[].session_id so validation cases can bind to the imported owned session."
        }
        "case_id_required" => "Give every validation case a stable non-empty id.",
        "start_required" => "Set the capture window start timestamp.",
        "end_required" => "Set the capture window end timestamp.",
        _ => "Resolve this validation-suite issue before treating the case as passed.",
    }
}
