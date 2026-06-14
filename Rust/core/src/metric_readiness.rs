use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    activity_candidates::{
        ACTIVITY_CANDIDATE_CLASSIFIER_INPUT_SCHEMA, ActivityCandidateClassifierReport,
    },
    capture_correlation::CaptureCorrelationReport,
    metrics::built_in_algorithm_definitions,
};

pub const METRIC_INPUT_READINESS_REPORT_SCHEMA: &str = "bull.metric-input-readiness-report.v1";

#[derive(Debug, Clone, Copy, Default)]
pub struct MetricInputReadinessOptions {
    pub require_scores_ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricInputReadinessReport {
    pub schema: String,
    pub generated_by: String,
    pub pass: bool,
    pub require_scores_ready: bool,
    pub capture_correlation_pass: bool,
    pub family_count: usize,
    pub ready_family_count: usize,
    pub families: Vec<MetricFamilyReadiness>,
    pub issues: Vec<String>,
    #[serde(default)]
    pub next_actions: Vec<MetricInputNextAction>,
    #[serde(default)]
    pub activity_session_promotion: ActivitySessionPromotionReadiness,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActivitySessionPromotionReadiness {
    pub classification_evidence_available: bool,
    pub pass: bool,
    pub window_count: usize,
    pub candidate_window_count: usize,
    pub unknown_window_count: usize,
    pub blocked_window_count: usize,
    pub blocker_reasons: Vec<String>,
    #[serde(default)]
    pub next_actions: Vec<MetricInputNextAction>,
}

impl ActivitySessionPromotionReadiness {
    fn missing_classification_evidence() -> Self {
        Self {
            classification_evidence_available: false,
            pass: false,
            window_count: 0,
            candidate_window_count: 0,
            unknown_window_count: 0,
            blocked_window_count: 0,
            blocker_reasons: vec!["classification_evidence_missing".to_string()],
            next_actions: vec![MetricInputNextAction {
                scope: "activity_session_promotion".to_string(),
                reason: "classification_evidence_missing".to_string(),
                action: "Run the activity candidate classifier on trusted HR, motion, and command-sync windows, then rerun metric readiness with the classifier output.".to_string(),
            }],
        }
    }

    fn from_classifier_report(report: &ActivityCandidateClassifierReport) -> Self {
        let mut blocker_reasons = BTreeSet::new();

        if report
            .issues
            .iter()
            .any(|issue| issue == "activity_candidate_classifier_input_schema_mismatch")
        {
            blocker_reasons.insert("classification_input_schema_mismatch".to_string());
        }
        if report
            .issues
            .iter()
            .any(|issue| issue == "no_activity_feature_windows_provided")
        {
            blocker_reasons.insert("classification_evidence_missing".to_string());
        }

        for window in &report.windows {
            for reason in &window.blocker_reasons {
                match reason.as_str() {
                    "missing_heart_rate" | "missing_motion" | "missing_command_sync" => {
                        blocker_reasons.insert(reason.clone());
                    }
                    "low_confidence" => {
                        blocker_reasons.insert("low_classification_confidence".to_string());
                    }
                    "candidate_promotion_not_approved" => {
                        blocker_reasons.insert("candidate_promotion_not_approved".to_string());
                    }
                    _ => {}
                }
            }
        }

        if blocker_reasons.is_empty() && !report.pass {
            blocker_reasons.insert("classification_evidence_missing".to_string());
        }

        let blocker_reasons = blocker_reasons.into_iter().collect::<Vec<_>>();
        let next_actions =
            dedupe_next_actions(activity_session_promotion_next_actions(&blocker_reasons));

        Self {
            classification_evidence_available: true,
            pass: report.pass,
            window_count: report.window_count,
            candidate_window_count: report.candidate_window_count,
            unknown_window_count: report.unknown_window_count,
            blocked_window_count: report.blocked_window_count,
            blocker_reasons,
            next_actions,
        }
    }
}

impl Default for ActivitySessionPromotionReadiness {
    fn default() -> Self {
        Self::missing_classification_evidence()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricFamilyReadiness {
    pub metric_family: String,
    pub algorithm_id: String,
    pub version: String,
    pub input_schema: String,
    pub output_schema: String,
    pub required_input_count: usize,
    pub ready_input_count: usize,
    pub score_ready: bool,
    pub inputs: Vec<MetricInputReadiness>,
    pub blocker_reasons: Vec<String>,
    #[serde(default)]
    pub next_actions: Vec<MetricInputNextAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricInputReadiness {
    pub input_name: String,
    pub unit: Option<String>,
    pub source_signal: String,
    pub status: String,
    pub required_summary_kinds: Vec<String>,
    pub candidate_observation_count: usize,
    pub trusted_evidence_count: usize,
    pub blocker_reasons: Vec<String>,
    #[serde(default)]
    pub next_actions: Vec<MetricInputNextAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetricInputNextAction {
    pub scope: String,
    pub reason: String,
    pub action: String,
}

#[derive(Debug, Clone)]
struct InputPlan {
    source_signal: &'static str,
    required_summary_kinds: &'static [&'static str],
    extraction_ready: bool,
    blocker: &'static str,
}

#[derive(Debug, Clone, Copy, Default)]
struct SummaryEvidence {
    observations: usize,
    trusted: usize,
}

pub fn run_metric_input_readiness(
    correlation: &CaptureCorrelationReport,
    options: MetricInputReadinessOptions,
) -> MetricInputReadinessReport {
    run_metric_input_readiness_internal(
        correlation,
        options,
        ActivitySessionPromotionReadiness::default(),
    )
}

pub fn run_metric_input_readiness_with_activity_classifier(
    correlation: &CaptureCorrelationReport,
    activity_classifier: &ActivityCandidateClassifierReport,
    options: MetricInputReadinessOptions,
) -> MetricInputReadinessReport {
    run_metric_input_readiness_internal(
        correlation,
        options,
        ActivitySessionPromotionReadiness::from_classifier_report(activity_classifier),
    )
}

fn run_metric_input_readiness_internal(
    correlation: &CaptureCorrelationReport,
    options: MetricInputReadinessOptions,
    activity_session_promotion: ActivitySessionPromotionReadiness,
) -> MetricInputReadinessReport {
    let mut issues = Vec::new();
    if !correlation.pass {
        issues.push("capture_correlation_report_not_passed".to_string());
    }
    let evidence = summary_evidence(correlation);
    let mut families = Vec::new();

    for definition in built_in_algorithm_definitions() {
        let input_requirements: serde_json::Value =
            serde_json::from_str(&definition.input_requirements_json)
                .unwrap_or(serde_json::Value::Null);
        let requirements = input_requirements.as_object().cloned().unwrap_or_default();
        let mut inputs = requirements
            .iter()
            .map(|(input_name, requirement)| {
                input_readiness(
                    input_name,
                    requirement
                        .get("unit")
                        .and_then(|value| value.as_str())
                        .map(str::to_string),
                    &evidence,
                )
            })
            .collect::<Vec<_>>();
        inputs.sort_by(|left, right| left.input_name.cmp(&right.input_name));

        let ready_input_count = inputs
            .iter()
            .filter(|input| input.status == "ready")
            .count();
        let blocker_reasons = inputs
            .iter()
            .filter(|input| input.status != "ready")
            .map(|input| format!("{}: {}", input.input_name, input.blocker_reasons.join("; ")))
            .collect::<Vec<_>>();
        let next_actions = dedupe_next_actions(
            inputs
                .iter()
                .flat_map(|input| input.next_actions.clone())
                .collect(),
        );
        let score_ready = blocker_reasons.is_empty();
        if options.require_scores_ready && !score_ready {
            issues.push(format!(
                "{} is not ready for trusted local scoring",
                definition.metric_family
            ));
        }

        families.push(MetricFamilyReadiness {
            metric_family: definition.metric_family,
            algorithm_id: definition.algorithm_id,
            version: definition.version,
            input_schema: definition.input_schema,
            output_schema: definition.output_schema,
            required_input_count: inputs.len(),
            ready_input_count,
            score_ready,
            inputs,
            blocker_reasons,
            next_actions,
        });
    }

    let ready_family_count = families.iter().filter(|family| family.score_ready).count();
    let mut next_actions = Vec::new();
    if !correlation.pass {
        next_actions.push(MetricInputNextAction {
            scope: "capture_correlation".to_string(),
            reason: "capture_correlation_report_not_passed".to_string(),
            action: "Run Capture Trust and satisfy its owned-capture next actions before promoting score inputs.".to_string(),
        });
    }
    next_actions.extend(
        families
            .iter()
            .flat_map(|family| family.next_actions.clone()),
    );
    let next_actions = dedupe_next_actions(next_actions);

    MetricInputReadinessReport {
        schema: METRIC_INPUT_READINESS_REPORT_SCHEMA.to_string(),
        generated_by: "bull-metric-input-readiness".to_string(),
        pass: issues.is_empty(),
        require_scores_ready: options.require_scores_ready,
        capture_correlation_pass: correlation.pass,
        family_count: families.len(),
        ready_family_count,
        families,
        issues,
        next_actions,
        activity_session_promotion,
    }
}

fn input_readiness(
    input_name: &str,
    unit: Option<String>,
    evidence: &BTreeMap<String, SummaryEvidence>,
) -> MetricInputReadiness {
    let plan = input_plan(input_name);
    let candidate_observation_count = plan
        .required_summary_kinds
        .iter()
        .map(|kind| {
            evidence
                .get(*kind)
                .map_or(0, |summary| summary.observations)
        })
        .sum();
    let trusted_evidence_count = plan
        .required_summary_kinds
        .iter()
        .map(|kind| evidence.get(*kind).map_or(0, |summary| summary.trusted))
        .sum();
    let mut blocker_reasons = Vec::new();
    if !plan.required_summary_kinds.is_empty() && trusted_evidence_count == 0 {
        blocker_reasons.push(format!(
            "no trusted owned capture evidence for {}",
            plan.required_summary_kinds.join("|")
        ));
    }
    if !plan.extraction_ready {
        blocker_reasons.push(plan.blocker.to_string());
    }
    let next_actions = next_actions_for_input(
        input_name,
        &plan,
        candidate_observation_count,
        trusted_evidence_count,
        &blocker_reasons,
    );
    MetricInputReadiness {
        input_name: input_name.to_string(),
        unit,
        source_signal: plan.source_signal.to_string(),
        status: if blocker_reasons.is_empty() {
            "ready".to_string()
        } else {
            "blocked".to_string()
        },
        required_summary_kinds: plan
            .required_summary_kinds
            .iter()
            .map(|kind| kind.to_string())
            .collect(),
        candidate_observation_count,
        trusted_evidence_count,
        blocker_reasons,
        next_actions,
    }
}

fn next_actions_for_input(
    input_name: &str,
    plan: &InputPlan,
    candidate_observation_count: usize,
    trusted_evidence_count: usize,
    blocker_reasons: &[String],
) -> Vec<MetricInputNextAction> {
    let mut actions = Vec::new();
    if !plan.required_summary_kinds.is_empty() && trusted_evidence_count == 0 {
        let summaries = plan.required_summary_kinds.join("|");
        let action = if candidate_observation_count == 0 {
            format!(
                "Import or live-capture owned frames that decode as {summaries}, then rerun Capture Trust and Metric Inputs."
            )
        } else {
            format!(
                "Replace synthetic-only {summaries} candidates with owned live BLE/File captures, then rerun Capture Trust and Metric Inputs."
            )
        };
        actions.push(MetricInputNextAction {
            scope: input_name.to_string(),
            reason: format!("no trusted owned capture evidence for {summaries}"),
            action,
        });
    }
    if !plan.extraction_ready {
        actions.push(MetricInputNextAction {
            scope: input_name.to_string(),
            reason: plan.blocker.to_string(),
            action: extraction_next_action(input_name, plan.blocker),
        });
    }
    if actions.is_empty() && !blocker_reasons.is_empty() {
        actions.push(MetricInputNextAction {
            scope: input_name.to_string(),
            reason: blocker_reasons.join("; "),
            action: format!(
                "Resolve the blocked input path for {input_name}, then rerun Metric Inputs."
            ),
        });
    }
    actions
}

fn extraction_next_action(input_name: &str, blocker: &str) -> String {
    match blocker {
        "respiratory_rate_semantics_unverified" => {
            "Validate normal-history respiratory-rate candidate offsets against owned captures and product/API respiratory values, then allow score promotion.".to_string()
        }
        "hrv_rr_interval_scale_unverified" => {
            "Validate the R17 interval scale against owned packet captures and an external beat-interval reference before allowing HRV or stress score promotion.".to_string()
        }
        "temperature_units_unverified" => {
            "Validate temperature event/history units and delta semantics against owned captures before allowing recovery score promotion.".to_string()
        }
        "input_mapping_not_defined" => {
            format!("Define the source signal and required evidence mapping for {input_name}, then add readiness tests.")
        }
        _ => format!("Implement or repair {blocker} for {input_name}, then rerun Metric Inputs."),
    }
}

fn dedupe_next_actions(actions: Vec<MetricInputNextAction>) -> Vec<MetricInputNextAction> {
    let mut deduped = Vec::new();
    for action in actions {
        if !deduped.iter().any(|existing| existing == &action) {
            deduped.push(action);
        }
    }
    deduped
}

fn summary_evidence(correlation: &CaptureCorrelationReport) -> BTreeMap<String, SummaryEvidence> {
    let mut evidence = BTreeMap::new();
    for summary in &correlation.summaries {
        evidence.insert(
            summary.body_summary_kind.clone(),
            SummaryEvidence {
                observations: summary.observation_count,
                trusted: summary.owned_capture_count,
            },
        );
    }
    evidence
}

fn activity_session_promotion_next_actions(
    blocker_reasons: &[String],
) -> Vec<MetricInputNextAction> {
    let mut actions = Vec::new();

    for reason in blocker_reasons {
        match reason.as_str() {
            "classification_evidence_missing" => actions.push(MetricInputNextAction {
                scope: "activity_session_promotion".to_string(),
                reason: reason.clone(),
                action: "Run the activity candidate classifier on trusted HR, motion, and command-sync windows, then rerun metric readiness with the classifier output.".to_string(),
            }),
            "classification_input_schema_mismatch" => actions.push(MetricInputNextAction {
                scope: "activity_session_promotion".to_string(),
                reason: reason.clone(),
                action: format!(
                    "Set schema to {} before rerunning the activity candidate classifier.",
                    ACTIVITY_CANDIDATE_CLASSIFIER_INPUT_SCHEMA
                ),
            }),
            "missing_heart_rate" => actions.push(MetricInputNextAction {
                scope: "activity_session_promotion".to_string(),
                reason: reason.clone(),
                action: "Capture trusted heart-rate evidence for the activity window and rerun the classifier.".to_string(),
            }),
            "missing_motion" => actions.push(MetricInputNextAction {
                scope: "activity_session_promotion".to_string(),
                reason: reason.clone(),
                action: "Capture motion evidence with gravity samples for the activity window and rerun the classifier.".to_string(),
            }),
            "missing_command_sync" => actions.push(MetricInputNextAction {
                scope: "activity_session_promotion".to_string(),
                reason: reason.clone(),
                action: "Carry command-sync evidence into the feature window before rerunning the classifier.".to_string(),
            }),
            "low_classification_confidence" => actions.push(MetricInputNextAction {
                scope: "activity_session_promotion".to_string(),
                reason: reason.clone(),
                action: "Tighten the window or improve evidence quality until the classifier confidence clears the threshold.".to_string(),
            }),
            "candidate_promotion_not_approved" => actions.push(MetricInputNextAction {
                scope: "activity_session_promotion".to_string(),
                reason: reason.clone(),
                action: "Ask the user to approve candidate promotion before creating an activity session.".to_string(),
            }),
            _ => {}
        }
    }

    dedupe_next_actions(actions)
}

fn input_plan(input_name: &str) -> InputPlan {
    match input_name {
        "rr_intervals_ms" => InputPlan {
            source_signal: "r17_optical_rr_interval_candidates",
            required_summary_kinds: &["r17_optical_or_labrador_filtered"],
            extraction_ready: false,
            blocker: "hrv_rr_interval_scale_unverified",
        },
        "heart_rate_bpm" => InputPlan {
            source_signal: "heart_rate_series",
            required_summary_kinds: &["normal_history", "v18_history"],
            extraction_ready: true,
            blocker: "",
        },
        "average_hr_bpm" | "max_hr_bpm" => InputPlan {
            source_signal: "heart_rate_window_aggregation",
            required_summary_kinds: &["normal_history", "v18_history"],
            extraction_ready: true,
            blocker: "",
        },
        "resting_hr_bpm" | "resting_hr_baseline_bpm" => InputPlan {
            source_signal: "resting_heart_rate_baseline",
            required_summary_kinds: &["normal_history", "v18_history"],
            extraction_ready: true,
            blocker: "",
        },
        "duration_minutes" => InputPlan {
            source_signal: "activity_heart_rate_window",
            required_summary_kinds: &["normal_history", "v18_history"],
            extraction_ready: true,
            blocker: "",
        },
        "hr_zone_minutes" => InputPlan {
            source_signal: "heart_rate_zone_aggregation",
            required_summary_kinds: &["normal_history", "v18_history"],
            extraction_ready: true,
            blocker: "",
        },
        "motion_intensity_0_to_1" => InputPlan {
            source_signal: "motion_summary",
            required_summary_kinds: &["raw_motion_k10", "raw_motion_k21"],
            extraction_ready: true,
            blocker: "",
        },
        "hrv_rmssd_ms" => InputPlan {
            source_signal: "hrv_algorithm_output",
            required_summary_kinds: &["r17_optical_or_labrador_filtered"],
            extraction_ready: false,
            blocker: "hrv_rr_interval_scale_unverified",
        },
        "hrv_baseline_rmssd_ms" => InputPlan {
            source_signal: "hrv_baseline_output",
            required_summary_kinds: &["r17_optical_or_labrador_filtered"],
            extraction_ready: false,
            blocker: "hrv_rr_interval_scale_unverified",
        },
        "sleep_need_minutes" => InputPlan {
            source_signal: "user_sleep_need_setting",
            required_summary_kinds: &[],
            extraction_ready: true,
            blocker: "",
        },
        "sleep_duration_minutes"
        | "time_in_bed_minutes"
        | "midpoint_deviation_minutes"
        | "disturbance_count" => InputPlan {
            source_signal: "sleep_motion_window_detector",
            required_summary_kinds: &["raw_motion_k10", "raw_motion_k21"],
            extraction_ready: true,
            blocker: "",
        },
        "sleep_score_0_to_100" => InputPlan {
            source_signal: "sleep_score_from_features",
            required_summary_kinds: &["raw_motion_k10", "raw_motion_k21"],
            extraction_ready: true,
            blocker: "",
        },
        "prior_strain_0_to_21" => InputPlan {
            source_signal: "strain_score_from_features",
            required_summary_kinds: &["normal_history", "v18_history"],
            extraction_ready: true,
            blocker: "",
        },
        "respiratory_rate_rpm" | "respiratory_rate_baseline_rpm" => InputPlan {
            source_signal: "normal_history_respiratory_rate_candidate",
            required_summary_kinds: &["normal_history", "v18_history"],
            extraction_ready: false,
            blocker: "respiratory_rate_semantics_unverified",
        },
        "skin_temp_delta_c" => InputPlan {
            source_signal: "normal_history_or_event_temperature_candidate",
            required_summary_kinds: &["normal_history", "event_temperature_level", "v18_history"],
            extraction_ready: false,
            blocker: "temperature_units_unverified",
        },
        _ => InputPlan {
            source_signal: "unknown",
            required_summary_kinds: &[],
            extraction_ready: false,
            blocker: "input_mapping_not_defined",
        },
    }
}
