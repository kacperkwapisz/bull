use std::fs;

use bull_core::capture_sanitize::{
    CaptureSanitizeOptions, sanitize_capture_path, sanitize_json_value, sanitize_text,
};

#[test]
fn sanitizes_jsonl_capture_without_losing_protocol_evidence() {
    let tempdir = tempfile::tempdir().unwrap();
    let input = tempdir.path().join("input");
    let output = tempdir.path().join("sanitized");
    fs::create_dir_all(input.join("ble")).unwrap();
    fs::write(
        input.join("ble/events.jsonl"),
        concat!(
            "{\"timestamp\":\"2026-05-28T12:00:00Z\",",
            "\"device_model\":\"WHOOP 5.0\",",
            "\"device_firmware\":\"1.2.3\",",
            "\"packet_hex\":\"aa55cc33\",",
            "\"payload_hex\":\"deadbeef\",",
            "\"access_token\":\"secret-token\",",
            "\"user_email\":\"person@example.com\",",
            "\"device_id\":\"strap-serial-123\",",
            "\"bluetooth_address\":\"AA:BB:CC:DD:EE:FF\"}\n"
        ),
    )
    .unwrap();

    let report = sanitize_capture_path(CaptureSanitizeOptions {
        input_path: &input,
        output_path: &output,
        salt: "test-salt",
    })
    .unwrap();

    assert!(report.pass, "{:?}", report.issues);
    assert!(report.input_valid);
    assert!(report.output_ready);
    assert!(report.supported_files_written);
    assert!(report.unsupported_files_omitted);
    assert!(report.redaction_scan_clear);
    assert!(report.warnings_clear);
    assert!(report.evidence_complete);
    assert!(report.sanitize_ready);
    assert_eq!(report.totals.files_seen, 1);
    assert_eq!(report.totals.files_written, 1);
    assert_eq!(report.totals.secret_redactions, 1);
    assert_eq!(report.totals.identifier_pseudonyms, 3);
    assert!(report.next_actions.is_empty());
    assert!(output.join("sanitize-manifest.json").exists());

    let sanitized = fs::read_to_string(output.join("ble/events.jsonl")).unwrap();
    assert!(sanitized.contains("\"packet_hex\":\"aa55cc33\""));
    assert!(sanitized.contains("\"payload_hex\":\"deadbeef\""));
    assert!(sanitized.contains("\"device_model\":\"WHOOP 5.0\""));
    assert!(sanitized.contains("<redacted:accesstoken>"));
    assert!(sanitized.contains("<pseudonym:useremail:"));
    assert!(sanitized.contains("<pseudonym:deviceid:"));
    assert!(sanitized.contains("<pseudonym:bluetoothaddress:"));
    assert!(!sanitized.contains("secret-token"));
    assert!(!sanitized.contains("person@example.com"));
    assert!(!sanitized.contains("AA:BB:CC:DD:EE:FF"));
}

#[test]
fn sanitizes_text_logs_and_preserves_ble_uuids() {
    let text = concat!(
        "Authorization: Bearer abc.def.ghi\n",
        "service_uuid=61080001-0000-1000-8000-00805f9b34fb\n",
        "packet_hex=aa55cc33\n",
        "email=person@example.com\n",
        "mac=AA:BB:CC:DD:EE:FF\n"
    );

    let (sanitized, redactions) = sanitize_text(text, "test-salt");

    assert!(sanitized.contains("Authorization: <redacted:authorization>"));
    assert!(sanitized.contains("service_uuid=61080001-0000-1000-8000-00805f9b34fb"));
    assert!(sanitized.contains("packet_hex=aa55cc33"));
    assert!(sanitized.contains("<pseudonym:mac:"));
    assert!(!sanitized.contains("person@example.com"));
    assert!(!sanitized.contains("AA:BB:CC:DD:EE:FF"));
    assert_eq!(redactions.authorization_redactions, 1);
    assert_eq!(redactions.email_redactions, 0);
    assert_eq!(redactions.identifier_pseudonyms, 2);
}

#[test]
fn omits_binary_files_by_default() {
    let tempdir = tempfile::tempdir().unwrap();
    let input = tempdir.path().join("input");
    let output = tempdir.path().join("sanitized");
    fs::create_dir_all(&input).unwrap();
    fs::write(input.join("dump.zip"), [0, 159, 146, 150]).unwrap();

    let report = sanitize_capture_path(CaptureSanitizeOptions {
        input_path: &input,
        output_path: &output,
        salt: "test-salt",
    })
    .unwrap();

    assert!(report.pass);
    assert!(report.input_valid);
    assert!(report.output_ready);
    assert!(report.supported_files_written);
    assert!(report.unsupported_files_omitted);
    assert!(report.redaction_scan_clear);
    assert!(!report.warnings_clear);
    assert!(!report.evidence_complete);
    assert!(report.sanitize_ready);
    assert_eq!(report.totals.files_seen, 1);
    assert_eq!(report.totals.files_omitted, 1);
    assert!(!output.join("dump.zip").exists());
    assert!(
        report.files[0]
            .warnings
            .iter()
            .any(|warning| warning.contains("binary file omitted"))
    );
    assert!(
        report
            .next_actions
            .iter()
            .any(|action| { action.scope == "dump.zip" && action.reason == "binary_file_omitted" }),
        "{:?}",
        report.next_actions
    );
}

#[test]
fn leak_check_failure_reports_redaction_next_action() {
    let tempdir = tempfile::tempdir().unwrap();
    let input = tempdir.path().join("input");
    let output = tempdir.path().join("sanitized");
    fs::create_dir_all(&input).unwrap();
    fs::write(
        input.join("bad.json"),
        r#"{"packet_hex":"Bearer still-visible-in-preserved-protocol-field"}"#,
    )
    .unwrap();

    let report = sanitize_capture_path(CaptureSanitizeOptions {
        input_path: &input,
        output_path: &output,
        salt: "test-salt",
    })
    .unwrap();

    assert!(!report.pass);
    assert!(report.input_valid);
    assert!(report.output_ready);
    assert!(!report.supported_files_written);
    assert!(report.unsupported_files_omitted);
    assert!(!report.redaction_scan_clear);
    assert!(report.warnings_clear);
    assert!(!report.evidence_complete);
    assert!(!report.sanitize_ready);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.contains("Bearer token marker"))
    );
    assert!(
        report.next_actions.iter().any(|action| {
            action.scope == "bad.json"
                && action.reason == "secret_redaction_failed"
                && action.action.contains("run privacy lint")
        }),
        "{:?}",
        report.next_actions
    );
}

#[test]
fn sanitizes_nested_json_values() {
    let mut value: serde_json::Value = serde_json::json!({
        "capture": {
            "frame_hex": "aa55cc33",
            "session_id": "official-session-123",
            "cookie": "whoop-cookie",
            "nested": [{"member_id": 42}]
        }
    });

    let redactions = sanitize_json_value(&mut value, "test-salt");

    assert_eq!(value["capture"]["frame_hex"], "aa55cc33");
    assert_eq!(value["capture"]["cookie"], "<redacted:cookie>");
    assert!(
        value["capture"]["session_id"]
            .as_str()
            .unwrap()
            .starts_with("<pseudonym:sessionid:")
    );
    assert!(
        value["capture"]["nested"][0]["member_id"]
            .as_str()
            .unwrap()
            .starts_with("<pseudonym:memberid:")
    );
    assert_eq!(redactions.secret_redactions, 1);
    assert_eq!(redactions.identifier_pseudonyms, 2);
}
