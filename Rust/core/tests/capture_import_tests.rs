use std::path::Path;

use bull_core::{
    capture_import::{
        CaptureImportOptions, CaptureSqliteImportOptions, CapturedFrameBatchOptions,
        CapturedFrameInput, import_capture_sqlite, import_captured_frame_batch,
        import_fixture_index,
    },
    fixtures::build_fixture_index,
    protocol::DeviceType,
    store::{CaptureSessionInput, BullStore},
};
use rusqlite::{Connection, params};

const GET_HELLO_FRAME: &str = "aa0108000001e67123019101363e5c8d";

#[test]
fn imports_indexed_frame_fixture_into_sqlite_raw_and_decoded_tables() {
    let tempdir = tempfile::tempdir().unwrap();
    let db_path = tempdir.path().join("bull.sqlite");
    let store = BullStore::open(&db_path).unwrap();
    let fixture_root = Path::new("fixtures");
    let index = build_fixture_index(fixture_root).unwrap();

    let report = import_fixture_index(
        &store,
        &index,
        CaptureImportOptions {
            fixture_root,
            database_path: &db_path,
            parser_version: "bull-core/test",
        },
    );

    assert!(report.pass, "{:?}", report.issues);
    assert!(report.next_actions.is_empty());
    assert_eq!(report.raw_inserted, 8);
    assert_eq!(report.frames_inserted, 8);
    assert_eq!(store.table_count("raw_evidence").unwrap(), 8);
    assert_eq!(store.table_count("decoded_frames").unwrap(), 8);

    let fixture = report
        .fixtures
        .iter()
        .find(|fixture| fixture.id == "synthetic.bull.v5.get_hello_frame")
        .unwrap();
    assert_eq!(fixture.packet_type, Some(35));
    assert_eq!(fixture.packet_type_name.as_deref(), Some("COMMAND"));
    assert_eq!(fixture.parsed_payload_kind.as_deref(), Some("command"));
    assert_eq!(fixture.sequence, Some(1));
    assert_eq!(fixture.command_or_event, Some(145));

    let historical = report
        .fixtures
        .iter()
        .find(|fixture| fixture.id == "synthetic.bull.v5.historical_k18_packet")
        .unwrap();
    assert_eq!(historical.packet_type, Some(47));
    assert_eq!(
        historical.packet_type_name.as_deref(),
        Some("HISTORICAL_DATA")
    );
    assert_eq!(
        historical.parsed_payload_kind.as_deref(),
        Some("data_packet")
    );
    assert_eq!(historical.sequence, Some(18));

    let event = report
        .fixtures
        .iter()
        .find(|fixture| fixture.id == "synthetic.bull.v5.temperature_event")
        .unwrap();
    assert_eq!(event.packet_type, Some(48));
    assert_eq!(event.packet_type_name.as_deref(), Some("EVENT"));
    assert_eq!(event.parsed_payload_kind.as_deref(), Some("event"));
    assert_eq!(event.command_or_event, Some(17));

    let motion = report
        .fixtures
        .iter()
        .find(|fixture| fixture.id == "synthetic.bull.v5.k10_motion_summary_short")
        .unwrap();
    assert_eq!(motion.parsed_payload_kind.as_deref(), Some("data_packet"));
    let decoded_motion = store
        .decoded_frame("synthetic.bull.v5.k10_motion_summary_short.frame.0")
        .unwrap()
        .unwrap();
    let parsed_payload: serde_json::Value =
        serde_json::from_str(&decoded_motion.parsed_payload_json).unwrap();
    assert_eq!(parsed_payload["body_summary"]["kind"], "raw_motion_k10");
    assert!(
        parsed_payload["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning == "accelerometer_x_truncated")
    );

    let sanitized_batch_motion = report
        .fixtures
        .iter()
        .find(|fixture| fixture.id == "synthetic.sanitized.corebluetooth.k10_motion")
        .unwrap();
    assert_eq!(
        sanitized_batch_motion.parsed_payload_kind.as_deref(),
        Some("data_packet")
    );
    assert_eq!(
        sanitized_batch_motion.packet_type_name.as_deref(),
        Some("REALTIME_RAW_DATA")
    );
}

#[test]
fn repeated_import_is_idempotent() {
    let tempdir = tempfile::tempdir().unwrap();
    let db_path = tempdir.path().join("bull.sqlite");
    let store = BullStore::open(&db_path).unwrap();
    let fixture_root = Path::new("fixtures");
    let index = build_fixture_index(fixture_root).unwrap();

    let first = import_fixture_index(
        &store,
        &index,
        CaptureImportOptions {
            fixture_root,
            database_path: &db_path,
            parser_version: "bull-core/test",
        },
    );
    let second = import_fixture_index(
        &store,
        &index,
        CaptureImportOptions {
            fixture_root,
            database_path: &db_path,
            parser_version: "bull-core/test",
        },
    );

    assert!(first.pass);
    assert!(second.pass);
    assert_eq!(second.raw_inserted, 0);
    assert_eq!(second.raw_existing, 8);
    assert_eq!(second.frames_inserted, 0);
    assert_eq!(second.frames_existing, 8);
    assert_eq!(store.table_count("raw_evidence").unwrap(), 8);
    assert_eq!(store.table_count("decoded_frames").unwrap(), 8);
}

#[test]
fn imports_app_captured_frame_batch_and_returns_timeline_rows() {
    let store = BullStore::open_in_memory().unwrap();
    store
        .start_capture_session(CaptureSessionInput {
            session_id: "capture-import-session",
            source: "ios.corebluetooth.notification",
            started_at_unix_ms: 1770000000000,
            device_model: "WHOOP 5.0 Bull",
            active_device_id: None,
            provenance_json: "{}",
        })
        .unwrap();
    let frames = vec![CapturedFrameInput {
        evidence_id: "app-capture-1".to_string(),
        frame_id: None,
        source: "ios.corebluetooth.notification".to_string(),
        captured_at: "2026-05-28T12:00:00Z".to_string(),
        device_model: "WHOOP 5.0 Bull".to_string(),
        frame_hex: GET_HELLO_FRAME.to_string(),
        sensitivity: "user-owned-capture".to_string(),
        capture_session_id: Some("capture-import-session".to_string()),
        device_type: DeviceType::Bull,
    }];

    let report = import_captured_frame_batch(
        &store,
        &frames,
        CapturedFrameBatchOptions {
            parser_version: "bull-core/test",
        },
    )
    .unwrap();

    assert!(report.pass, "{:?}", report.issues);
    assert!(report.next_actions.is_empty());
    assert_eq!(report.raw_inserted, 1);
    assert_eq!(report.frames_inserted, 1);
    assert_eq!(report.timeline_rows.len(), 1);
    assert_eq!(report.timeline_rows[0].category, "command");
    assert_eq!(
        report.results[0].packet_type_name.as_deref(),
        Some("COMMAND")
    );
    assert_eq!(store.table_count("raw_evidence").unwrap(), 1);
    assert_eq!(store.table_count("decoded_frames").unwrap(), 1);
    let raw = store.raw_evidence("app-capture-1").unwrap().unwrap();
    assert_eq!(
        raw.capture_session_id.as_deref(),
        Some("capture-import-session")
    );
}

#[test]
fn captured_frame_batch_preserves_raw_bytes_when_session_reference_is_broken() {
    let store = BullStore::open_in_memory().unwrap();
    let frames = vec![CapturedFrameInput {
        evidence_id: "app-capture-missing-session".to_string(),
        frame_id: None,
        source: "ios.corebluetooth.notification".to_string(),
        captured_at: "2026-05-28T12:00:00Z".to_string(),
        device_model: "WHOOP 5.0 Bull".to_string(),
        frame_hex: GET_HELLO_FRAME.to_string(),
        sensitivity: "user-owned-capture".to_string(),
        capture_session_id: Some("missing-capture-session".to_string()),
        device_type: DeviceType::Bull,
    }];

    let report = import_captured_frame_batch(
        &store,
        &frames,
        CapturedFrameBatchOptions {
            parser_version: "bull-core/test",
        },
    )
    .unwrap();

    assert!(!report.pass);
    assert_eq!(report.raw_inserted, 1);
    assert_eq!(report.frames_inserted, 1);
    assert!(
        report.results[0].issues.iter().any(|issue| issue.contains(
            "raw evidence inserted without capture_session_id after session-scoped insert failed"
        )),
        "{:?}",
        report.results[0].issues
    );
    let raw = store
        .raw_evidence("app-capture-missing-session")
        .unwrap()
        .unwrap();
    assert_eq!(raw.capture_session_id, None);
    assert!(
        store
            .decoded_frame("app-capture-missing-session.frame.0")
            .unwrap()
            .is_some()
    );
}

#[test]
fn captured_frame_batch_preserves_raw_bytes_when_parse_fails() {
    let store = BullStore::open_in_memory().unwrap();
    let frames = vec![CapturedFrameInput {
        evidence_id: "app-capture-malformed".to_string(),
        frame_id: None,
        source: "ios.corebluetooth.notification".to_string(),
        captured_at: "2026-05-28T12:00:00Z".to_string(),
        device_model: "WHOOP 5.0 Bull".to_string(),
        frame_hex: "00010203".to_string(),
        sensitivity: "user-owned-capture".to_string(),
        capture_session_id: None,
        device_type: DeviceType::Bull,
    }];

    let report = import_captured_frame_batch(
        &store,
        &frames,
        CapturedFrameBatchOptions {
            parser_version: "bull-core/test",
        },
    )
    .unwrap();

    assert!(!report.pass);
    assert_eq!(report.raw_inserted, 1);
    assert_eq!(report.frames_inserted, 0);
    assert_eq!(report.timeline_rows.len(), 0);
    assert!(!report.results[0].parse_ok);
    assert!(
        report.results[0]
            .issues
            .iter()
            .any(|issue| issue.contains("does not start with 0xaa"))
    );
    assert!(
        report.results[0].next_actions.iter().any(|action| {
            action.reason == "frame_parse_failed"
                && action.action.contains("add this frame as a parser fixture")
        }),
        "{:?}",
        report.results[0].next_actions
    );
    assert!(
        report.next_actions.iter().any(|action| {
            action.scope == "app-capture-malformed" && action.reason == "frame_parse_failed"
        }),
        "{:?}",
        report.next_actions
    );
    let raw = store
        .raw_evidence("app-capture-malformed")
        .unwrap()
        .unwrap();
    assert_eq!(raw.payload_hex, "00010203");
}

#[test]
fn repeated_captured_frame_batch_import_is_idempotent() {
    let store = BullStore::open_in_memory().unwrap();
    let frames = vec![CapturedFrameInput {
        evidence_id: "app-capture-repeat".to_string(),
        frame_id: Some("app-capture-repeat.frame.known".to_string()),
        source: "ios.corebluetooth.notification".to_string(),
        captured_at: "2026-05-28T12:00:00Z".to_string(),
        device_model: "WHOOP 5.0 Bull".to_string(),
        frame_hex: GET_HELLO_FRAME.to_string(),
        sensitivity: "user-owned-capture".to_string(),
        capture_session_id: None,
        device_type: DeviceType::Bull,
    }];

    let first = import_captured_frame_batch(
        &store,
        &frames,
        CapturedFrameBatchOptions {
            parser_version: "bull-core/test",
        },
    )
    .unwrap();
    let second = import_captured_frame_batch(
        &store,
        &frames,
        CapturedFrameBatchOptions {
            parser_version: "bull-core/test",
        },
    )
    .unwrap();

    assert!(first.pass);
    assert!(second.pass);
    assert!(second.next_actions.is_empty());
    assert_eq!(second.raw_inserted, 0);
    assert_eq!(second.raw_existing, 1);
    assert_eq!(second.frames_inserted, 0);
    assert_eq!(second.frames_existing, 1);
    assert_eq!(second.timeline_rows.len(), 1);
}

#[test]
fn captured_frame_batch_reports_next_actions_for_invalid_hex_and_empty_input() {
    let store = BullStore::open_in_memory().unwrap();
    let invalid_hex = import_captured_frame_batch(
        &store,
        &[CapturedFrameInput {
            evidence_id: "app-capture-invalid-hex".to_string(),
            frame_id: None,
            source: "ios.corebluetooth.notification".to_string(),
            captured_at: "2026-05-28T12:00:00Z".to_string(),
            device_model: "WHOOP 5.0 Bull".to_string(),
            frame_hex: "not hex".to_string(),
            sensitivity: "user-owned-capture".to_string(),
            capture_session_id: None,
            device_type: DeviceType::Bull,
        }],
        CapturedFrameBatchOptions {
            parser_version: "bull-core/test",
        },
    )
    .unwrap();

    assert!(!invalid_hex.pass);
    assert!(
        invalid_hex.next_actions.iter().any(|action| {
            action.scope == "app-capture-invalid-hex" && action.reason == "frame_hex_invalid"
        }),
        "{:?}",
        invalid_hex.next_actions
    );

    let empty = import_captured_frame_batch(
        &store,
        &[],
        CapturedFrameBatchOptions {
            parser_version: "bull-core/test",
        },
    )
    .unwrap();

    assert!(!empty.pass);
    assert!(
        empty.next_actions.iter().any(|action| {
            action.scope == "captured_frame_batch" && action.reason == "captured_frame_batch_empty"
        }),
        "{:?}",
        empty.next_actions
    );
}

#[test]
fn imports_processed_capture_sqlite_into_owned_bull_session() {
    let tempdir = tempfile::tempdir().unwrap();
    let source_path = tempdir.path().join("capture.sqlite");
    let db_path = tempdir.path().join("bull.sqlite");
    seed_processed_capture_sqlite(&source_path, &[("2026-05-29T00:50:27.270763+00:00", 3)]);

    let store = BullStore::open(&db_path).unwrap();
    let first = import_capture_sqlite(
        &store,
        CaptureSqliteImportOptions {
            source_database_path: &source_path,
            target_database_path: &db_path,
            session_id: "capture.sqlite.import.test",
            device_model: "WHOOP 5.0 Bull",
            sensitivity: "user-owned-capture",
            parser_version: "bull-core/test",
        },
    )
    .unwrap();

    assert!(first.pass, "{:?}", first.issues);
    assert!(first.decode_pass);
    assert_eq!(first.source_frame_count, 1);
    assert_eq!(first.raw_inserted, 1);
    assert_eq!(first.frames_inserted, 1);
    assert_eq!(first.parse_failed_count, 0);
    assert!(first.raw_import_completed);
    assert!(first.session_started);
    assert!(first.session_finished);
    assert_eq!(store.table_count("capture_sessions").unwrap(), 1);
    assert_eq!(store.table_count("raw_evidence").unwrap(), 1);
    assert_eq!(store.table_count("decoded_frames").unwrap(), 1);

    let session = store
        .capture_session("capture.sqlite.import.test")
        .unwrap()
        .unwrap();
    assert_eq!(session.status, "finished");
    assert_eq!(session.frame_count, 1);
    assert_eq!(session.started_at_unix_ms, 1_780_015_827_270);
    assert_eq!(session.ended_at_unix_ms, Some(1_780_015_827_270));

    let raw = store
        .raw_evidence("capture.sqlite.import.test.line-3.decode-0")
        .unwrap()
        .unwrap();
    assert_eq!(raw.captured_at, "2026-05-29T00:50:27.270763Z");
    assert_eq!(
        raw.capture_session_id.as_deref(),
        Some("capture.sqlite.import.test")
    );

    let second = import_capture_sqlite(
        &store,
        CaptureSqliteImportOptions {
            source_database_path: &source_path,
            target_database_path: &db_path,
            session_id: "capture.sqlite.import.test",
            device_model: "WHOOP 5.0 Bull",
            sensitivity: "user-owned-capture",
            parser_version: "bull-core/test",
        },
    )
    .unwrap();

    assert!(second.pass, "{:?}", second.issues);
    assert_eq!(second.raw_inserted, 0);
    assert_eq!(second.raw_existing, 1);
    assert_eq!(second.frames_inserted, 0);
    assert_eq!(second.frames_existing, 1);
    assert_eq!(store.table_count("raw_evidence").unwrap(), 1);
    assert_eq!(store.table_count("decoded_frames").unwrap(), 1);
}

#[test]
fn capture_sqlite_import_preserves_raw_evidence_for_parser_failures() {
    let tempdir = tempfile::tempdir().unwrap();
    let source_path = tempdir.path().join("capture.sqlite");
    let db_path = tempdir.path().join("bull.sqlite");
    seed_processed_capture_sqlite_with_hex(
        &source_path,
        &[("2026-05-29T00:50:27Z", 3, "00010203")],
    );

    let store = BullStore::open(&db_path).unwrap();
    let report = import_capture_sqlite(
        &store,
        CaptureSqliteImportOptions {
            source_database_path: &source_path,
            target_database_path: &db_path,
            session_id: "capture.sqlite.malformed",
            device_model: "WHOOP 5.0 Bull",
            sensitivity: "user-owned-capture",
            parser_version: "bull-core/test",
        },
    )
    .unwrap();

    assert!(!report.pass);
    assert!(!report.decode_pass);
    assert!(report.raw_import_completed);
    assert_eq!(report.raw_inserted, 1);
    assert_eq!(report.frames_inserted, 0);
    assert_eq!(report.parse_failed_count, 1);
    assert!(
        report.next_actions.iter().any(|action| {
            action.reason == "capture_sqlite_decode_incomplete"
                || action.reason == "frame_parse_failed"
        }),
        "{:?}",
        report.next_actions
    );
    assert_eq!(store.table_count("raw_evidence").unwrap(), 1);
    assert_eq!(store.table_count("decoded_frames").unwrap(), 0);
}

fn seed_processed_capture_sqlite(path: &Path, rows: &[(&str, i64)]) {
    let rows = rows
        .iter()
        .map(|(timestamp, line_no)| (*timestamp, *line_no, GET_HELLO_FRAME))
        .collect::<Vec<_>>();
    seed_processed_capture_sqlite_with_hex(path, &rows);
}

fn seed_processed_capture_sqlite_with_hex(path: &Path, rows: &[(&str, i64, &str)]) {
    let connection = Connection::open(path).unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TABLE records (
                id INTEGER PRIMARY KEY,
                file_id INTEGER NOT NULL,
                line_no INTEGER NOT NULL,
                ts TEXT,
                kind TEXT,
                direction TEXT,
                address TEXT,
                role TEXT,
                service_uuid TEXT,
                characteristic_uuid TEXT,
                descriptor_uuid TEXT,
                value_hex TEXT,
                raw_json TEXT NOT NULL
            );
            CREATE TABLE packets (
                id INTEGER PRIMARY KEY,
                record_id INTEGER NOT NULL,
                decode_index INTEGER NOT NULL,
                packet_type TEXT,
                packet_type_id INTEGER,
                command TEXT,
                command_id INTEGER,
                event TEXT,
                event_id INTEGER,
                result TEXT,
                result_id INTEGER,
                sequence INTEGER,
                origin_sequence INTEGER,
                data_packet_revision INTEGER,
                data_packet_domain TEXT,
                raw_stream TEXT,
                request_schema TEXT,
                request_domain TEXT,
                request_operation TEXT,
                request_complete INTEGER,
                request_payload_len INTEGER,
                request_padding_len INTEGER,
                request_padding_is_zero INTEGER,
                response_schema TEXT,
                event_schema TEXT,
                event_domain TEXT,
                payload_hex TEXT,
                payload_len INTEGER,
                is_frame INTEGER,
                frame_complete INTEGER,
                frame_header_crc_valid INTEGER,
                frame_payload_crc32_valid INTEGER,
                decoded_json TEXT NOT NULL
            );
            "#,
        )
        .unwrap();
    for (index, (timestamp, line_no, frame_hex)) in rows.iter().enumerate() {
        let record_id = index as i64 + 1;
        connection
            .execute(
                r#"
                INSERT INTO records (
                    id, file_id, line_no, ts, kind, direction, role, value_hex, raw_json
                ) VALUES (?1, 1, ?2, ?3, 'att', 'notify', 'data_from_strap', ?4, '{}')
                "#,
                params![record_id, line_no, timestamp, frame_hex],
            )
            .unwrap();
        connection
            .execute(
                r#"
                INSERT INTO packets (
                    id, record_id, decode_index, packet_type, packet_type_id, is_frame, decoded_json
                ) VALUES (?1, ?2, 0, 'COMMAND', 35, 1, '{}')
                "#,
                params![record_id, record_id],
            )
            .unwrap();
    }
}

