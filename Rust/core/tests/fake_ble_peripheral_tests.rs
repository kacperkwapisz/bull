use bull_core::{
    commands::{
        CommandEmulatorLogEvidenceOptions, command_evidence_from_emulator_log_text,
        validate_commands,
    },
    openwhoop_reference::{
        WHOOP_COMMAND_FROM_STRAP_GEN5, WHOOP_COMMAND_TO_STRAP_GEN5, WHOOP_DATA_FROM_STRAP_GEN5,
        WHOOP_SERVICE_GEN5,
    },
    protocol::{
        PACKET_TYPE_COMMAND_RESPONSE, PACKET_TYPE_HISTORICAL_DATA, build_v5_command_frame,
        build_v5_payload_frame,
    },
};

const GET_DATA_RANGE_COMMAND: u8 = 34;
const SEND_HISTORICAL_DATA_COMMAND: u8 = 22;
const FAKE_CAPTURE_SOURCE: &str = "fake_ble_peripheral";
const FAKE_SOURCE_LOG: &str = "tests/fake_ble_peripheral/historical_flow.jsonl";
const FAKE_TRIGGERING_UI_ACTION: &str = "History > Sync";

#[test]
fn fake_ble_peripheral_harness_emits_historical_command_write_response_and_notification_rows() {
    let mut peripheral = FakeBlePeripheralHarness::new(FAKE_SOURCE_LOG);

    let get_data_range_frame = peripheral.write_and_respond(GET_DATA_RANGE_COMMAND, 1, &[], 0);
    let get_data_range_response =
        command_response_frame_hex_for_sequence(GET_DATA_RANGE_COMMAND, 1, 0);
    let historical_notification = peripheral.emit_notification(
        "data_from_strap",
        WHOOP_DATA_FROM_STRAP_GEN5,
        historical_data_notification_frame(),
    );
    let send_historical_data_frame =
        peripheral.write_and_respond(SEND_HISTORICAL_DATA_COMMAND, 2, &[0x01], 0);
    let send_historical_data_response =
        command_response_frame_hex_for_sequence(SEND_HISTORICAL_DATA_COMMAND, 2, 0);

    let log = peripheral.render_log();
    assert!(log.contains(r#""role":"command_to_strap""#));
    assert!(log.contains(r#""role":"command_from_strap""#));
    assert!(log.contains(r#""role":"data_from_strap""#));
    assert!(log.contains(get_data_range_frame.as_str()));
    assert!(log.contains(get_data_range_response.as_str()));
    assert!(log.contains(historical_notification.as_str()));
    assert!(log.contains(send_historical_data_frame.as_str()));
    assert!(log.contains(send_historical_data_response.as_str()));

    let evidence_report = command_evidence_from_emulator_log_text(
        peripheral.source_log(),
        &log,
        &historical_flow_options(),
    )
    .unwrap();

    assert!(evidence_report.pass, "{:?}", evidence_report.issues);
    assert!(evidence_report.official_capture_ready);
    assert!(evidence_report.local_frame_match_ready);
    assert!(evidence_report.direct_validation_ready);
    assert!(evidence_report.trusted_capture_context);
    assert!(
        evidence_report.issues.is_empty(),
        "{:?}",
        evidence_report.issues
    );
    assert_eq!(evidence_report.line_count, 5);
    assert_eq!(evidence_report.transaction_count, 2);
    assert_eq!(evidence_report.evidence_count, 2);
    assert_eq!(
        evidence_report
            .evidence
            .iter()
            .map(|row| row.command.as_str())
            .collect::<Vec<_>>(),
        vec!["get_data_range", "send_historical_data"]
    );

    let get_data_range = evidence_report
        .evidence
        .iter()
        .find(|row| row.command == "get_data_range")
        .unwrap();
    assert_eq!(get_data_range.official_capture_count, 1);
    assert_eq!(
        get_data_range.official_frame_hex.as_deref(),
        Some(get_data_range_frame.as_str())
    );
    assert_eq!(
        get_data_range.official_response_frame_hex.as_deref(),
        Some(get_data_range_response.as_str())
    );
    assert_eq!(
        get_data_range.local_frame_hex.as_deref(),
        Some(get_data_range_frame.as_str())
    );
    assert!(
        get_data_range
            .provenance_json
            .as_deref()
            .unwrap_or_default()
            .contains(r#""transaction_lines":[1]"#)
    );

    let send_historical_data = evidence_report
        .evidence
        .iter()
        .find(|row| row.command == "send_historical_data")
        .unwrap();
    assert_eq!(send_historical_data.official_capture_count, 1);
    assert_eq!(
        send_historical_data.official_frame_hex.as_deref(),
        Some(send_historical_data_frame.as_str())
    );
    assert_eq!(
        send_historical_data.official_response_frame_hex.as_deref(),
        Some(send_historical_data_response.as_str())
    );
    assert_eq!(
        send_historical_data.local_frame_hex.as_deref(),
        Some(send_historical_data_frame.as_str())
    );
    assert!(
        send_historical_data
            .provenance_json
            .as_deref()
            .unwrap_or_default()
            .contains(r#""transaction_lines":[4]"#)
    );

    let validation_report = validate_commands(&evidence_report.evidence);
    for command in ["get_data_range", "send_historical_data"] {
        let result = validation_report
            .commands
            .iter()
            .find(|row| row.command == command)
            .unwrap();
        assert!(
            result.direct_send_ready,
            "{command}: {:?}",
            result.missing_requirements
        );
    }
}

struct FakeBlePeripheralHarness {
    source_log: String,
    rows: Vec<String>,
    next_source_line: usize,
}

impl FakeBlePeripheralHarness {
    fn new(source_log: impl Into<String>) -> Self {
        Self {
            source_log: source_log.into(),
            rows: Vec::new(),
            next_source_line: 1,
        }
    }

    fn source_log(&self) -> &str {
        &self.source_log
    }

    fn write_and_respond(
        &mut self,
        command_number: u8,
        sequence: u8,
        payload: &[u8],
        result_code: u8,
    ) -> String {
        let write_frame_hex =
            hex::encode(build_v5_command_frame(sequence, command_number, payload));
        self.rows.push(
            serde_json::json!({
                "schema": "whoop-reversing.emulator-command-capture-row.v1",
                "kind": "ble_write_characteristic",
                "direction": "phone_to_device",
                "characteristic_uuid": WHOOP_COMMAND_TO_STRAP_GEN5,
                "characteristic_uuid_label": {"role": "command_to_strap"},
                "service_uuid": WHOOP_SERVICE_GEN5,
                "source": FAKE_CAPTURE_SOURCE,
                "source_log": &self.source_log,
                "source_line": self.next_source_line,
                "value_hex": &write_frame_hex,
                "write_type": "withResponse"
            })
            .to_string(),
        );
        self.next_source_line += 1;

        let response_frame_hex =
            command_response_frame_hex_for_sequence(command_number, sequence, result_code);
        self.rows.push(
            serde_json::json!({
                "schema": "whoop-reversing.emulator-command-capture-row.v1",
                "kind": "ble_characteristic_changed",
                "direction": "device_to_phone",
                "characteristic_uuid": WHOOP_COMMAND_FROM_STRAP_GEN5,
                "characteristic_uuid_label": {"role": "command_from_strap"},
                "notify_queued": true,
                "service_uuid": WHOOP_SERVICE_GEN5,
                "source": FAKE_CAPTURE_SOURCE,
                "source_log": &self.source_log,
                "source_line": self.next_source_line,
                "value_hex": &response_frame_hex,
            })
            .to_string(),
        );
        self.next_source_line += 1;

        write_frame_hex
    }

    fn emit_notification(
        &mut self,
        role: &str,
        characteristic_uuid: &str,
        frame_hex: String,
    ) -> String {
        self.rows.push(
            serde_json::json!({
                "schema": "whoop-reversing.emulator-command-capture-row.v1",
                "kind": "ble_characteristic_changed",
                "direction": "device_to_phone",
                "characteristic_uuid": characteristic_uuid,
                "characteristic_uuid_label": {"role": role},
                "notify_queued": true,
                "service_uuid": WHOOP_SERVICE_GEN5,
                "source": FAKE_CAPTURE_SOURCE,
                "source_log": &self.source_log,
                "source_line": self.next_source_line,
                "value_hex": &frame_hex,
            })
            .to_string(),
        );
        self.next_source_line += 1;
        frame_hex
    }

    fn render_log(&self) -> String {
        self.rows.join("\n")
    }
}

fn historical_flow_options() -> CommandEmulatorLogEvidenceOptions {
    CommandEmulatorLogEvidenceOptions {
        visible_user_intent: true,
        triggering_ui_action: Some(FAKE_TRIGGERING_UI_ACTION.to_string()),
        mirror_local_frame: true,
        ..CommandEmulatorLogEvidenceOptions::default()
    }
}

fn command_response_frame_hex_for_sequence(
    command: u8,
    origin_sequence: u8,
    result_code: u8,
) -> String {
    hex::encode(build_v5_payload_frame(&[
        PACKET_TYPE_COMMAND_RESPONSE,
        9,
        command,
        origin_sequence,
        result_code,
    ]))
}

fn historical_data_notification_frame() -> String {
    hex::encode(build_v5_payload_frame(&[
        PACKET_TYPE_HISTORICAL_DATA,
        0x10,
        0x11,
        0x12,
    ]))
}
