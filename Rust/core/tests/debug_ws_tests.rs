use bull_core::{
    debug_ws::{
        DEBUG_COMMAND_SCHEMA, DEBUG_EVENT_TOPIC_ACTIVITY_CANDIDATE_CORRECTED,
        DEBUG_EVENT_TOPIC_ACTIVITY_CANDIDATE_CREATED,
        DEBUG_EVENT_TOPIC_ACTIVITY_CANDIDATE_PROMOTED,
        DEBUG_EVENT_TOPIC_ACTIVITY_FEATURE_WINDOW_CREATED,
        DEBUG_EVENT_TOPIC_ACTIVITY_SESSION_STATS_DISPLAYED,
        DEBUG_EVENT_TOPIC_EXPORT_RAW_TIMEFRAME_COMPLETED,
        DEBUG_EVENT_TOPIC_EXPORT_RAW_TIMEFRAME_PLANNED,
        DEBUG_EVENT_TOPIC_HEALTH_SYNC_ACTIVITY_BLOCKED,
        DEBUG_EVENT_TOPIC_HEALTH_SYNC_ACTIVITY_PLANNED, DebugBridgeConfig, DebugCommandEnvelope,
        DebugCommandFinishInput, DebugCommandStartInput, DebugEventInput, DebugSessionStartInput,
        DebugWsContractInput, append_debug_event, finish_debug_command, start_debug_command,
        start_debug_session, validate_debug_ws_contract,
    },
    debug_ws_server::{DebugWsServerOptions, bind_debug_ws_listener, serve_debug_ws_listener_once},
    store::BullStore,
};
use serde_json::json;
use std::thread;

#[test]
fn valid_debug_ws_contract_fixture_passes() {
    let input = valid_input();
    let report = validate_debug_ws_contract(&input);

    assert!(report.pass, "{:?}", report.issues);
    assert!(report.input_valid);
    assert!(report.bridge_valid);
    assert!(report.commands_valid);
    assert!(report.events_valid);
    assert!(report.stream_order_valid);
    assert!(report.command_references_valid);
    assert!(report.command_results_correlated);
    assert!(report.contract_ready);
    assert_eq!(report.command_count, 2);
    assert_eq!(report.event_count, 6);
    assert!(report.command_results.iter().all(|result| result.started));
    assert!(report.command_results.iter().all(|result| result.result));
    assert!(report.next_actions.is_empty());
}

#[test]
fn missing_session_token_fails_closed() {
    let mut input = valid_input();
    input.bridge.token_present = false;

    let report = validate_debug_ws_contract(&input);

    assert!(!report.pass);
    assert!(report.input_valid);
    assert!(!report.bridge_valid);
    assert!(report.commands_valid);
    assert!(report.events_valid);
    assert!(report.stream_order_valid);
    assert!(report.command_references_valid);
    assert!(report.command_results_correlated);
    assert!(!report.contract_ready);
    assert!(report.issues.contains(&"bridge_token_missing".to_string()));
    assert!(report.next_actions.iter().any(|action| {
        action.scope == "debug_ws"
            && action.reason == "bridge_token_missing"
            && action.action.contains("Generate and attach")
    }));
}

#[test]
fn non_loopback_bind_is_rejected() {
    let mut input = valid_input();
    input.bridge.bind_host = "0.0.0.0".to_string();

    let report = validate_debug_ws_contract(&input);

    assert!(!report.pass);
    assert!(!report.bridge_valid);
    assert!(!report.contract_ready);
    assert!(
        report
            .issues
            .contains(&"bridge_bind_host_must_be_loopback".to_string())
    );
}

#[test]
fn event_sequences_must_strictly_increase() {
    let mut input = valid_input();
    input.events[2].sequence = input.events[1].sequence;

    let report = validate_debug_ws_contract(&input);

    assert!(!report.pass);
    assert!(!report.stream_order_valid);
    assert!(!report.contract_ready);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.starts_with("event_sequence_not_strictly_increasing"))
    );
    assert!(report.next_actions.iter().any(|action| {
        action.reason == "event_sequence_not_strictly_increasing"
            && action.action.contains("sequence numbers strictly increase")
    }));
}

