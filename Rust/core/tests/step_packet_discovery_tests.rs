use bull_core::{
    step_discovery::{
        StepCaptureValidationOptions, StepPacketDiscoveryOptions, run_step_capture_validation,
        run_step_packet_discovery,
    },
    store::DecodedFrameRow,
};
use serde_json::json;

#[test]
fn step_packet_discovery_promotes_explicit_decoded_step_counter() {
    let rows = vec![decoded_frame_row(
        "step-frame-1",
        "2026-06-02T12:00:00Z",
        "HISTORICAL_DATA",
        json!({
            "kind": "data_packet",
            "packet_k": 11,
            "domain": "raw_stream_counted",
            "body_summary": {
                "kind": "raw_stream_counted",
                "step_count": 4120,
                "cadence": 98,
                "activity": 2
            },
            "warnings": []
        }),
    )];

    let report = run_step_packet_discovery(
        &rows,
        "synthetic.sqlite",
        "2026-06-02T00:00:00Z",
        "2026-06-03T00:00:00Z",
        StepPacketDiscoveryOptions::default(),
    )
    .unwrap();

    assert_eq!(report.schema, "bull.step-packet-discovery-report.v1");
    assert!(report.pass, "{:?}", report.issues);
    assert!(report.explicit_step_counter_found);
    assert_eq!(report.decoded_frame_count, 1);
    assert_eq!(report.inspected_frame_count, 1);
    assert_eq!(report.candidate_field_count, 3);
    assert_eq!(
        report
            .inspected_packet_family_counts
            .get("K11/raw_stream_counted"),
        Some(&1)
    );
    let step = report
        .candidate_fields
        .iter()
        .find(|field| field.field_name == "step_count")
        .expect("step_count candidate");
    assert_eq!(step.match_kind, "step_count");
    assert_eq!(step.source_kind_inference, "device_counter");
    assert_eq!(step.json_path, "$.body_summary.step_count");
}

#[test]
fn step_packet_discovery_promotes_v18_step_motion_counter() {
    // The v18 historical body the device exposes over Bluetooth carries a
    // step_motion_counter; the generic field scanner should classify it as an
    // explicit step counter (key contains "step") without any v18-specific code.
    let rows = vec![decoded_frame_row(
        "v18-frame-1",
        "2026-06-02T12:00:00Z",
        "HISTORICAL_DATA",
        json!({
            "kind": "data_packet",
            "packet_k": 18,
            "domain": "normal_history_with_hr_marker",
            "body_summary": {
                "kind": "v18_history",
                "hr": 75,
                "rr_intervals_ms": [900, 950],
                "gravity_x": 0.1,
                "gravity_y": 0.2,
                "gravity_z": 9.8,
                "skin_temp_raw": 4096,
                "step_motion_counter": 42,
                "warnings": []
            },
            "warnings": []
        }),
    )];

    let report = run_step_packet_discovery(
        &rows,
        "synthetic.sqlite",
        "2026-06-02T00:00:00Z",
        "2026-06-03T00:00:00Z",
        StepPacketDiscoveryOptions::default(),
    )
    .unwrap();

    assert!(report.pass, "{:?}", report.issues);
    assert!(report.explicit_step_counter_found);
    let step = report
        .candidate_fields
        .iter()
        .find(|field| field.field_name == "step_motion_counter")
        .expect("step_motion_counter candidate");
    assert_eq!(step.match_kind, "step_count");
    assert_eq!(step.source_kind_inference, "device_counter");
    assert_eq!(step.json_path, "$.body_summary.step_motion_counter");
}

#[test]
fn step_packet_discovery_blocks_when_motion_decode_exposes_no_pedometer_fields() {
    let rows = vec![decoded_frame_row(
        "motion-frame-1",
        "2026-06-02T12:00:00Z",
        "REALTIME_RAW_DATA",
        json!({
            "kind": "data_packet",
            "packet_k": 10,
            "domain": "raw_motion_stream_result",
            "body_summary": {
                "kind": "raw_motion_k10",
                "heart_rate": 72
            },
            "warnings": []
        }),
    )];

    let report = run_step_packet_discovery(
        &rows,
        "synthetic.sqlite",
        "2026-06-02T00:00:00Z",
        "2026-06-03T00:00:00Z",
        StepPacketDiscoveryOptions::default(),
    )
    .unwrap();

    assert!(!report.pass);
    assert!(!report.explicit_step_counter_found);
    assert_eq!(report.inspected_frame_count, 1);
    assert_eq!(report.candidate_field_count, 0);
    assert!(
        report
            .issues
            .contains(&"no_step_or_pedometer_fields_in_decoded_frames".to_string())
    );
    assert!(
        report
            .issues
            .contains(&"no_explicit_step_counter_field_found".to_string())
    );
}

