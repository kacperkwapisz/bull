use bull_core::rr_hr_consistency::{
    RR_HR_CONSISTENCY_LABEL_POLICY, RR_HR_CONSISTENCY_REPORT_SCHEMA, RR_HR_CONSISTENCY_SCALE_BASIS,
    RrHrConsistencyOptions, RrHrConsistencyVerdict, RrHrFrameInput, evaluate_rr_hr_consistency,
};

fn input(frame: &str, hr: f64, rr: Vec<f64>) -> RrHrFrameInput {
    RrHrFrameInput {
        frame_id: frame.to_string(),
        evidence_id: format!("ev.{frame}"),
        captured_at: "2026-05-28T18:29:31Z".to_string(),
        reported_hr_bpm: hr,
        rr_intervals_ms: rr,
    }
}

/// RR intervals that imply exactly the reported HR (60000/600ms = 100 bpm).
fn consistent_input(frame: &str, hr: f64) -> RrHrFrameInput {
    let mean_rr = 60_000.0 / hr;
    input(frame, hr, vec![mean_rr, mean_rr])
}

#[test]
fn empty_input_is_insufficient_data() {
    let report = evaluate_rr_hr_consistency(&[], RrHrConsistencyOptions::default());
    assert_eq!(report.verdict, RrHrConsistencyVerdict::InsufficientData);
    assert_eq!(report.eligible_frame_count, 0);
    assert_eq!(report.candidate_frame_count, 0);
    assert!(report.mean_abs_error_bpm.is_none());
    assert!(
        report
            .blockers
            .contains(&"insufficient_eligible_v24_rr_hr_frames".to_string())
    );
}

#[test]
fn report_carries_schema_and_label_policy() {
    let report = evaluate_rr_hr_consistency(&[], RrHrConsistencyOptions::default());
    assert_eq!(report.schema, RR_HR_CONSISTENCY_REPORT_SCHEMA);
    assert_eq!(report.scale_basis_under_test, RR_HR_CONSISTENCY_SCALE_BASIS);
    assert_eq!(report.label_policy, RR_HR_CONSISTENCY_LABEL_POLICY);
}

#[test]
fn below_min_eligible_is_insufficient_even_if_all_consistent() {
    let inputs: Vec<RrHrFrameInput> = (0..5)
        .map(|i| consistent_input(&format!("f{i}"), 60.0))
        .collect();
    let report = evaluate_rr_hr_consistency(&inputs, RrHrConsistencyOptions::default());
    assert_eq!(report.eligible_frame_count, 5);
    assert_eq!(report.consistent_frame_count, 5);
    assert_eq!(report.verdict, RrHrConsistencyVerdict::InsufficientData);
}

#[test]
fn all_consistent_above_threshold_is_verified() {
    let inputs: Vec<RrHrFrameInput> = (0..25)
        .map(|i| consistent_input(&format!("f{i}"), 55.0 + (i as f64)))
        .collect();
    let report = evaluate_rr_hr_consistency(&inputs, RrHrConsistencyOptions::default());
    assert_eq!(report.eligible_frame_count, 25);
    assert_eq!(report.consistent_frame_count, 25);
    assert!((report.consistency_ratio - 1.0).abs() < 1e-9);
    assert_eq!(report.verdict, RrHrConsistencyVerdict::Verified);
    let mean_abs = report.mean_abs_error_bpm.expect("mean abs error present");
    assert!(mean_abs < 1.0, "mean abs error should be ~0, got {mean_abs}");
}

#[test]
fn rr_in_seconds_scale_is_inconsistent() {
    // If the field were seconds (e.g. 0.6) instead of ms, implied HR explodes
    // (60000/0.6 = 100000 bpm) and must be flagged inconsistent.
    let inputs: Vec<RrHrFrameInput> = (0..25)
        .map(|i| input(&format!("f{i}"), 60.0, vec![0.6, 0.6]))
        .collect();
    let report = evaluate_rr_hr_consistency(&inputs, RrHrConsistencyOptions::default());
    // 0.6 is below the plausible RR floor, so these frames are not eligible.
    assert_eq!(report.eligible_frame_count, 0);
    assert_eq!(report.verdict, RrHrConsistencyVerdict::InsufficientData);
}

#[test]
fn wrong_ms_values_disagree_with_hr_and_are_inconsistent() {
    // Plausible-range RR values that nonetheless contradict the reported HR.
    // reported 60 bpm but mean RR 500ms implies 120 bpm -> 60 bpm abs error.
    let inputs: Vec<RrHrFrameInput> = (0..25)
        .map(|i| input(&format!("f{i}"), 60.0, vec![500.0, 500.0]))
        .collect();
    let report = evaluate_rr_hr_consistency(&inputs, RrHrConsistencyOptions::default());
    assert_eq!(report.eligible_frame_count, 25);
    assert_eq!(report.consistent_frame_count, 0);
    assert_eq!(report.verdict, RrHrConsistencyVerdict::Inconsistent);
    assert!(
        report
            .blockers
            .contains(&"rr_hr_consistency_below_threshold".to_string())
    );
}

#[test]
fn mixed_consistency_uses_pass_ratio() {
    // 20 consistent + 5 inconsistent = 0.8 ratio, exactly at default threshold.
    let mut inputs: Vec<RrHrFrameInput> = (0..20)
        .map(|i| consistent_input(&format!("c{i}"), 62.0))
        .collect();
    inputs.extend((0..5).map(|i| input(&format!("x{i}"), 60.0, vec![450.0, 450.0])));
    let report = evaluate_rr_hr_consistency(&inputs, RrHrConsistencyOptions::default());
    assert_eq!(report.eligible_frame_count, 25);
    assert_eq!(report.consistent_frame_count, 20);
    assert!((report.consistency_ratio - 0.8).abs() < 1e-9);
    assert_eq!(report.verdict, RrHrConsistencyVerdict::Verified);
}

#[test]
fn zero_hr_frames_are_skipped() {
    let inputs = vec![
        input("a", 0.0, vec![600.0, 600.0]),
        input("b", 0.0, vec![600.0, 600.0]),
    ];
    let report = evaluate_rr_hr_consistency(&inputs, RrHrConsistencyOptions::default());
    assert_eq!(report.candidate_frame_count, 2);
    assert_eq!(report.eligible_frame_count, 0);
}

#[test]
fn frames_below_min_rr_per_frame_are_skipped() {
    let options = RrHrConsistencyOptions {
        min_rr_intervals_per_frame: 2,
        ..RrHrConsistencyOptions::default()
    };
    let inputs = vec![input("a", 60.0, vec![1000.0])];
    let report = evaluate_rr_hr_consistency(&inputs, options);
    assert_eq!(report.eligible_frame_count, 0);
}

#[test]
fn evidence_is_capped() {
    let inputs: Vec<RrHrFrameInput> = (0..120)
        .map(|i| consistent_input(&format!("f{i}"), 60.0))
        .collect();
    let report = evaluate_rr_hr_consistency(&inputs, RrHrConsistencyOptions::default());
    assert!(report.evidence.len() <= 50);
    assert_eq!(report.eligible_frame_count, 120);
}
