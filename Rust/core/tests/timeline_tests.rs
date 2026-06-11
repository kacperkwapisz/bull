use std::path::Path;

use bull_core::{
    capture_import::{CaptureImportOptions, import_fixture_index},
    debug_ws::{
        DEBUG_EVENT_TOPIC_ACTIVITY_CANDIDATE_CORRECTED,
        DEBUG_EVENT_TOPIC_ACTIVITY_CANDIDATE_CREATED,
        DEBUG_EVENT_TOPIC_ACTIVITY_CANDIDATE_PROMOTED,
        DEBUG_EVENT_TOPIC_ACTIVITY_FEATURE_WINDOW_CREATED,
        DEBUG_EVENT_TOPIC_ACTIVITY_SESSION_STATS_DISPLAYED,
        DEBUG_EVENT_TOPIC_EXPORT_RAW_TIMEFRAME_COMPLETED,
        DEBUG_EVENT_TOPIC_EXPORT_RAW_TIMEFRAME_PLANNED,
        DEBUG_EVENT_TOPIC_HEALTH_SYNC_ACTIVITY_BLOCKED,
        DEBUG_EVENT_TOPIC_HEALTH_SYNC_ACTIVITY_PLANNED,
    },
    fixtures::build_fixture_index,
    store::{DebugEventRow, DecodedFrameRow, BullStore, RawEvidenceRow},
    timeline::{
        ObservabilityStage, PacketTimelineRow, observability_timeline_from_rows,
        packet_timeline_between, packet_timeline_from_decoded_frames,
    },
};

#[test]
fn timeline_normalizes_command_event_history_and_batch_rows() {
    let store = BullStore::open_in_memory().unwrap();
    let fixture_root = Path::new("fixtures");
    let index = build_fixture_index(fixture_root).unwrap();
    let report = import_fixture_index(
        &store,
        &index,
        CaptureImportOptions {
            fixture_root,
            database_path: Path::new(":memory:"),
            parser_version: "bull-core/test",
        },
    );
    assert!(report.pass, "{:?}", report.issues);

    let rows =
        packet_timeline_between(&store, "2026-05-27T00:00:00Z", "2026-05-27T00:30:00Z").unwrap();

    assert!(rows.len() >= 5, "{rows:?}");

    let command = rows
        .iter()
        .find(|row| row.evidence_id == "synthetic.bull.v5.get_hello_frame")
        .unwrap();
    assert_eq!(command.category, "command");
    assert_eq!(command.title, "Command GET_HELLO");
    assert_eq!(command.packet_type_name.as_deref(), Some("COMMAND"));

    let event = rows
        .iter()
        .find(|row| row.evidence_id == "synthetic.bull.v5.temperature_event")
        .unwrap();
    assert_eq!(event.category, "event");
    assert_eq!(event.title, "Event TEMPERATURE_LEVEL");
    assert_eq!(event.device_timestamp_seconds, Some(16909060));
    assert_eq!(event.body_hex.as_deref(), Some("deadbeef"));

    let historical = rows
        .iter()
        .find(|row| row.evidence_id == "synthetic.bull.v5.historical_k18_packet")
        .unwrap();
    assert_eq!(historical.category, "data_packet");
    assert_eq!(
        historical.title,
        "Data packet normal_history_with_hr_marker"
    );
    assert_eq!(historical.device_timestamp_seconds, Some(287454020));
    assert_eq!(historical.summary["packet_k"], 18);
    assert_eq!(historical.summary["hr_present_marker"], 77);

    let batch_motion = rows
        .iter()
        .find(|row| row.evidence_id == "synthetic.sanitized.corebluetooth.k10_motion")
        .unwrap();
    assert_eq!(batch_motion.category, "data_packet");
    assert_eq!(
        batch_motion.packet_type_name.as_deref(),
        Some("REALTIME_RAW_DATA")
    );
    assert_eq!(
        batch_motion.summary["body_summary"]["kind"],
        "raw_motion_k10"
    );
}

