use std::fs;

use bull_core::activity_candidates::{
    ACTIVITY_CANDIDATE_CLASSIFIER_INPUT_SCHEMA, ACTIVITY_CANDIDATE_CLASSIFIER_REPORT_SCHEMA,
    ACTIVITY_CANDIDATE_UNKNOWN_ACTIVITY_TYPE, ActivityCandidateClassifierInput,
    ActivityCandidateClassifierOptions, ActivityCandidateClassifierReport, ActivityCandidateState,
    ActivityCommandSyncEvidence, ActivityEvidenceProvenance, ActivityFeatureWindowInput,
    ActivityGravitySample, ActivityHeartRateEvidence, ActivityMotionEvidence,
    ActivitySessionPacketDerivedMetricPlanReport, run_activity_candidate_classifier,
    run_packet_derived_activity_metric_planner,
};
use bull_core::activity_sessions::{
    ACTIVITY_SESSION_CORRECTION_SCOPE, activity_session_correction_plans,
};
use bull_core::health_sync::{ActivitySyncCandidate, ActivitySyncMetric, HealthSyncSessionKind};
use serde_json::json;

fn provenance(source: &str, evidence_id: &str) -> ActivityEvidenceProvenance {
    ActivityEvidenceProvenance {
        source: source.to_string(),
        evidence_id: Some(evidence_id.to_string()),
        capture_session_id: Some("capture-session-1".to_string()),
        frame_id: Some("frame-1".to_string()),
        note: Some("test evidence".to_string()),
        details: json!({
            "source": source,
            "evidence_id": evidence_id,
        }),
    }
}

fn gravity_sample(
    evidence_id: &str,
    x: f64,
    y: f64,
    z: f64,
    confidence: f64,
) -> ActivityGravitySample {
    ActivityGravitySample {
        gravity_x_g: x,
        gravity_y_g: y,
        gravity_z_g: z,
        confidence_0_to_1: confidence,
        provenance: provenance("gravity_sample", evidence_id),
    }
}

fn heart_rate_evidence(bpm: f64, confidence: f64, evidence_id: &str) -> ActivityHeartRateEvidence {
    ActivityHeartRateEvidence {
        heart_rate_bpm: bpm,
        confidence_0_to_1: confidence,
        provenance: provenance("heart_rate", evidence_id),
    }
}

fn motion_evidence(confidence: f64, evidence_id: &str) -> ActivityMotionEvidence {
    ActivityMotionEvidence {
        gravity_samples: vec![
            gravity_sample(&format!("{evidence_id}.g0"), 0.0, 0.0, -1.0, 0.98),
            gravity_sample(&format!("{evidence_id}.g1"), 0.01, -0.01, -0.9999, 0.97),
            gravity_sample(&format!("{evidence_id}.g2"), -0.02, 0.02, -0.9995, 0.96),
        ],
        confidence_0_to_1: confidence,
        provenance: provenance("motion", evidence_id),
    }
}

fn command_sync_evidence(
    synced: bool,
    confidence: f64,
    evidence_id: &str,
) -> ActivityCommandSyncEvidence {
    ActivityCommandSyncEvidence {
        synced,
        confidence_0_to_1: confidence,
        provenance: provenance("command_sync", evidence_id),
    }
}

fn window_input(
    window_id: &str,
    heart_rate: Option<ActivityHeartRateEvidence>,
    motion: Option<ActivityMotionEvidence>,
    command_sync: Option<ActivityCommandSyncEvidence>,
    approved_by_user: bool,
) -> ActivityFeatureWindowInput {
    ActivityFeatureWindowInput {
        window_id: window_id.to_string(),
        start_time: "2026-05-27T00:00:00Z".to_string(),
        end_time: "2026-05-27T00:20:00Z".to_string(),
        heart_rate,
        motion,
        command_sync,
        approved_by_user,
    }
}

fn classifier_input(windows: Vec<ActivityFeatureWindowInput>) -> ActivityCandidateClassifierInput {
    ActivityCandidateClassifierInput {
        schema: ACTIVITY_CANDIDATE_CLASSIFIER_INPUT_SCHEMA.to_string(),
        options: ActivityCandidateClassifierOptions::default(),
        windows,
    }
}

fn packet_derived_classifier_input() -> ActivityCandidateClassifierInput {
    let raw =
        fs::read_to_string("fixtures/synthetic/activity_candidates_packet_derived_windows.json")
            .unwrap();
    serde_json::from_str(&raw).unwrap()
}