#[test]
fn event_times_must_not_move_backwards() {
    let mut input = valid_input();
    input.events[3].time_unix_ms = input.events[2].time_unix_ms - 1;

    let report = validate_debug_ws_contract(&input);

    assert!(!report.pass);
    assert!(!report.stream_order_valid);
    assert!(!report.contract_ready);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.starts_with("event_time_decreased"))
    );
}

#[test]
fn every_command_requires_started_and_result_events() {
    let mut input = valid_input();
    input
        .events
        .retain(|event| event.command_id.as_deref() != Some("cmd-export-bundle"));

    let report = validate_debug_ws_contract(&input);

    assert!(!report.pass);
    assert!(!report.command_results_correlated);
    assert!(!report.contract_ready);
    assert!(
        report
            .issues
            .contains(&"command_missing_started_event:cmd-export-bundle".to_string())
    );
    assert!(
        report
            .issues
            .contains(&"command_missing_result_event:cmd-export-bundle".to_string())
    );
    assert!(report.next_actions.iter().any(|action| {
        action.scope == "cmd-export-bundle"
            && action.reason == "command_missing_result_event"
            && action.action.contains("debug.finish_command")
    }));
}

#[test]
fn debug_lifecycle_event_source_is_allowed() {
    let mut input = valid_input();
    let mut event = input.events[0].clone();
    event.sequence = 7;
    event.time_unix_ms = 1779840000400;
    event.source = "debug".to_string();
    event.topic = "debug.ws.started".to_string();
    event.command_id = None;
    input.events.push(event);

    let report = validate_debug_ws_contract(&input);

    assert!(report.pass, "{:?}", report.issues);
}

#[test]
fn command_events_must_reference_known_commands() {
    let mut input = valid_input();
    let mut event = input.events[0].clone();
    event.sequence = 7;
    event.time_unix_ms = 1779840000400;
    event.command_id = Some("cmd-unknown".to_string());
    input.events.push(event);

    let report = validate_debug_ws_contract(&input);

    assert!(!report.pass);
    assert!(!report.command_references_valid);
    assert!(!report.contract_ready);
    assert!(
        report
            .issues
            .contains(&"command_event_unknown_command_id:cmd-unknown".to_string())
    );
}

#[test]
fn command_args_and_event_data_must_be_objects() {
    let mut input = valid_input();
    input.commands[0].args = json!(null);
    input.events[0].data = json!(["not", "an", "object"]);

    let report = validate_debug_ws_contract(&input);

    assert!(!report.pass);
    assert!(!report.commands_valid);
    assert!(!report.events_valid);
    assert!(!report.contract_ready);
    assert!(
        report
            .issues
            .contains(&"command_args_must_be_object:cmd-capture-start".to_string())
    );
    assert!(
        report
            .issues
            .contains(&"event_data_must_be_object:1".to_string())
    );
}

