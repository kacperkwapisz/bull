//! RR ↔ HR internal-consistency verification.
//!
//! WHOOP 5.0 V24 history packets carry the device's own reported heart rate
//! (`hr`) co-located with a small set of RR intervals (`rr_intervals_ms`) in the
//! same packet body. If those RR intervals are genuinely beat-to-beat intervals
//! expressed in milliseconds, then `60000 / mean(RR_ms)` must reproduce the
//! reported instantaneous heart rate for the same packet.
//!
//! This module proves (or disproves) the millisecond scale of the V24 RR field
//! using nothing but device-internal data. It never reads official WHOOP-derived
//! values, so it is safe under the project label policy: the reference here is
//! the band's own HR field, not a WHOOP score or label.
//!
//! The verifier is intentionally pure: callers pass decoded frames (or, in unit
//! tests, raw `(hr, rr_ms)` pairs) and receive a schema-tagged report. It does
//! not mutate any trust gate by itself; promoting the HRV pipeline to trust the
//! V24 RR source remains a separate, evidence-gated decision driven by the
//! `verdict` this module emits.

use serde::{Deserialize, Serialize};

use crate::{
    BullError, BullResult,
    protocol::{DataPacketBodySummary, ParsedPayload},
    store::DecodedFrameRow,
};

pub const RR_HR_CONSISTENCY_REPORT_SCHEMA: &str = "bull.rr-hr-consistency-report.v1";
pub const RR_HR_CONSISTENCY_GENERATED_BY: &str = "bull-rr-hr-consistency-verifier";
pub const RR_HR_CONSISTENCY_SCALE_BASIS: &str = "v24_history_rr_intervals_ms";
/// The verifier only ever compares against the band's own reported HR field; it
/// never ingests official WHOOP scores or labels.
pub const RR_HR_CONSISTENCY_LABEL_POLICY: &str = "device_internal_hr_only_no_official_labels";

/// Lowest plausible beat-to-beat interval in milliseconds (≈200 bpm).
pub const MIN_PLAUSIBLE_RR_MS: f64 = 300.0;
/// Highest plausible beat-to-beat interval in milliseconds (≈30 bpm).
pub const MAX_PLAUSIBLE_RR_MS: f64 = 2000.0;

const MAX_EVIDENCE_ROWS: usize = 50;

#[derive(Debug, Clone, Copy)]
pub struct RrHrConsistencyOptions {
    /// A frame is consistent when the implied HR is within this many bpm of the
    /// reported HR (absolute tolerance, applied for low/normal heart rates).
    pub max_hr_abs_error_bpm: f64,
    /// A frame is consistent when the implied HR is within this fraction of the
    /// reported HR (relative tolerance, applied for elevated heart rates).
    pub max_hr_fractional_error: f64,
    /// Minimum RR intervals required in a frame for it to count as eligible.
    pub min_rr_intervals_per_frame: usize,
    /// Minimum eligible frames required before a verdict other than
    /// `insufficient_data` can be emitted.
    pub min_eligible_frames: usize,
    /// Minimum fraction of eligible frames that must be consistent to verify the
    /// millisecond scale.
    pub consistency_pass_ratio: f64,
}