fn packet_derived_metric(
    session_kind: &str,
    metric_name: &str,
    value: f64,
    unit: &str,
    evidence_id: &str,
) -> ActivitySyncMetric {
    ActivitySyncMetric {
        name: metric_name.to_string(),
        value,
        unit: unit.to_string(),
        start_time: Some("2026-05-27T06:00:00Z".to_string()),
        end_time: Some("2026-05-27T06:45:00Z".to_string()),
        quality_flags: vec!["trusted_packet_derived".to_string()],
        provenance: json!({
            "source": "packet_derived",
            "evidence_id": evidence_id,
            "capture_session_id": "synthetic.activity.packet_derived.session",
            "frame_id": format!("{evidence_id}.frame"),
            "details": {
                "fixture_id": "synthetic.activity.sessions.pre_device.hand_derived",
                "session_kind": session_kind,
                "row_kind": "metric",
                "status": "pre_device",
            }
        }),
    }
}

fn packet_derived_activity_session(
    session_id: &str,
    activity_type: &str,
    fixture_session_kind: &str,
    session_kind: HealthSyncSessionKind,
    source_kind: &str,
    confidence_0_to_1: f64,
    approved_by_user: bool,
    metrics: Vec<ActivitySyncMetric>,
) -> ActivitySyncCandidate {
    ActivitySyncCandidate {
        session_id: session_id.to_string(),
        session_kind,
        activity_type: activity_type.to_string(),
        raw_activity_type: Some(activity_type.to_string()),
        custom_label: Some(activity_type.to_string()),
        source_kind: source_kind.to_string(),
        start_time: "2026-05-27T06:00:00Z".to_string(),
        end_time: "2026-05-27T06:45:00Z".to_string(),
        confidence_0_to_1,
        approved_by_user,
        metrics,
        intervals: Vec::new(),
        provenance: json!({
            "source": "synthetic.activity.fixture",
            "fixture_id": "synthetic.activity.sessions.pre_device.hand_derived",
            "session_kind": fixture_session_kind,
            "row_kind": "session",
            "status": "pre_device"
        }),
    }
}