#[test]
fn debug_session_store_records_command_lifecycle_and_valid_snapshot() {
    let store = BullStore::open_in_memory().unwrap();
    let bridge = valid_input().bridge;

    let empty_snapshot = start_debug_session(
        &store,
        &DebugSessionStartInput {
            session_id: "debug-session-store".to_string(),
            started_at_unix_ms: 1779840000000,
            bridge,
        },
    )
    .unwrap();
    assert!(empty_snapshot.contract_report.pass);
    assert!(empty_snapshot.contract_report.contract_ready);
    assert_eq!(empty_snapshot.commands.len(), 0);
    assert_eq!(empty_snapshot.events.len(), 0);

    let started_snapshot = start_debug_command(
        &store,
        &DebugCommandStartInput {
            session_id: "debug-session-store".to_string(),
            received_at_unix_ms: 1779840000100,
            command: DebugCommandEnvelope {
                schema: DEBUG_COMMAND_SCHEMA.to_string(),
                command_id: "cmd-storage-check".to_string(),
                command: "storage.check".to_string(),
                args: json!({"self_test": true}),
                dry_run: true,
            },
        },
    )
    .unwrap();
    assert!(!started_snapshot.contract_report.pass);
    assert!(!started_snapshot.contract_report.command_results_correlated);
    assert!(!started_snapshot.contract_report.contract_ready);
    assert!(
        started_snapshot
            .contract_report
            .issues
            .contains(&"command_missing_result_event:cmd-storage-check".to_string())
    );
    assert_eq!(started_snapshot.events[0].sequence, 1);
    assert_eq!(started_snapshot.events[0].topic, "command.started");

    let app_event = append_debug_event(
        &store,
        &DebugEventInput {
            session_id: "debug-session-store".to_string(),
            time_unix_ms: 1779840000150,
            source: "sqlite".to_string(),
            level: "debug".to_string(),
            topic: "storage.rows.counted".to_string(),
            message: "storage check counted rows".to_string(),
            command_id: None,
            data: json!({"raw_evidence": 3}),
        },
    )
    .unwrap();
    assert_eq!(app_event.sequence, 2);

    let finished_snapshot = finish_debug_command(
        &store,
        &DebugCommandFinishInput {
            session_id: "debug-session-store".to_string(),
            time_unix_ms: 1779840000200,
            command_id: "cmd-storage-check".to_string(),
            ok: true,
            message: "storage.check completed".to_string(),
            data: json!({"tables": 14}),
        },
    )
    .unwrap();

    assert!(finished_snapshot.contract_report.pass);
    assert!(finished_snapshot.contract_report.command_results_correlated);
    assert!(finished_snapshot.contract_report.contract_ready);
    assert_eq!(finished_snapshot.commands.len(), 1);
    assert_eq!(finished_snapshot.events.len(), 3);
    assert_eq!(finished_snapshot.events[2].sequence, 3);
    assert_eq!(finished_snapshot.events[2].topic, "command.result");
    assert_eq!(finished_snapshot.events[2].data["ok"], true);
}