// -- Live bulk-stream ingest decimation ------------------------------------

fn bulk_k10_frame_hex() -> String {
    let mut payload = vec![0u8; 1288];
    payload[0] = bull_core::protocol::PACKET_TYPE_REALTIME_RAW_DATA;
    payload[1] = 10;
    hex::encode(bull_core::protocol::build_v5_payload_frame(&payload))
}

fn live_frame(id: &str, captured_at: &str, source: &str) -> CapturedFrameInput {
    CapturedFrameInput {
        evidence_id: id.to_string(),
        frame_id: None,
        source: source.to_string(),
        captured_at: captured_at.to_string(),
        device_model: "WHOOP 5.0 Bull".to_string(),
        frame_hex: bulk_k10_frame_hex(),
        sensitivity: "user-owned-capture".to_string(),
        capture_session_id: None,
        device_type: DeviceType::Bull,
    }
}

#[test]
fn live_bulk_stream_frames_are_decimated_to_the_sampling_interval() {
    let store = BullStore::open_in_memory().unwrap();
    let live = "ios.corebluetooth.notification";
    let report = import_captured_frame_batch(
        &store,
        &[
            live_frame("live-0s", "2026-07-08T00:00:00Z", live),
            live_frame("live-5s", "2026-07-08T00:00:05Z", live),
            live_frame("live-12s", "2026-07-08T00:00:12Z", live),
        ],
        CapturedFrameBatchOptions {
            parser_version: "bull-core/test",
        },
    )
    .unwrap();
    assert!(report.pass, "{:?}", report.issues);
    assert_eq!(report.frames_decimated, 1, "the 5s frame is inside the window");
    assert_eq!(report.raw_inserted, 2, "0s and 12s frames are kept");

    // The watermark persists across batches: 3s after the last kept frame is
    // still inside the sampling window.
    let followup = import_captured_frame_batch(
        &store,
        &[live_frame("live-15s", "2026-07-08T00:00:15Z", live)],
        CapturedFrameBatchOptions {
            parser_version: "bull-core/test",
        },
    )
    .unwrap();
    assert_eq!(followup.frames_decimated, 1);
    assert_eq!(followup.raw_inserted, 0);

    // Historical sync transfers are never decimated, even for the same shape.
    let historical = import_captured_frame_batch(
        &store,
        &[
            live_frame("hist-0s", "2026-07-08T00:00:01Z", "ios.historical_sync"),
            live_frame("hist-2s", "2026-07-08T00:00:03Z", "ios.historical_sync"),
        ],
        CapturedFrameBatchOptions {
            parser_version: "bull-core/test",
        },
    )
    .unwrap();
    assert_eq!(historical.frames_decimated, 0);
    assert_eq!(historical.raw_inserted, 2);
}