#[test]
fn step_packet_discovery_skips_unrelated_command_frames() {
    let rows = vec![decoded_frame_row(
        "command-frame-1",
        "2026-06-02T12:00:00Z",
        "COMMAND_RESPONSE",
        json!({
            "kind": "command_response",
            "response_to_command_name": "GET_HELLO",
            "result_code": 0,
            "warnings": []
        }),
    )];

    let report = run_step_packet_discovery(
        &rows,
        "synthetic.sqlite",
        "2026-06-02T00:00:00Z",
        "2026-06-03T00:00:00Z",
        StepPacketDiscoveryOptions::default(),
    )
    .unwrap();

    assert!(!report.pass);
    assert_eq!(report.decoded_frame_count, 1);
    assert_eq!(report.inspected_frame_count, 0);
    assert_eq!(report.skipped_frame_count, 1);
    assert!(
        report
            .issues
            .contains(&"no_step_discovery_frames".to_string())
    );
}

#[test]
fn step_capture_validation_accepts_monotonic_counter_matching_labels() {
    let rows = vec![
        decoded_frame_row(
            "step-frame-1",
            "2026-06-02T12:00:00Z",
            "HISTORICAL_DATA",
            json!({
                "kind": "data_packet",
                "packet_k": 11,
                "domain": "raw_stream_counted",
                "body_summary": {
                    "kind": "raw_stream_counted",
                    "step_count": 4100
                },
                "warnings": []
            }),
        ),
        decoded_frame_row(
            "step-frame-2",
            "2026-06-02T12:05:00Z",
            "HISTORICAL_DATA",
            json!({
                "kind": "data_packet",
                "packet_k": 11,
                "domain": "raw_stream_counted",
                "body_summary": {
                    "kind": "raw_stream_counted",
                    "step_count": 4200
                },
                "warnings": []
            }),
        ),
    ];

    let report = run_step_capture_validation(
        &rows,
        "synthetic.sqlite",
        "2026-06-02T12:00:00Z",
        "2026-06-02T12:10:00Z",
        StepCaptureValidationOptions {
            capture_kind: Some("100_counted_steps".to_string()),
            manual_step_delta: Some(100),
            official_whoop_step_delta: Some(102),
            tolerance_steps: 3,
            label_provenance: Some(json!({
                "source": "manual_count_plus_whoop_app_readout",
                "official_labels_are_labels": true
            })),
            ..StepCaptureValidationOptions::default()
        },
    )
    .unwrap();

    assert!(report.pass, "{:?}", report.issues);
    assert_eq!(report.schema, "bull.step-capture-validation-report.v1");
    assert_eq!(report.matching_counter_delta_count, 1);
    assert_eq!(report.counter_deltas[0].delta, 100);
    assert_eq!(report.counter_deltas[0].manual_delta_error, Some(0));
    assert_eq!(report.counter_deltas[0].official_delta_error, Some(-2));
    assert_eq!(report.counter_deltas[0].matches_manual_label, Some(true));
    assert_eq!(report.counter_deltas[0].matches_official_label, Some(true));
    assert_eq!(
        report.counter_deltas[0].source_kind_inference,
        "device_counter"
    );
}

#[test]
fn step_capture_validation_blocks_official_label_without_policy_marker() {
    let rows = vec![
        decoded_frame_row(
            "step-frame-1",
            "2026-06-02T12:00:00Z",
            "HISTORICAL_DATA",
            json!({
                "kind": "data_packet",
                "packet_k": 11,
                "domain": "raw_stream_counted",
                "body_summary": {
                    "kind": "raw_stream_counted",
                    "step_count": 4100
                },
                "warnings": []
            }),
        ),
        decoded_frame_row(
            "step-frame-2",
            "2026-06-02T12:05:00Z",
            "HISTORICAL_DATA",
            json!({
                "kind": "data_packet",
                "packet_k": 11,
                "domain": "raw_stream_counted",
                "body_summary": {
                    "kind": "raw_stream_counted",
                    "step_count": 4200
                },
                "warnings": []
            }),
        ),
    ];

    let report = run_step_capture_validation(
        &rows,
        "synthetic.sqlite",
        "2026-06-02T12:00:00Z",
        "2026-06-02T12:10:00Z",
        StepCaptureValidationOptions {
            capture_kind: Some("100_counted_steps".to_string()),
            manual_step_delta: Some(100),
            official_whoop_step_delta: Some(100),
            tolerance_steps: 0,
            label_provenance: Some(json!({
                "source": "whoop_app_readout"
            })),
            ..StepCaptureValidationOptions::default()
        },
    )
    .unwrap();

    assert!(!report.pass);
    assert_eq!(
        report.label_policy,
        "official_whoop_values_are_validation_labels_not_inputs"
    );
    assert_eq!(report.matching_counter_delta_count, 1);
    assert!(
        report
            .issues
            .contains(&"official_label_policy_not_marked".to_string())
    );
    assert!(
        report
            .next_actions
            .iter()
            .any(|action| action.reason == "official_label_policy_not_marked")
    );
}