impl Default for RrHrConsistencyOptions {
    fn default() -> Self {
        Self {
            max_hr_abs_error_bpm: 8.0,
            max_hr_fractional_error: 0.12,
            min_rr_intervals_per_frame: 2,
            min_eligible_frames: 20,
            consistency_pass_ratio: 0.8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RrHrConsistencyVerdict {
    /// Eligible frames met the count threshold and the consistency ratio passed:
    /// the V24 RR field reproduces device HR in milliseconds.
    Verified,
    /// Enough eligible frames, but too many disagree with reported HR: the
    /// millisecond interpretation is not supported by this data.
    Inconsistent,
    /// Not enough eligible frames to draw any conclusion yet.
    InsufficientData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RrHrFrameEvaluation {
    pub frame_id: String,
    pub evidence_id: String,
    pub captured_at: String,
    pub reported_hr_bpm: f64,
    pub plausible_rr_interval_count: usize,
    pub mean_rr_ms: f64,
    pub implied_hr_bpm: f64,
    pub abs_error_bpm: f64,
    pub fractional_error: f64,
    pub consistent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RrHrConsistencyOptionsSnapshot {
    pub max_hr_abs_error_bpm: f64,
    pub max_hr_fractional_error: f64,
    pub min_rr_intervals_per_frame: usize,
    pub min_eligible_frames: usize,
    pub consistency_pass_ratio: f64,
}

impl From<RrHrConsistencyOptions> for RrHrConsistencyOptionsSnapshot {
    fn from(options: RrHrConsistencyOptions) -> Self {
        Self {
            max_hr_abs_error_bpm: options.max_hr_abs_error_bpm,
            max_hr_fractional_error: options.max_hr_fractional_error,
            min_rr_intervals_per_frame: options.min_rr_intervals_per_frame,
            min_eligible_frames: options.min_eligible_frames,
            consistency_pass_ratio: options.consistency_pass_ratio,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RrHrConsistencyReport {
    pub schema: String,
    pub generated_by: String,
    pub scale_basis_under_test: String,
    pub label_policy: String,
    pub verdict: RrHrConsistencyVerdict,
    /// Total decoded frames scanned in the requested range (all packet kinds).
    pub decoded_frame_count: usize,
    /// Decoded frames whose body is a V24 history body (regardless of whether
    /// they carry HR and RR). Diagnoses whether historical sync has run.
    pub v24_history_frame_count: usize,
    /// V24 frames that carry both a non-zero HR and at least one RR interval.
    pub candidate_frame_count: usize,
    pub eligible_frame_count: usize,
    pub consistent_frame_count: usize,
    pub consistency_ratio: f64,
    pub mean_abs_error_bpm: Option<f64>,
    pub options: RrHrConsistencyOptionsSnapshot,
    pub evidence: Vec<RrHrFrameEvaluation>,
    pub blockers: Vec<String>,
    pub next_actions: Vec<String>,
}

/// A single frame's reported HR plus its RR intervals, decoupled from frame
/// storage so the core scoring can be unit-tested directly.
#[derive(Debug, Clone)]
pub struct RrHrFrameInput {
    pub frame_id: String,
    pub evidence_id: String,
    pub captured_at: String,
    pub reported_hr_bpm: f64,
    pub rr_intervals_ms: Vec<f64>,
}

/// Evaluate RR↔HR consistency from already-extracted frame inputs.
pub fn evaluate_rr_hr_consistency(
    inputs: &[RrHrFrameInput],
    options: RrHrConsistencyOptions,
) -> RrHrConsistencyReport {
    let candidate_frame_count = inputs.len();
    let mut evaluations: Vec<RrHrFrameEvaluation> = Vec::new();

    for input in inputs {
        if !(input.reported_hr_bpm > 0.0) {
            continue;
        }
        let plausible: Vec<f64> = input
            .rr_intervals_ms
            .iter()
            .copied()
            .filter(|rr| (MIN_PLAUSIBLE_RR_MS..=MAX_PLAUSIBLE_RR_MS).contains(rr))
            .collect();
        if plausible.len() < options.min_rr_intervals_per_frame {
            continue;
        }
        let mean_rr_ms = plausible.iter().sum::<f64>() / plausible.len() as f64;
        if !(mean_rr_ms > 0.0) {
            continue;
        }
        let implied_hr_bpm = 60_000.0 / mean_rr_ms;
        let abs_error_bpm = (implied_hr_bpm - input.reported_hr_bpm).abs();
        let fractional_error = abs_error_bpm / input.reported_hr_bpm;
        let consistent = abs_error_bpm <= options.max_hr_abs_error_bpm
            || fractional_error <= options.max_hr_fractional_error;

        evaluations.push(RrHrFrameEvaluation {
            frame_id: input.frame_id.clone(),
            evidence_id: input.evidence_id.clone(),
            captured_at: input.captured_at.clone(),
            reported_hr_bpm: input.reported_hr_bpm,
            plausible_rr_interval_count: plausible.len(),
            mean_rr_ms,
            implied_hr_bpm,
            abs_error_bpm,
            fractional_error,
            consistent,
        });
    }

    let eligible_frame_count = evaluations.len();
    let consistent_frame_count = evaluations.iter().filter(|e| e.consistent).count();
    let consistency_ratio = if eligible_frame_count == 0 {
        0.0
    } else {
        consistent_frame_count as f64 / eligible_frame_count as f64
    };
    let mean_abs_error_bpm = if eligible_frame_count == 0 {
        None
    } else {
        Some(evaluations.iter().map(|e| e.abs_error_bpm).sum::<f64>() / eligible_frame_count as f64)
    };

    let mut blockers = Vec::new();
    let mut next_actions = Vec::new();
    let verdict = if eligible_frame_count < options.min_eligible_frames {
        blockers.push("insufficient_eligible_v24_rr_hr_frames".to_string());
        next_actions.push(format!(
            "Capture more worn V24 history frames carrying both HR and RR; need at least {} eligible frames (have {}).",
            options.min_eligible_frames, eligible_frame_count
        ));
        RrHrConsistencyVerdict::InsufficientData
    } else if consistency_ratio >= options.consistency_pass_ratio {
        next_actions.push(
            "Scale verified from device-internal HR; the V24 rr_intervals_ms field may be promoted to a trusted HRV source under the readiness gate.".to_string(),
        );
        RrHrConsistencyVerdict::Verified
    } else {
        blockers.push("rr_hr_consistency_below_threshold".to_string());
        next_actions.push(format!(
            "Implied HR disagrees with reported HR in {:.0}% of eligible frames; do not treat the V24 RR field as milliseconds. Re-examine the byte layout/scale before promotion.",
            (1.0 - consistency_ratio) * 100.0
        ));
        RrHrConsistencyVerdict::Inconsistent
    };

    let mut evidence = evaluations;
    evidence.truncate(MAX_EVIDENCE_ROWS);

    RrHrConsistencyReport {
        schema: RR_HR_CONSISTENCY_REPORT_SCHEMA.to_string(),
        generated_by: RR_HR_CONSISTENCY_GENERATED_BY.to_string(),
        scale_basis_under_test: RR_HR_CONSISTENCY_SCALE_BASIS.to_string(),
        label_policy: RR_HR_CONSISTENCY_LABEL_POLICY.to_string(),
        verdict,
        decoded_frame_count: candidate_frame_count,
        v24_history_frame_count: candidate_frame_count,
        candidate_frame_count,
        eligible_frame_count,
        consistent_frame_count,
        consistency_ratio,
        mean_abs_error_bpm,
        options: options.into(),
        evidence,
        blockers,
        next_actions,
    }
}

/// Extract `(hr, rr_ms)` inputs from decoded frames and evaluate consistency.
///
/// Only V24 history bodies that carry a non-zero HR are considered; frames
/// without the V24 body, without an HR, or with no RR intervals are skipped (and
/// do not count toward the candidate total).
pub fn run_rr_hr_consistency_report(
    decoded_rows: &[DecodedFrameRow],
    options: RrHrConsistencyOptions,
) -> BullResult<RrHrConsistencyReport> {
    let mut inputs = Vec::new();
    let mut v24_history_frame_count = 0usize;
    for row in decoded_rows {
        if row_is_v24_history(row)? {
            v24_history_frame_count += 1;
        }
        let Some(input) = rr_hr_input_from_row(row)? else {
            continue;
        };
        inputs.push(input);
    }
    let mut report = evaluate_rr_hr_consistency(&inputs, options);
    report.decoded_frame_count = decoded_rows.len();
    report.v24_history_frame_count = v24_history_frame_count;
    Ok(report)
}

fn row_is_v24_history(row: &DecodedFrameRow) -> BullResult<bool> {
    let parsed_payload: ParsedPayload =
        serde_json::from_str(&row.parsed_payload_json).map_err(|error| {
            BullError::message(format!(
                "{} parsed_payload_json invalid: {error}",
                row.frame_id
            ))
        })?;
    Ok(matches!(
        parsed_payload,
        ParsedPayload::DataPacket {
            body_summary: Some(DataPacketBodySummary::V24History { .. }),
            ..
        }
    ))
}

fn rr_hr_input_from_row(row: &DecodedFrameRow) -> BullResult<Option<RrHrFrameInput>> {
    let parsed_payload: ParsedPayload =
        serde_json::from_str(&row.parsed_payload_json).map_err(|error| {
            BullError::message(format!(
                "{} parsed_payload_json invalid: {error}",
                row.frame_id
            ))
        })?;

    let ParsedPayload::DataPacket {
        body_summary:
            Some(DataPacketBodySummary::V24History {
                hr: Some(hr),
                rr_intervals_ms,
                ..
            }),
        ..
    } = parsed_payload
    else {
        return Ok(None);
    };

    if hr == 0 || rr_intervals_ms.is_empty() {
        return Ok(None);
    }

    Ok(Some(RrHrFrameInput {
        frame_id: row.frame_id.clone(),
        evidence_id: row.evidence_id.clone(),
        captured_at: row.captured_at.clone(),
        reported_hr_bpm: f64::from(hr),
        rr_intervals_ms: rr_intervals_ms.into_iter().map(f64::from).collect(),
    }))
}
