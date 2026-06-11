use bull_core::activity_identity::{ActivityIdentityInput, activity_idempotency_key};
use serde_json::json;

fn base_input() -> ActivityIdentityInput {
    ActivityIdentityInput {
        source: "activity_session_fixture".to_string(),
        provenance: json!({
            "activity_session_id": "stable-session",
            "nested": {
                "alpha": 1,
                "beta": 2
            },
            "source": "activity_session_fixture"
        }),
        start_time: "2026-05-27T06:00:00-04:00".to_string(),
        end_time: "2026-05-27T06:45:00-04:00".to_string(),
        activity_type: "Running".to_string(),
        raw_identifiers: vec![
            "external-session-42".to_string(),
            "source-record-9".to_string(),
        ],
        labels: vec!["Morning Tempo Run".to_string(), "Tempo Run".to_string()],
    }
}

#[test]
fn repeated_inputs_with_equivalent_identity_data_share_a_key() {
    let base = base_input();
    let mut repeated = base.clone();
    repeated.provenance = json!({
        "nested": {
            "beta": 2,
            "alpha": 1
        },
        "source": "activity_session_fixture",
        "activity_session_id": "stable-session"
    });
    repeated.start_time = "2026-05-27T10:00:00Z".to_string();
    repeated.end_time = "2026-05-27T10:45:00Z".to_string();
    repeated.activity_type = "running".to_string();

    assert_eq!(
        activity_idempotency_key(&base),
        activity_idempotency_key(&repeated)
    );
}

#[test]
fn changing_source_window_or_activity_type_changes_the_key() {
    let base = base_input();
    let base_key = activity_idempotency_key(&base);

    let mut source_changed = base.clone();
    source_changed.source = "manual_capture".to_string();
    assert_ne!(base_key, activity_idempotency_key(&source_changed));

    let mut window_changed = base.clone();
    window_changed.end_time = "2026-05-27T07:00:00-04:00".to_string();
    assert_ne!(base_key, activity_idempotency_key(&window_changed));

    let mut type_changed = base;
    type_changed.activity_type = "cycling".to_string();
    assert_ne!(base_key, activity_idempotency_key(&type_changed));
}

#[test]
fn custom_and_raw_labels_are_normalized_deterministically() {
    let mut first = base_input();
    first.labels = vec!["Morning Tempo Run".to_string(), "track workout".to_string()];
    first.raw_identifiers = vec![
        "source-record-9".to_string(),
        "external-session-42".to_string(),
    ];

    let mut second = base_input();
    second.labels = vec![
        " track workout ".to_string(),
        " morning tempo run ".to_string(),
    ];
    second.raw_identifiers = vec![
        "external-session-42".to_string(),
        "source-record-9".to_string(),
    ];

    assert_eq!(
        activity_idempotency_key(&first),
        activity_idempotency_key(&second)
    );
}