#[test]
fn debug_session_store_records_structured_activity_export_and_health_sync_events() {
    let store = BullStore::open_in_memory().unwrap();
    let bridge = valid_input().bridge;

    start_debug_session(
        &store,
        &DebugSessionStartInput {
            session_id: "debug-session-structured".to_string(),
            started_at_unix_ms: 1779840000000,
            bridge,
        },
    )
    .unwrap();

    append_debug_event(
        &store,
        &DebugEventInput {
            session_id: "debug-session-structured".to_string(),
            time_unix_ms: 1779840000100,
            source: "rust".to_string(),
            level: "debug".to_string(),
            topic: DEBUG_EVENT_TOPIC_ACTIVITY_FEATURE_WINDOW_CREATED.to_string(),
            message: "feature window built".to_string(),
            command_id: None,
            data: json!({
                "raw_evidence_id": "synthetic.raw.evidence-1",
                "frame_id": "synthetic.frame-1",
                "feature_window_id": "feature-window-1",
                "window_id": "feature-window-1"
            }),
        },
    )
    .unwrap();
    append_debug_event(
        &store,
        &DebugEventInput {
            session_id: "debug-session-structured".to_string(),
            time_unix_ms: 1779840000200,
            source: "rust".to_string(),
            level: "info".to_string(),
            topic: DEBUG_EVENT_TOPIC_ACTIVITY_CANDIDATE_CREATED.to_string(),
            message: "activity candidate created".to_string(),
            command_id: None,
            data: json!({
                "feature_window_id": "feature-window-1",
                "candidate_id": "candidate-1",
                "activity_type": "running",
                "confidence_0_to_1": 0.91
            }),
        },
    )
    .unwrap();
    append_debug_event(
        &store,
        &DebugEventInput {
            session_id: "debug-session-structured".to_string(),
            time_unix_ms: 1779840000300,
            source: "rust".to_string(),
            level: "info".to_string(),
            topic: DEBUG_EVENT_TOPIC_ACTIVITY_CANDIDATE_PROMOTED.to_string(),
            message: "activity candidate promoted".to_string(),
            command_id: None,
            data: json!({
                "candidate_id": "candidate-1",
                "activity_session_id": "session-1",
                "stat_id": "displayed-stat-1"
            }),
        },
    )
    .unwrap();
    append_debug_event(
        &store,
        &DebugEventInput {
            session_id: "debug-session-structured".to_string(),
            time_unix_ms: 1779840000350,
            source: "rust".to_string(),
            level: "warn".to_string(),
            topic: DEBUG_EVENT_TOPIC_ACTIVITY_CANDIDATE_CORRECTED.to_string(),
            message: "activity candidate corrected".to_string(),
            command_id: None,
            data: json!({
                "candidate_id": "candidate-1",
                "activity_session_id": "session-1",
                "correction": "activity_type->jogging"
            }),
        },
    )
    .unwrap();
    append_debug_event(
        &store,
        &DebugEventInput {
            session_id: "debug-session-structured".to_string(),
            time_unix_ms: 1779840000400,
            source: "metric".to_string(),
            level: "info".to_string(),
            topic: DEBUG_EVENT_TOPIC_ACTIVITY_SESSION_STATS_DISPLAYED.to_string(),
            message: "activity stats displayed".to_string(),
            command_id: None,
            data: json!({
                "activity_session_id": "session-1",
                "stat_id": "displayed-stat-1",
                "metric_name": "strain",
                "value": 12.3,
                "unit": "load"
            }),
        },
    )
    .unwrap();
    append_debug_event(
        &store,
        &DebugEventInput {
            session_id: "debug-session-structured".to_string(),
            time_unix_ms: 1779840000500,
            source: "sqlite".to_string(),
            level: "debug".to_string(),
            topic: DEBUG_EVENT_TOPIC_EXPORT_RAW_TIMEFRAME_PLANNED.to_string(),
            message: "export raw timeframe planned".to_string(),
            command_id: None,
            data: json!({
                "export_job_id": "export-1",
                "raw_evidence_rows": 2,
                "decoded_frame_rows": 1,
                "packet_timeline_rows": 1
            }),
        },
    )
    .unwrap();
    append_debug_event(
        &store,
        &DebugEventInput {
            session_id: "debug-session-structured".to_string(),
            time_unix_ms: 1779840000550,
            source: "sqlite".to_string(),
            level: "info".to_string(),
            topic: DEBUG_EVENT_TOPIC_EXPORT_RAW_TIMEFRAME_COMPLETED.to_string(),
            message: "export raw timeframe completed".to_string(),
            command_id: None,
            data: json!({
                "export_job_id": "export-1",
                "row_count": 3,
                "bundle_path": "export.bullbundle"
            }),
        },
    )
    .unwrap();
    append_debug_event(
        &store,
        &DebugEventInput {
            session_id: "debug-session-structured".to_string(),
            time_unix_ms: 1779840000600,
            source: "rust".to_string(),
            level: "info".to_string(),
            topic: DEBUG_EVENT_TOPIC_HEALTH_SYNC_ACTIVITY_PLANNED.to_string(),
            message: "health sync activity planned".to_string(),
            command_id: None,
            data: json!({
                "plan_id": "health-plan-1",
                "activity_session_id": "session-1",
                "platform": "health_kit",
                "planned_session_count": 1
            }),
        },
    )
    .unwrap();
    append_debug_event(
        &store,
        &DebugEventInput {
            session_id: "debug-session-structured".to_string(),
            time_unix_ms: 1779840000700,
            source: "rust".to_string(),
            level: "warn".to_string(),
            topic: DEBUG_EVENT_TOPIC_HEALTH_SYNC_ACTIVITY_BLOCKED.to_string(),
            message: "health sync activity blocked".to_string(),
            command_id: None,
            data: json!({
                "plan_id": "health-plan-2",
                "activity_session_id": "session-2",
                "blocked_session_count": 1
            }),
        },
    )
    .unwrap();

    let snapshot =
        bull_core::debug_ws::debug_session_snapshot(&store, "debug-session-structured").unwrap();

    assert!(
        snapshot.contract_report.pass,
        "{:?}",
        snapshot.contract_report.issues
    );
    assert_eq!(snapshot.events.len(), 9);
    assert_eq!(
        snapshot.events[0].topic,
        DEBUG_EVENT_TOPIC_ACTIVITY_FEATURE_WINDOW_CREATED
    );
    assert_eq!(
        snapshot.events[1].topic,
        DEBUG_EVENT_TOPIC_ACTIVITY_CANDIDATE_CREATED
    );
    assert_eq!(
        snapshot.events[2].topic,
        DEBUG_EVENT_TOPIC_ACTIVITY_CANDIDATE_PROMOTED
    );
    assert_eq!(
        snapshot.events[5].topic,
        DEBUG_EVENT_TOPIC_EXPORT_RAW_TIMEFRAME_PLANNED
    );
    assert_eq!(
        snapshot.events[6].topic,
        DEBUG_EVENT_TOPIC_EXPORT_RAW_TIMEFRAME_COMPLETED
    );
    assert_eq!(
        snapshot.events[7].topic,
        DEBUG_EVENT_TOPIC_HEALTH_SYNC_ACTIVITY_PLANNED
    );
    assert_eq!(
        snapshot.events[8].topic,
        DEBUG_EVENT_TOPIC_HEALTH_SYNC_ACTIVITY_BLOCKED
    );
}