#[test]
fn finish_stale_capture_sessions_closes_only_orphans_before_cutoff() {
    let store = BullStore::open_in_memory().unwrap();
    for (session_id, started_at) in [("orphan-old", 1_000i64), ("fresh", 900_000i64)] {
        store
            .start_capture_session(CaptureSessionInput {
                session_id,
                source: "test",
                started_at_unix_ms: started_at,
                device_model: "WHOOP 5.0 Bull",
                active_device_id: None,
                provenance_json: "{}",
            })
            .unwrap();
    }
    store.finish_capture_session("orphan-old", 2_000, 5).ok();
    // Re-open a second orphan that stays active.
    store
        .start_capture_session(CaptureSessionInput {
            session_id: "orphan-active",
            source: "test",
            started_at_unix_ms: 5_000,
            device_model: "WHOOP 5.0 Bull",
            active_device_id: None,
            provenance_json: "{}",
        })
        .unwrap();

    let closed = store.finish_stale_capture_sessions(100_000).unwrap();
    assert_eq!(closed, 1, "only the active orphan before the cutoff closes");

    let orphan = store.capture_session("orphan-active").unwrap().unwrap();
    assert_eq!(orphan.status, "finished");
    assert_eq!(
        orphan.ended_at_unix_ms,
        Some(5_000),
        "orphans close at their start time; they cannot prove later coverage"
    );
    let fresh = store.capture_session("fresh").unwrap().unwrap();
    assert_eq!(fresh.status, "active", "sessions after the cutoff are untouched");
}