#[test]
fn observability_timeline_links_raw_packets_feature_windows_candidates_promotions_and_stats() {
    let raw_row = RawEvidenceRow {
        evidence_id: "synthetic.raw.evidence-1".to_string(),
        source: "synthetic.activity".to_string(),
        captured_at: "2026-05-27T06:00:00Z".to_string(),
        device_model: "WHOOP 5.0 Bull".to_string(),
        payload_hex: "deadbeef".to_string(),
        sha256: "sha256-raw-1".to_string(),
        sensitivity: "public-test-fixture".to_string(),
        capture_session_id: Some("capture-session-1".to_string()),
    };
    let packet_row = PacketTimelineRow {
        timeline_id: "synthetic.frame-1.timeline".to_string(),
        frame_id: "synthetic.frame-1".to_string(),
        evidence_id: raw_row.evidence_id.clone(),
        captured_at: "2026-05-27T06:00:01Z".to_string(),
        category: "data_packet".to_string(),
        title: "Data packet normal_history".to_string(),
        packet_type_name: Some("HISTORICAL_DATA".to_string()),
        sequence: Some(7),
        command_or_event: Some(18),
        device_timestamp_seconds: Some(287454020),
        device_timestamp_subseconds: Some(123),
        body_hex: Some("cafe".to_string()),
        summary: serde_json::json!({
            "packet_k": 18,
            "body_summary": {
                "kind": "normal_history"
            }
        }),
        warnings: vec!["parser: synthetic".to_string()],
    };
    let debug_rows = vec![
        DebugEventRow {
            session_id: "debug-observability".to_string(),
            sequence: 1,
            schema: "bull.debug.event.v1".to_string(),
            time_unix_ms: 1779840000100,
            source: "rust".to_string(),
            level: "debug".to_string(),
            topic: DEBUG_EVENT_TOPIC_ACTIVITY_FEATURE_WINDOW_CREATED.to_string(),
            message: "feature window built".to_string(),
            command_id: None,
            data_json: serde_json::json!({
                "raw_evidence_id": raw_row.evidence_id.clone(),
                "frame_id": packet_row.frame_id.clone(),
                "feature_window_id": "feature-window-1",
                "window_id": "feature-window-1"
            })
            .to_string(),
        },
        DebugEventRow {
            session_id: "debug-observability".to_string(),
            sequence: 2,
            schema: "bull.debug.event.v1".to_string(),
            time_unix_ms: 1779840000200,
            source: "rust".to_string(),
            level: "info".to_string(),
            topic: DEBUG_EVENT_TOPIC_ACTIVITY_CANDIDATE_CREATED.to_string(),
            message: "activity candidate created".to_string(),
            command_id: None,
            data_json: serde_json::json!({
                "feature_window_id": "feature-window-1",
                "candidate_id": "candidate-1",
                "activity_type": "running",
                "confidence_0_to_1": 0.91
            })
            .to_string(),
        },
        DebugEventRow {
            session_id: "debug-observability".to_string(),
            sequence: 3,
            schema: "bull.debug.event.v1".to_string(),
            time_unix_ms: 1779840000250,
            source: "rust".to_string(),
            level: "warn".to_string(),
            topic: DEBUG_EVENT_TOPIC_ACTIVITY_CANDIDATE_CORRECTED.to_string(),
            message: "activity candidate corrected".to_string(),
            command_id: None,
            data_json: serde_json::json!({
                "candidate_id": "candidate-1",
                "feature_window_id": "feature-window-1",
                "activity_session_id": "session-1",
                "correction": "activity_type->jogging"
            })
            .to_string(),
        },
        DebugEventRow {
            session_id: "debug-observability".to_string(),
            sequence: 4,
            schema: "bull.debug.event.v1".to_string(),
            time_unix_ms: 1779840000300,
            source: "rust".to_string(),
            level: "info".to_string(),
            topic: DEBUG_EVENT_TOPIC_ACTIVITY_CANDIDATE_PROMOTED.to_string(),
            message: "activity candidate promoted".to_string(),
            command_id: None,
            data_json: serde_json::json!({
                "candidate_id": "candidate-1",
                "activity_session_id": "session-1",
                "stat_id": "displayed-stat-1"
            })
            .to_string(),
        },
        DebugEventRow {
            session_id: "debug-observability".to_string(),
            sequence: 5,
            schema: "bull.debug.event.v1".to_string(),
            time_unix_ms: 1779840000400,
            source: "metric".to_string(),
            level: "info".to_string(),
            topic: DEBUG_EVENT_TOPIC_ACTIVITY_SESSION_STATS_DISPLAYED.to_string(),
            message: "activity stats displayed".to_string(),
            command_id: None,
            data_json: serde_json::json!({
                "activity_session_id": "session-1",
                "stat_id": "displayed-stat-1",
                "metric_name": "strain",
                "value": 12.3,
                "unit": "load"
            })
            .to_string(),
        },
        DebugEventRow {
            session_id: "debug-observability".to_string(),
            sequence: 6,
            schema: "bull.debug.event.v1".to_string(),
            time_unix_ms: 1779840000500,
            source: "sqlite".to_string(),
            level: "debug".to_string(),
            topic: DEBUG_EVENT_TOPIC_EXPORT_RAW_TIMEFRAME_PLANNED.to_string(),
            message: "export raw timeframe planned".to_string(),
            command_id: None,
            data_json: serde_json::json!({
                "export_job_id": "export-1",
                "activity_session_id": "session-1",
                "raw_evidence_rows": 2,
                "decoded_frame_rows": 1,
                "packet_timeline_rows": 1
            })
            .to_string(),
        },
        DebugEventRow {
            session_id: "debug-observability".to_string(),
            sequence: 7,
            schema: "bull.debug.event.v1".to_string(),
            time_unix_ms: 1779840000550,
            source: "sqlite".to_string(),
            level: "info".to_string(),
            topic: DEBUG_EVENT_TOPIC_EXPORT_RAW_TIMEFRAME_COMPLETED.to_string(),
            message: "export raw timeframe completed".to_string(),
            command_id: None,
            data_json: serde_json::json!({
                "export_job_id": "export-1",
                "activity_session_id": "session-1",
                "row_count": 3,
                "bundle_path": "export.bullbundle"
            })
            .to_string(),
        },
        DebugEventRow {
            session_id: "debug-observability".to_string(),
            sequence: 8,
            schema: "bull.debug.event.v1".to_string(),
            time_unix_ms: 1779840000600,
            source: "rust".to_string(),
            level: "info".to_string(),
            topic: DEBUG_EVENT_TOPIC_HEALTH_SYNC_ACTIVITY_PLANNED.to_string(),
            message: "health sync activity planned".to_string(),
            command_id: None,
            data_json: serde_json::json!({
                "plan_id": "health-plan-1",
                "activity_session_id": "session-1",
                "platform": "health_kit",
                "planned_session_count": 1
            })
            .to_string(),
        },
        DebugEventRow {
            session_id: "debug-observability".to_string(),
            sequence: 9,
            schema: "bull.debug.event.v1".to_string(),
            time_unix_ms: 1779840000700,
            source: "rust".to_string(),
            level: "warn".to_string(),
            topic: DEBUG_EVENT_TOPIC_HEALTH_SYNC_ACTIVITY_BLOCKED.to_string(),
            message: "health sync activity blocked".to_string(),
            command_id: None,
            data_json: serde_json::json!({
                "plan_id": "health-plan-2",
                "candidate_id": "candidate-1",
                "blocked_session_count": 1
            })
            .to_string(),
        },
    ];

    let rows = observability_timeline_from_rows(&[raw_row], &[packet_row], &debug_rows).unwrap();

    assert_eq!(rows.len(), 11, "{rows:#?}");
    assert_eq!(rows[0].stage, ObservabilityStage::RawFrame);
    assert_eq!(rows[0].timeline_id, "raw.synthetic.raw.evidence-1");
    assert_eq!(rows[0].parent_timeline_id, None);
    assert_eq!(rows[1].stage, ObservabilityStage::DecodedPacket);
    assert_eq!(
        rows[1].parent_timeline_id.as_deref(),
        Some("raw.synthetic.raw.evidence-1")
    );
    assert_eq!(rows[2].stage, ObservabilityStage::FeatureWindow);
    assert_eq!(
        rows[2].parent_timeline_id.as_deref(),
        Some("synthetic.frame-1.timeline")
    );
    assert_eq!(rows[3].stage, ObservabilityStage::ActivityCandidate);
    assert_eq!(
        rows[3].parent_timeline_id.as_deref(),
        Some("feature-window.feature-window-1")
    );
    assert_eq!(rows[4].stage, ObservabilityStage::CandidateCorrection);
    assert_eq!(
        rows[4].parent_timeline_id.as_deref(),
        Some("activity-candidate.candidate-1")
    );
    assert_eq!(rows[5].stage, ObservabilityStage::PromotedSession);
    assert_eq!(
        rows[5].parent_timeline_id.as_deref(),
        Some("promoted-session.session-1")
    );
    assert_eq!(rows[6].stage, ObservabilityStage::DisplayedStats);
    assert_eq!(
        rows[6].parent_timeline_id.as_deref(),
        Some("promoted-session.session-1")
    );
    assert_eq!(rows[7].stage, ObservabilityStage::ExportPlanning);
    assert_eq!(
        rows[7].parent_timeline_id.as_deref(),
        Some("promoted-session.session-1")
    );
    assert_eq!(rows[8].stage, ObservabilityStage::ExportCompleted);
    assert_eq!(
        rows[8].parent_timeline_id.as_deref(),
        Some("promoted-session.session-1")
    );
    assert_eq!(rows[9].stage, ObservabilityStage::HealthSyncPlanning);
    assert_eq!(
        rows[9].topic.as_deref(),
        Some(DEBUG_EVENT_TOPIC_HEALTH_SYNC_ACTIVITY_PLANNED)
    );
    assert_eq!(rows[9].title, "Health sync plan health-plan-1");
    assert_eq!(rows[10].stage, ObservabilityStage::HealthSyncPlanning);
    assert_eq!(
        rows[10].topic.as_deref(),
        Some(DEBUG_EVENT_TOPIC_HEALTH_SYNC_ACTIVITY_BLOCKED)
    );
    assert_eq!(rows[10].title, "Health sync blocked health-plan-2");
}