#[test]
fn debug_session_store_rejects_invalid_bridge_and_event_shapes() {
    let store = BullStore::open_in_memory().unwrap();
    let mut bridge = valid_input().bridge;
    bridge.remote_bind_enabled = true;
    bridge.visible_remote_bind_toggle = false;

    let invalid = start_debug_session(
        &store,
        &DebugSessionStartInput {
            session_id: "debug-session-invalid".to_string(),
            started_at_unix_ms: 1779840000000,
            bridge,
        },
    );
    assert!(invalid.is_err());
    assert!(
        invalid
            .unwrap_err()
            .to_string()
            .contains("remote_bind_requires_visible_toggle")
    );

    let bridge = valid_input().bridge;
    start_debug_session(
        &store,
        &DebugSessionStartInput {
            session_id: "debug-session-invalid".to_string(),
            started_at_unix_ms: 1779840000000,
            bridge,
        },
    )
    .unwrap();
    let invalid_event = append_debug_event(
        &store,
        &DebugEventInput {
            session_id: "debug-session-invalid".to_string(),
            time_unix_ms: 1779840000100,
            source: "app".to_string(),
            level: "info".to_string(),
            topic: "app.ready".to_string(),
            message: "app ready".to_string(),
            command_id: None,
            data: json!(["not", "an", "object"]),
        },
    );
    assert!(invalid_event.is_err());
    assert!(
        invalid_event
            .unwrap_err()
            .to_string()
            .contains("JSON object")
    );
}