fn assert_packet_derived_plan_report(
    report: &ActivitySessionPacketDerivedMetricPlanReport,
    expected_session_id: &str,
    expected_activity_type: &str,
    expected_session_kind: HealthSyncSessionKind,
    expected_source_kind: &str,
    expected_fixture_session_kind: &str,
    expected_metric_names: &[&str],
    expected_ignored_metric_names: &[&str],
) {
    assert!(report.pass, "{:?}", report.issues);
    assert_eq!(
        report.schema,
        bull_core::activity_candidates::ACTIVITY_SESSION_PACKET_DERIVED_METRIC_PLAN_REPORT_SCHEMA
    );
    assert_eq!(
        report.generated_by,
        bull_core::activity_candidates::ACTIVITY_SESSION_PACKET_DERIVED_METRIC_PLAN_GENERATED_BY
    );
    assert_eq!(report.session_id, expected_session_id);
    assert_eq!(report.activity_type, expected_activity_type);
    assert_eq!(report.session_kind, expected_session_kind);
    assert_eq!(report.source_kind, expected_source_kind);
    assert_eq!(
        report.metric_count,
        expected_metric_names.len() + expected_ignored_metric_names.len()
    );
    assert_eq!(report.attached_metric_count, expected_metric_names.len());
    assert_eq!(
        report.ignored_metric_count,
        expected_ignored_metric_names.len()
    );

    let metric_names = report
        .metric_plans
        .iter()
        .map(|plan| plan.metric_name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(metric_names, expected_metric_names);
    assert_eq!(
        report.ignored_metric_names,
        expected_ignored_metric_names
            .iter()
            .map(|name| (*name).to_string())
            .collect::<Vec<_>>()
    );

    for (index, plan) in report.metric_plans.iter().enumerate() {
        assert_eq!(plan.session_id, expected_session_id);
        assert_eq!(plan.activity_type, expected_activity_type);
        assert_eq!(plan.session_kind, expected_session_kind);
        assert_eq!(plan.source_kind, expected_source_kind);
        assert_eq!(
            plan.quality_flags,
            vec!["trusted_packet_derived".to_string()]
        );
        assert_eq!(
            plan.provenance["source"],
            serde_json::Value::String("packet_derived".to_string())
        );
        assert_eq!(
            plan.provenance["details"]["fixture_id"],
            serde_json::Value::String(
                "synthetic.activity.sessions.pre_device.hand_derived".to_string()
            )
        );
        assert_eq!(
            plan.provenance["details"]["session_kind"],
            serde_json::Value::String(expected_fixture_session_kind.to_string())
        );
        assert_eq!(
            plan.provenance["details"]["row_kind"],
            serde_json::Value::String("metric".to_string())
        );
        assert_eq!(
            plan.provenance["details"]["status"],
            serde_json::Value::String("pre_device".to_string())
        );
        assert!(plan.start_time.is_some());
        assert!(plan.end_time.is_some());
        assert!(
            !plan.metric_name.is_empty(),
            "metric plan #{index} should keep the metric name"
        );
    }
}

#[test]
fn hr_only_window_stays_unknown_and_reports_missing_motion_and_sync() {
    let input = classifier_input(vec![window_input(
        "window-hr-only",
        Some(heart_rate_evidence(122.0, 0.95, "hr-1")),
        None,
        None,
        false,
    )]);

    let report = run_activity_candidate_classifier(&input);
    assert_eq!(report.schema, ACTIVITY_CANDIDATE_CLASSIFIER_REPORT_SCHEMA);
    assert!(!report.pass);
    assert_eq!(report.window_count, 1);
    assert_eq!(report.candidate_window_count, 0);
    assert_eq!(report.unknown_window_count, 1);
    assert_eq!(report.blocked_window_count, 0);
    assert_eq!(report.windows.len(), 1);

    let window = &report.windows[0];
    assert_eq!(
        window.activity_type,
        ACTIVITY_CANDIDATE_UNKNOWN_ACTIVITY_TYPE
    );
    assert_eq!(window.state, ActivityCandidateState::Unknown);
    assert!(
        window
            .blocker_reasons
            .contains(&"missing_motion".to_string())
    );
    assert!(
        window
            .blocker_reasons
            .contains(&"missing_command_sync".to_string())
    );
    assert!(
        window
            .blocker_reasons
            .iter()
            .all(|reason| reason != "candidate_promotion_not_approved")
    );
    assert!(
        window
            .readiness_reasons
            .contains(&"heart_rate_present".to_string())
    );
    assert!(window.next_actions.iter().any(|action| {
        action.reason == "missing_motion"
            && action
                .action
                .contains("motion evidence with gravity samples")
    }));
    assert!(window.next_actions.iter().any(|action| {
        action.reason == "missing_command_sync"
            && action.action.contains("Carry command-sync evidence")
    }));
}

#[test]
fn motion_only_window_stays_unknown_and_flags_low_confidence() {
    let input = classifier_input(vec![window_input(
        "window-motion-only",
        None,
        Some(motion_evidence(0.40, "motion-1")),
        None,
        false,
    )]);

    let report = run_activity_candidate_classifier(&input);
    assert!(!report.pass);
    assert_eq!(report.window_count, 1);
    assert_eq!(report.unknown_window_count, 1);
    assert_eq!(report.candidate_window_count, 0);
    assert_eq!(report.blocked_window_count, 0);

    let window = &report.windows[0];
    assert_eq!(window.state, ActivityCandidateState::Unknown);
    assert!(
        window
            .blocker_reasons
            .contains(&"missing_heart_rate".to_string())
    );
    assert!(
        window
            .blocker_reasons
            .contains(&"missing_command_sync".to_string())
    );
    assert!(
        window
            .blocker_reasons
            .contains(&"low_confidence".to_string())
    );
    assert!(
        window
            .readiness_reasons
            .iter()
            .any(|reason| reason == "motion_present")
    );
    assert!(
        window
            .readiness_reasons
            .iter()
            .any(|reason| reason.starts_with("gravity_stability_reference:"))
    );
    assert!(window.confidence_0_to_1 < 0.75);
}

#[test]
fn hr_and_motion_window_can_promote_to_candidate_when_ready() {
    let input = classifier_input(vec![window_input(
        "window-hr-motion",
        Some(heart_rate_evidence(128.0, 0.96, "hr-2")),
        Some(motion_evidence(0.95, "motion-2")),
        Some(command_sync_evidence(true, 0.97, "sync-1")),
        true,
    )]);

    let report = run_activity_candidate_classifier(&input);
    assert!(report.pass, "{:?}", report.issues);
    assert_eq!(report.window_count, 1);
    assert_eq!(report.candidate_window_count, 1);
    assert_eq!(report.unknown_window_count, 0);
    assert_eq!(report.blocked_window_count, 0);

    let window = &report.windows[0];
    assert_eq!(window.state, ActivityCandidateState::Candidate);
    assert_eq!(
        window.activity_type,
        ACTIVITY_CANDIDATE_UNKNOWN_ACTIVITY_TYPE
    );
    assert!(window.blocker_reasons.is_empty());
    assert!(
        window
            .readiness_reasons
            .contains(&"heart_rate_present".to_string())
    );
    assert!(
        window
            .readiness_reasons
            .contains(&"motion_present".to_string())
    );
    assert!(
        window
            .readiness_reasons
            .contains(&"command_sync_present".to_string())
    );
    assert!(
        window
            .readiness_reasons
            .contains(&"candidate_promotion_approved".to_string())
    );
    assert!(window.gravity_stability_0_to_1.unwrap() > 0.80);
    let correction_plans = activity_session_correction_plans();
    let correction_actions = window
        .next_actions
        .iter()
        .filter(|action| action.scope == ACTIVITY_SESSION_CORRECTION_SCOPE)
        .collect::<Vec<_>>();
    assert_eq!(correction_actions.len(), correction_plans.len());
    for plan in correction_plans {
        assert!(
            correction_actions.iter().any(|action| {
                action.reason == plan.kind.as_str() && action.action == plan.action
            })
        );
    }

    let serialized = serde_json::to_value(&report).unwrap();
    assert_eq!(
        serialized["schema"],
        ACTIVITY_CANDIDATE_CLASSIFIER_REPORT_SCHEMA
    );
    assert_eq!(serialized["windows"][0]["state"], "candidate");
    assert_eq!(serialized["windows"][0]["activity_type"], "unknown");

    let round_trip: ActivityCandidateClassifierReport = serde_json::from_value(serialized).unwrap();
    assert_eq!(
        round_trip.windows[0].state,
        ActivityCandidateState::Candidate
    );
    assert_eq!(round_trip.candidate_window_count, 1);
    assert!(
        (round_trip.windows[0].confidence_0_to_1 - report.windows[0].confidence_0_to_1).abs()
            < 1e-12
    );
}

#[test]
fn packet_derived_hr_only_and_motion_only_windows_keep_provenance_and_blockers_visible() {
    let input = packet_derived_classifier_input();
    let report = run_activity_candidate_classifier(&input);

    assert!(!report.pass);
    assert_eq!(report.schema, ACTIVITY_CANDIDATE_CLASSIFIER_REPORT_SCHEMA);
    assert_eq!(report.window_count, 2);
    assert_eq!(report.candidate_window_count, 0);
    assert_eq!(report.unknown_window_count, 2);
    assert_eq!(report.blocked_window_count, 0);
    assert!(report.issues.is_empty());

    let hr_only = report
        .windows
        .iter()
        .find(|window| window.window_id == "synthetic.activity.packet_derived.hr_only.window")
        .unwrap();
    assert_eq!(hr_only.state, ActivityCandidateState::Unknown);
    assert!(
        hr_only
            .blocker_reasons
            .contains(&"missing_motion".to_string())
    );
    assert!(
        hr_only
            .blocker_reasons
            .contains(&"missing_command_sync".to_string())
    );
    assert!(
        hr_only
            .readiness_reasons
            .contains(&"heart_rate_present".to_string())
    );
    assert!(
        hr_only
            .readiness_reasons
            .iter()
            .all(|reason| !reason.starts_with("gravity_stability_reference:"))
    );
    assert!(
        hr_only
            .provenance
            .get("heart_rate_provenance")
            .and_then(serde_json::Value::as_object)
            .and_then(|value| value.get("details"))
            .and_then(serde_json::Value::as_object)
            .and_then(|value| value.get("fixture_id"))
            .and_then(serde_json::Value::as_str)
            == Some("synthetic.bull.v5.historical_k18_packet")
    );
    assert_eq!(
        hr_only.provenance["motion_provenance"],
        serde_json::Value::Null
    );
    assert_eq!(
        hr_only.provenance["command_sync_provenance"],
        serde_json::Value::Null
    );
    assert!(hr_only.next_actions.iter().any(|action| {
        action.reason == "missing_motion"
            && action
                .action
                .contains("motion evidence with gravity samples")
    }));
    assert!(hr_only.next_actions.iter().any(|action| {
        action.reason == "missing_command_sync"
            && action.action.contains("Carry command-sync evidence")
    }));

    let motion_only = report
        .windows
        .iter()
        .find(|window| window.window_id == "synthetic.activity.packet_derived.motion_only.window")
        .unwrap();
    assert_eq!(motion_only.state, ActivityCandidateState::Unknown);
    assert!(
        motion_only
            .blocker_reasons
            .contains(&"missing_heart_rate".to_string())
    );
    assert!(
        motion_only
            .blocker_reasons
            .contains(&"missing_command_sync".to_string())
    );
    assert!(
        motion_only
            .blocker_reasons
            .contains(&"low_confidence".to_string())
    );
    assert!(
        motion_only
            .readiness_reasons
            .contains(&"motion_present".to_string())
    );
    assert!(
        motion_only
            .readiness_reasons
            .iter()
            .any(|reason| reason.starts_with("gravity_stability_reference:"))
    );
    assert_eq!(motion_only.motion_confidence_0_to_1, Some(0.4));
    assert!(motion_only.gravity_stability_0_to_1.unwrap() > 0.80);
    assert!(
        motion_only
            .provenance
            .get("motion_provenance")
            .and_then(serde_json::Value::as_object)
            .and_then(|value| value.get("details"))
            .and_then(serde_json::Value::as_object)
            .and_then(|value| value.get("fixture_id"))
            .and_then(serde_json::Value::as_str)
            == Some("synthetic.bull.v5.k10_motion_summary_short")
    );
    assert_eq!(
        motion_only.provenance["heart_rate_provenance"],
        serde_json::Value::Null
    );
    assert_eq!(
        motion_only.provenance["command_sync_provenance"],
        serde_json::Value::Null
    );
    assert!(motion_only.next_actions.iter().any(|action| {
        action.reason == "missing_heart_rate"
            && action
                .action
                .contains("Capture trusted heart-rate evidence")
    }));
    assert!(motion_only.next_actions.iter().any(|action| {
        action.reason == "low_confidence"
            && action
                .action
                .contains("confidence score clears the threshold")
    }));
}

#[test]
fn hr_and_motion_window_blocks_unapproved_promotion() {
    let input = classifier_input(vec![window_input(
        "window-unapproved",
        Some(heart_rate_evidence(128.0, 0.96, "hr-3")),
        Some(motion_evidence(0.95, "motion-3")),
        Some(command_sync_evidence(true, 0.97, "sync-2")),
        false,
    )]);

    let report = run_activity_candidate_classifier(&input);
    assert!(!report.pass);
    assert_eq!(report.window_count, 1);
    assert_eq!(report.candidate_window_count, 0);
    assert_eq!(report.unknown_window_count, 0);
    assert_eq!(report.blocked_window_count, 1);

    let window = &report.windows[0];
    assert_eq!(window.state, ActivityCandidateState::Blocked);
    assert!(
        window
            .blocker_reasons
            .contains(&"candidate_promotion_not_approved".to_string())
    );
    assert!(
        window
            .blocker_reasons
            .iter()
            .all(|reason| reason != "low_confidence")
    );
}

#[test]
fn packet_derived_load_and_strain_attach_to_unknown_and_run_like_sessions() {
    let unknown = packet_derived_activity_session(
        "synthetic.activity.unknown.session",
        "unknown",
        "unknown",
        HealthSyncSessionKind::Activity,
        "packet_derived_activity",
        0.29,
        false,
        vec![
            packet_derived_metric(
                "unknown",
                "load",
                61.0,
                "load",
                "synthetic.activity.unknown.metric.load",
            ),
            packet_derived_metric(
                "unknown",
                "strain",
                8.8,
                "score_0_to_21",
                "synthetic.activity.unknown.metric.strain",
            ),
            packet_derived_metric(
                "unknown",
                "distance",
                6.8,
                "km",
                "synthetic.activity.unknown.metric.distance",
            ),
            packet_derived_metric(
                "unknown",
                "speed",
                3.4,
                "m/s",
                "synthetic.activity.unknown.metric.speed",
            ),
        ],
    );
    let unknown_report = run_packet_derived_activity_metric_planner(&unknown);
    assert_packet_derived_plan_report(
        &unknown_report,
        "synthetic.activity.unknown.session",
        "unknown",
        HealthSyncSessionKind::Activity,
        "packet_derived_activity",
        "unknown",
        &["load", "strain"],
        &["distance", "speed"],
    );

    let run_like = packet_derived_activity_session(
        "synthetic.activity.run_like.session",
        "running",
        "run_like",
        HealthSyncSessionKind::Activity,
        "packet_derived_activity",
        0.96,
        true,
        vec![
            packet_derived_metric(
                "run_like",
                "load",
                72.0,
                "load",
                "synthetic.activity.run_like.metric.load",
            ),
            packet_derived_metric(
                "run_like",
                "strain",
                13.5,
                "score_0_to_21",
                "synthetic.activity.run_like.metric.strain",
            ),
            packet_derived_metric(
                "run_like",
                "distance",
                10.2,
                "km",
                "synthetic.activity.run_like.metric.distance",
            ),
            packet_derived_metric(
                "run_like",
                "cadence",
                172.0,
                "count",
                "synthetic.activity.run_like.metric.cadence",
            ),
        ],
    );
    let run_like_report = run_packet_derived_activity_metric_planner(&run_like);
    assert_packet_derived_plan_report(
        &run_like_report,
        "synthetic.activity.run_like.session",
        "running",
        HealthSyncSessionKind::Activity,
        "packet_derived_activity",
        "run_like",
        &["load", "strain"],
        &["cadence", "distance"],
    );
}

#[test]
fn packet_derived_load_and_strain_attach_to_strength_and_ride_like_sessions() {
    let ride_like = packet_derived_activity_session(
        "synthetic.activity.ride_like.session",
        "cycling",
        "ride_like",
        HealthSyncSessionKind::Activity,
        "packet_derived_activity",
        0.93,
        true,
        vec![
            packet_derived_metric(
                "ride_like",
                "load",
                84.0,
                "load",
                "synthetic.activity.ride_like.metric.load",
            ),
            packet_derived_metric(
                "ride_like",
                "strain",
                11.2,
                "score_0_to_21",
                "synthetic.activity.ride_like.metric.strain",
            ),
            packet_derived_metric(
                "ride_like",
                "power",
                192.0,
                "w",
                "synthetic.activity.ride_like.metric.power",
            ),
            packet_derived_metric(
                "ride_like",
                "cadence",
                89.0,
                "count",
                "synthetic.activity.ride_like.metric.cadence",
            ),
        ],
    );
    let ride_like_report = run_packet_derived_activity_metric_planner(&ride_like);
    assert_packet_derived_plan_report(
        &ride_like_report,
        "synthetic.activity.ride_like.session",
        "cycling",
        HealthSyncSessionKind::Activity,
        "packet_derived_activity",
        "ride_like",
        &["load", "strain"],
        &["cadence", "power"],
    );

    let strength_like = packet_derived_activity_session(
        "synthetic.activity.strength_like.session",
        "strength",
        "strength_like",
        HealthSyncSessionKind::Workout,
        "packet_derived_workout",
        0.91,
        true,
        vec![
            packet_derived_metric(
                "strength_like",
                "load",
                86.0,
                "load",
                "synthetic.activity.strength_like.metric.load",
            ),
            packet_derived_metric(
                "strength_like",
                "strain",
                15.1,
                "score_0_to_21",
                "synthetic.activity.strength_like.metric.strain",
            ),
            packet_derived_metric(
                "strength_like",
                "repetitions",
                156.0,
                "count",
                "synthetic.activity.strength_like.metric.repetitions",
            ),
            packet_derived_metric(
                "strength_like",
                "calories",
                320.0,
                "kcal",
                "synthetic.activity.strength_like.metric.calories",
            ),
        ],
    );
    let strength_like_report = run_packet_derived_activity_metric_planner(&strength_like);
    assert_packet_derived_plan_report(
        &strength_like_report,
        "synthetic.activity.strength_like.session",
        "strength",
        HealthSyncSessionKind::Workout,
        "packet_derived_workout",
        "strength_like",
        &["load", "strain"],
        &["calories", "repetitions"],
    );
}

#[test]
fn empty_window_batch_reports_an_input_issue() {
    let report = run_activity_candidate_classifier(&classifier_input(Vec::new()));

    assert!(!report.pass);
    assert_eq!(report.window_count, 0);
    assert_eq!(report.candidate_window_count, 0);
    assert_eq!(report.unknown_window_count, 0);
    assert_eq!(report.blocked_window_count, 0);
    assert_eq!(
        report.issues,
        vec!["no_activity_feature_windows_provided".to_string()]
    );
    assert_eq!(report.next_actions.len(), 1);
    assert_eq!(
        report.next_actions[0].reason,
        "no_activity_feature_windows_provided"
    );
}