#[test]
fn observability_timeline_threads_capture_session_story_and_imports() {
    let raw_row = RawEvidenceRow {
        evidence_id: "synthetic.raw.capture-live-1".to_string(),
        source: "ios.corebluetooth.notification".to_string(),
        captured_at: "2026-05-27T06:00:01Z".to_string(),
        device_model: "WHOOP 5.0 Bull".to_string(),
        payload_hex: "deadbeef".to_string(),
        sha256: "sha256-raw-2".to_string(),
        sensitivity: "user-owned-live-notification".to_string(),
        capture_session_id: Some("capture-live-1".to_string()),
    };
    let packet_row = PacketTimelineRow {
        timeline_id: "synthetic.frame.capture-live-1.timeline".to_string(),
        frame_id: "synthetic.frame.capture-live-1".to_string(),
        evidence_id: raw_row.evidence_id.clone(),
        captured_at: "2026-05-27T06:00:02Z".to_string(),
        category: "data_packet".to_string(),
        title: "Data packet normal_history".to_string(),
        packet_type_name: Some("HISTORICAL_DATA".to_string()),
        sequence: Some(7),
        command_or_event: Some(18),
        device_timestamp_seconds: Some(287454020),
        device_timestamp_subseconds: Some(123),
        body_hex: Some("cafe".to_string()),
        summary: serde_json::json!({
            "packet_k": 18,
            "body_summary": {
                "kind": "normal_history"
            }
        }),
        warnings: vec![],
    };
    let debug_rows = vec![
        capture_session_story_event(
            "capture-live-1",
            1,
            1779840000000,
            "app",
            "info",
            "capture.session.scan",
            "scan",
            "scan start",
            "device scan started",
        ),
        capture_session_story_event(
            "capture-live-1",
            2,
            1779840000100,
            "device",
            "info",
            "capture.session.connect",
            "connect",
            "connect device",
            "device connected",
        ),
        capture_session_story_event(
            "capture-live-1",
            3,
            1779840000200,
            "app",
            "info",
            "capture.session.subscribe",
            "subscribe",
            "subscribe notifications",
            "notifications subscribed",
        ),
        capture_session_story_event(
            "capture-live-1",
            4,
            1779840000300,
            "ble",
            "info",
            "capture.session.import_notifications",
            "import_notifications",
            "import recent notifications",
            "recent notifications imported",
        ),
        capture_session_story_event(
            "capture-live-1",
            5,
            1779840000400,
            "capture",
            "info",
            "capture.session.import_capture_file",
            "import_capture_file",
            "import capture file",
            "capture file imported",
        ),
    ];

    let rows = observability_timeline_from_rows(&[raw_row], &[packet_row], &debug_rows).unwrap();

    assert_eq!(rows.len(), 7, "{rows:#?}");
    assert_eq!(rows[0].stage, ObservabilityStage::CaptureSession);
    assert_eq!(rows[0].timeline_id, "capture-session.capture-live-1.scan");
    assert_eq!(rows[0].parent_timeline_id, None);
    assert_eq!(
        rows[0].capture_session_id.as_deref(),
        Some("capture-live-1")
    );
    assert_eq!(rows[0].capture_session_action_key.as_deref(), Some("scan"));
    assert_eq!(rows[1].stage, ObservabilityStage::CaptureSession);
    assert_eq!(
        rows[1].timeline_id,
        "capture-session.capture-live-1.connect"
    );
    assert_eq!(
        rows[1].parent_timeline_id.as_deref(),
        Some("capture-session.capture-live-1.scan")
    );
    assert_eq!(rows[2].stage, ObservabilityStage::CaptureSession);
    assert_eq!(
        rows[2].timeline_id,
        "capture-session.capture-live-1.subscribe"
    );
    assert_eq!(
        rows[2].parent_timeline_id.as_deref(),
        Some("capture-session.capture-live-1.connect")
    );
    assert_eq!(rows[3].stage, ObservabilityStage::CaptureSession);
    assert_eq!(
        rows[3].timeline_id,
        "capture-session.capture-live-1.import_notifications"
    );
    assert_eq!(
        rows[3].parent_timeline_id.as_deref(),
        Some("capture-session.capture-live-1.subscribe")
    );
    assert_eq!(rows[4].stage, ObservabilityStage::CaptureSession);
    assert_eq!(
        rows[4].timeline_id,
        "capture-session.capture-live-1.import_capture_file"
    );
    assert_eq!(
        rows[4].parent_timeline_id.as_deref(),
        Some("capture-session.capture-live-1.import_notifications")
    );

    let raw = rows
        .iter()
        .find(|row| row.timeline_id == "raw.synthetic.raw.capture-live-1")
        .unwrap();
    assert_eq!(raw.stage, ObservabilityStage::RawFrame);
    assert_eq!(
        raw.parent_timeline_id.as_deref(),
        Some("capture-session.capture-live-1.scan")
    );
    assert_eq!(raw.capture_session_id.as_deref(), Some("capture-live-1"));

    let decoded = rows
        .iter()
        .find(|row| row.timeline_id == "synthetic.frame.capture-live-1.timeline")
        .unwrap();
    assert_eq!(decoded.stage, ObservabilityStage::DecodedPacket);
    assert_eq!(
        decoded.parent_timeline_id.as_deref(),
        Some("raw.synthetic.raw.capture-live-1")
    );
    assert_eq!(
        decoded.capture_session_id.as_deref(),
        Some("capture-live-1")
    );
}