#[test]
fn debug_session_store_preserves_monotonic_stream_order() {
    let store = BullStore::open_in_memory().unwrap();
    start_debug_session(
        &store,
        &DebugSessionStartInput {
            session_id: "debug-session-order".to_string(),
            started_at_unix_ms: 1779840000000,
            bridge: valid_input().bridge,
        },
    )
    .unwrap();
    append_debug_event(
        &store,
        &DebugEventInput {
            session_id: "debug-session-order".to_string(),
            time_unix_ms: 1779840000100,
            source: "app".to_string(),
            level: "info".to_string(),
            topic: "app.ready".to_string(),
            message: "app ready".to_string(),
            command_id: None,
            data: json!({}),
        },
    )
    .unwrap();

    let backwards = append_debug_event(
        &store,
        &DebugEventInput {
            session_id: "debug-session-order".to_string(),
            time_unix_ms: 1779840000099,
            source: "app".to_string(),
            level: "info".to_string(),
            topic: "app.backwards".to_string(),
            message: "time moved backwards".to_string(),
            command_id: None,
            data: json!({}),
        },
    );

    assert!(backwards.is_err());
    assert!(
        backwards
            .unwrap_err()
            .to_string()
            .contains("before previous event time")
    );
}

#[test]
fn debug_ws_server_streams_persisted_events_over_loopback_websocket() {
    let tempdir = tempfile::tempdir().unwrap();
    let db = tempdir.path().join("bull.sqlite");
    let options = DebugWsServerOptions {
        database_path: db.clone(),
        session_id: "debug-session-ws".to_string(),
        bind_host: "127.0.0.1".to_string(),
        port: 0,
        token: "test-token".to_string(),
        poll_interval_ms: 10,
        idle_timeout_ms: 500,
        max_events: Some(2),
    };
    let listener = bind_debug_ws_listener(&options).unwrap();
    let port = listener.local_addr().unwrap().port();
    let url = format!("ws://127.0.0.1:{port}/bull-debug/stream?token=test-token");

    let store = BullStore::open(&db).unwrap();
    start_debug_session(
        &store,
        &DebugSessionStartInput {
            session_id: "debug-session-ws".to_string(),
            started_at_unix_ms: 1779840000000,
            bridge: DebugBridgeConfig {
                url: url.clone(),
                bind_host: "127.0.0.1".to_string(),
                token_required: true,
                token_present: true,
                remote_bind_enabled: false,
                visible_remote_bind_toggle: false,
            },
        },
    )
    .unwrap();
    append_debug_event(
        &store,
        &DebugEventInput {
            session_id: "debug-session-ws".to_string(),
            time_unix_ms: 1779840000100,
            source: "app".to_string(),
            level: "info".to_string(),
            topic: "app.ready".to_string(),
            message: "app ready".to_string(),
            command_id: None,
            data: json!({"phase": "first"}),
        },
    )
    .unwrap();
    append_debug_event(
        &store,
        &DebugEventInput {
            session_id: "debug-session-ws".to_string(),
            time_unix_ms: 1779840000200,
            source: "rust".to_string(),
            level: "debug".to_string(),
            topic: "parser.ready".to_string(),
            message: "parser ready".to_string(),
            command_id: None,
            data: json!({"phase": "second"}),
        },
    )
    .unwrap();
    drop(store);

    let server_options = options.clone();
    let server = thread::spawn(move || serve_debug_ws_listener_once(listener, server_options));
    let (mut socket, _) = tungstenite::connect(url.as_str()).unwrap();
    let first = socket.read().unwrap().into_text().unwrap();
    let second = socket.read().unwrap().into_text().unwrap();
    let first_event: serde_json::Value = serde_json::from_str(&first).unwrap();
    let second_event: serde_json::Value = serde_json::from_str(&second).unwrap();

    assert_eq!(first_event["schema"], "bull.debug.event.v1");
    assert_eq!(first_event["sequence"], 1);
    assert_eq!(first_event["topic"], "app.ready");
    assert_eq!(second_event["sequence"], 2);
    assert_eq!(second_event["topic"], "parser.ready");

    let report = server.join().unwrap().unwrap();
    assert!(report.pass, "{:?}", report.issues);
    assert!(report.server_valid);
    assert!(report.handshake_accepted);
    assert!(report.session_found);
    assert!(report.stream_observed);
    assert_eq!(report.completion_reason, "max_events_reached");
    assert_eq!(report.events_sent, 2);
    assert_eq!(report.last_sequence, 2);
}