#[test]
fn step_capture_validation_blocks_counter_without_validation_label() {
    let rows = vec![
        decoded_frame_row(
            "step-frame-1",
            "2026-06-02T12:00:00Z",
            "HISTORICAL_DATA",
            json!({
                "kind": "data_packet",
                "packet_k": 11,
                "domain": "raw_stream_counted",
                "body_summary": {
                    "kind": "raw_stream_counted",
                    "step_count": 4100
                },
                "warnings": []
            }),
        ),
        decoded_frame_row(
            "step-frame-2",
            "2026-06-02T12:05:00Z",
            "HISTORICAL_DATA",
            json!({
                "kind": "data_packet",
                "packet_k": 11,
                "domain": "raw_stream_counted",
                "body_summary": {
                    "kind": "raw_stream_counted",
                    "step_count": 4200
                },
                "warnings": []
            }),
        ),
    ];

    let report = run_step_capture_validation(
        &rows,
        "synthetic.sqlite",
        "2026-06-02T12:00:00Z",
        "2026-06-02T12:10:00Z",
        StepCaptureValidationOptions::default(),
    )
    .unwrap();

    assert!(!report.pass);
    assert_eq!(report.counter_delta_candidate_count, 1);
    assert_eq!(report.matching_counter_delta_count, 0);
    assert!(
        report
            .issues
            .contains(&"no_step_delta_validation_label".to_string())
    );
}

#[test]
fn step_packet_discovery_overflow_group_still_qualifies_and_truncates_output() {
    let max_fields = 4usize;
    let mut rows = Vec::new();
    for index in 0..8 {
        rows.push(decoded_frame_row(
            &format!("hidden-counter-{index}"),
            &format!("2026-06-02T12:{index:02}:00Z"),
            "HISTORICAL_DATA",
            json!({
                "kind": "data_packet",
                "packet_k": 11,
                "domain": "raw_stream_counted",
                "body_summary": {
                    "kind": "raw_stream_counted",
                    "hidden_stride": 1000 + index as i64
                },
                "warnings": []
            }),
        ));
    }

    let report = run_step_packet_discovery(
        &rows,
        "synthetic.sqlite",
        "2026-06-02T00:00:00Z",
        "2026-06-03T00:00:00Z",
        StepPacketDiscoveryOptions {
            max_candidate_fields: max_fields,
        },
    )
    .unwrap();

    assert_eq!(report.monotonic_counter_candidate_count, 1);
    assert!(
        report
            .issues
            .contains(&"candidate_field_output_truncated".to_string()),
        "{:?}",
        report.issues
    );
    assert!(
        report.candidate_field_count > report.candidate_fields.len(),
        "count {} emitted {}",
        report.candidate_field_count,
        report.candidate_fields.len()
    );
    assert!(
        report
            .candidate_fields
            .iter()
            .any(|field| field.field_name == "hidden_stride"),
        "{:?}",
        report.candidate_fields
    );
}

fn decoded_frame_row(
    frame_id: &str,
    captured_at: &str,
    packet_type_name: &str,
    parsed_payload: serde_json::Value,
) -> DecodedFrameRow {
    DecodedFrameRow {
        frame_id: frame_id.to_string(),
        evidence_id: format!("{frame_id}.evidence"),
        captured_at: captured_at.to_string(),
        device_type: "BULL".to_string(),
        raw_len: 0,
        header_len: 0,
        declared_len: 0,
        payload_hex: String::new(),
        payload_crc_hex: String::new(),
        header_crc_valid: true,
        payload_crc_valid: true,
        packet_type: None,
        packet_type_name: Some(packet_type_name.to_string()),
        sequence: None,
        command_or_event: None,
        parsed_payload_json: parsed_payload.to_string(),
        parser_version: "bull-core/step-discovery-test".to_string(),
        warnings_json: "[]".to_string(),
    }
}