#[test]
fn timeline_reports_malformed_decoded_payload_json() {
    let rows = vec![DecodedFrameRow {
        frame_id: "bad-frame".to_string(),
        evidence_id: "evidence-1".to_string(),
        captured_at: "2026-05-27T00:00:00Z".to_string(),
        device_type: "BULL".to_string(),
        raw_len: 1,
        header_len: 1,
        declared_len: 1,
        payload_hex: "ff".to_string(),
        payload_crc_hex: String::new(),
        header_crc_valid: true,
        payload_crc_valid: true,
        packet_type: Some(255),
        packet_type_name: None,
        sequence: None,
        command_or_event: None,
        parsed_payload_json: "{not-json".to_string(),
        parser_version: "test".to_string(),
        warnings_json: "[]".to_string(),
    }];

    let error = packet_timeline_from_decoded_frames(&rows).unwrap_err();

    assert!(error.to_string().contains("bad-frame parsed_payload_json"));
}

fn capture_session_story_event(
    session_id: &str,
    sequence: i64,
    time_unix_ms: i64,
    source: &str,
    level: &str,
    topic: &str,
    action_key: &str,
    action_label: &str,
    message: &str,
) -> DebugEventRow {
    DebugEventRow {
        session_id: "debug-capture-session".to_string(),
        sequence,
        schema: "bull.debug.event.v1".to_string(),
        time_unix_ms,
        source: source.to_string(),
        level: level.to_string(),
        topic: topic.to_string(),
        message: message.to_string(),
        command_id: None,
        data_json: serde_json::json!({
            "capture_session_id": session_id,
            "capture_session_action_key": action_key,
            "capture_session_action": action_label,
            "capture_session_story_event_count": sequence,
        })
        .to_string(),
    }
}