#[test]
fn debug_ws_server_reports_empty_stream_as_validation_gap() {
    let tempdir = tempfile::tempdir().unwrap();
    let db = tempdir.path().join("bull.sqlite");
    let options = DebugWsServerOptions {
        database_path: db.clone(),
        session_id: "debug-session-ws-empty".to_string(),
        bind_host: "127.0.0.1".to_string(),
        port: 0,
        token: "empty-token".to_string(),
        poll_interval_ms: 10,
        idle_timeout_ms: 50,
        max_events: None,
    };
    let listener = bind_debug_ws_listener(&options).unwrap();
    let port = listener.local_addr().unwrap().port();
    let url = format!("ws://127.0.0.1:{port}/bull-debug/stream?token=empty-token");

    let store = BullStore::open(&db).unwrap();
    start_debug_session(
        &store,
        &DebugSessionStartInput {
            session_id: "debug-session-ws-empty".to_string(),
            started_at_unix_ms: 1779840000000,
            bridge: DebugBridgeConfig {
                url: url.clone(),
                bind_host: "127.0.0.1".to_string(),
                token_required: true,
                token_present: true,
                remote_bind_enabled: false,
                visible_remote_bind_toggle: false,
            },
        },
    )
    .unwrap();
    drop(store);

    let server_options = options.clone();
    let server = thread::spawn(move || serve_debug_ws_listener_once(listener, server_options));
    let (_socket, _) = tungstenite::connect(url.as_str()).unwrap();
    let report = server.join().unwrap().unwrap();

    assert!(!report.pass);
    assert!(report.server_valid);
    assert!(report.handshake_accepted);
    assert!(report.session_found);
    assert!(!report.stream_observed);
    assert_eq!(report.completion_reason, "idle_timeout");
    assert_eq!(report.events_sent, 0);
    assert!(
        report
            .issues
            .contains(&"debug_event_stream_empty".to_string())
    );
    assert!(report.next_actions.iter().any(|action| {
        action.scope == "debug_events"
            && action.reason == "debug_event_stream_empty"
            && action
                .action
                .contains("Record app, BLE, parser, or command debug events")
    }));
}

#[test]
fn debug_ws_server_rejects_missing_or_wrong_token_at_handshake() {
    let tempdir = tempfile::tempdir().unwrap();
    let db = tempdir.path().join("bull.sqlite");
    let options = DebugWsServerOptions {
        database_path: db,
        session_id: "debug-session-ws-token".to_string(),
        bind_host: "127.0.0.1".to_string(),
        port: 0,
        token: "correct-token".to_string(),
        poll_interval_ms: 10,
        idle_timeout_ms: 100,
        max_events: Some(1),
    };
    let listener = bind_debug_ws_listener(&options).unwrap();
    let port = listener.local_addr().unwrap().port();
    let bad_url = format!("ws://127.0.0.1:{port}/bull-debug/stream?token=wrong-token");
    let server_options = options.clone();
    let server = thread::spawn(move || serve_debug_ws_listener_once(listener, server_options));

    assert!(tungstenite::connect(bad_url.as_str()).is_err());
    let report = server.join().unwrap().unwrap();
    assert!(!report.pass);
    assert!(!report.server_valid);
    assert!(!report.handshake_accepted);
    assert!(!report.session_found);
    assert!(!report.stream_observed);
    assert_eq!(report.completion_reason, "handshake_failed");
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.contains("websocket handshake failed"))
    );
    assert!(report.next_actions.iter().any(|action| {
        action.scope == "debug_ws_handshake"
            && action.reason == "websocket_handshake_failed"
            && action
                .action
                .contains("path /bull-debug/stream and the current per-session token")
    }));
}

fn valid_input() -> DebugWsContractInput {
    serde_json::from_str(include_str!(
        "../fixtures/synthetic/debug_ws_contract_valid.json"
    ))
    .unwrap()
}
