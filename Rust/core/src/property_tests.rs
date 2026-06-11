use std::{
    collections::{BTreeMap, BTreeSet},
    panic::{AssertUnwindSafe, catch_unwind},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    BullError, BullResult,
    metrics::{
        HrvInput, RecoveryInput, SleepInput, SleepModelStatusInput, SleepV1Input, StrainInput,
        StressInput, bull_hrv_v0, bull_recovery_v0, bull_sleep_v0, bull_sleep_v1,
        bull_strain_v0, bull_stress_v0,
    },
    protocol::{DeviceType, FrameAccumulator, build_v5_payload_frame, parse_frame},
};

pub const PROPERTY_TEST_REPORT_SCHEMA: &str = "bull.property-test-report.v1";
pub const DEFAULT_PROPERTY_SEED: u64 = 0x676f_6f73_655f_7031;
pub const DEFAULT_CASES_PER_GROUP: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PropertySuiteOptions {
    pub seed: u64,
    pub cases_per_group: usize,
}

impl Default for PropertySuiteOptions {
    fn default() -> Self {
        Self {
            seed: DEFAULT_PROPERTY_SEED,
            cases_per_group: DEFAULT_CASES_PER_GROUP,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PropertySuiteReport {
    pub schema: String,
    pub generated_by: String,
    pub seed: u64,
    pub cases_per_group: usize,
    pub pass: bool,
    #[serde(default)]
    pub input_valid: bool,
    #[serde(default)]
    pub parser_properties_valid: bool,
    #[serde(default)]
    pub deframer_properties_valid: bool,
    #[serde(default)]
    pub algorithm_bounds_valid: bool,
    #[serde(default)]
    pub algorithm_metamorphic_valid: bool,
    #[serde(default)]
    pub all_groups_valid: bool,
    #[serde(default)]
    pub property_suite_ready: bool,
    pub groups: Vec<PropertyGroupReport>,
    pub issues: Vec<String>,
    #[serde(default)]
    pub next_actions: Vec<PropertySuiteNextAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PropertyGroupReport {
    pub name: String,
    pub pass: bool,
    pub cases: usize,
    pub checks: usize,
    pub failures: Vec<PropertyFailure>,
    #[serde(default)]
    pub next_actions: Vec<PropertySuiteNextAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct PropertySuiteNextAction {
    pub scope: String,
    pub reason: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PropertyFailure {
    pub case_index: usize,
    pub property: String,
    pub message: String,
    pub context: Value,
}

pub fn run_property_suite(options: PropertySuiteOptions) -> BullResult<PropertySuiteReport> {
    if options.cases_per_group == 0 {
        return Err(BullError::message(
            "cases_per_group must be greater than 0",
        ));
    }

    let mut rng = DeterministicRng::new(options.seed);
    let groups = vec![
        parser_frame_properties(&mut rng, options.cases_per_group),
        deframer_properties(&mut rng, options.cases_per_group),
        algorithm_bounds_properties(&mut rng, options.cases_per_group),
        algorithm_metamorphic_properties(options.cases_per_group),
    ];
    Ok(property_suite_report_from_groups(
        options.seed,
        options.cases_per_group,
        groups,
    ))
}

pub fn property_suite_report_from_groups(
    seed: u64,
    cases_per_group: usize,
    groups: Vec<PropertyGroupReport>,
) -> PropertySuiteReport {
    let input_valid = cases_per_group > 0;
    let parser_properties_valid = property_group_valid(&groups, "parser_frame_invariants");
    let deframer_properties_valid = property_group_valid(&groups, "deframer_stream_invariants");
    let algorithm_bounds_valid =
        property_group_valid(&groups, "algorithm_bounds_and_quality_invariants");
    let algorithm_metamorphic_valid =
        property_group_valid(&groups, "algorithm_metamorphic_invariants");
    let all_groups_valid = parser_properties_valid
        && deframer_properties_valid
        && algorithm_bounds_valid
        && algorithm_metamorphic_valid
        && groups.iter().all(|group| group.pass);
    let issues = groups
        .iter()
        .filter(|group| !group.pass)
        .map(|group| format!("{} failed {} checks", group.name, group.failures.len()))
        .collect::<Vec<_>>();
    let next_actions = property_suite_next_actions(&groups);
    let property_suite_ready = input_valid && all_groups_valid && issues.is_empty();
    let pass = property_suite_ready;

    PropertySuiteReport {
        schema: PROPERTY_TEST_REPORT_SCHEMA.to_string(),
        generated_by: "bull-property-test-suite".to_string(),
        seed,
        cases_per_group,
        pass,
        input_valid,
        parser_properties_valid,
        deframer_properties_valid,
        algorithm_bounds_valid,
        algorithm_metamorphic_valid,
        all_groups_valid,
        property_suite_ready,
        groups,
        issues,
        next_actions,
    }
}

fn property_group_valid(groups: &[PropertyGroupReport], name: &str) -> bool {
    groups.iter().any(|group| group.name == name && group.pass)
}

pub fn property_suite_next_actions(groups: &[PropertyGroupReport]) -> Vec<PropertySuiteNextAction> {
    dedupe_property_next_actions(
        groups
            .iter()
            .flat_map(|group| group.next_actions.iter().cloned())
            .collect(),
    )
}

pub fn property_group_next_actions(
    group_name: &str,
    failures: &[PropertyFailure],
) -> Vec<PropertySuiteNextAction> {
    dedupe_property_next_actions(
        failures
            .iter()
            .map(|failure| {
                let reason = property_failure_reason(group_name, &failure.property);
                PropertySuiteNextAction {
                    scope: format!(
                        "{group_name}:{}:case_{}",
                        failure.property, failure.case_index
                    ),
                    reason: reason.to_string(),
                    action: property_failure_action(group_name, &failure.property, reason)
                        .to_string(),
                }
            })
            .collect(),
    )
}

fn property_failure_reason(group_name: &str, property: &str) -> &'static str {
    if property.contains("no_panic") {
        "panic_safety_failure"
    } else if group_name == "parser_frame_invariants"
        && (property.contains("length")
            || property.contains("crc")
            || property.contains("payload")
            || property.contains("parses"))
    {
        "parser_frame_invariant_failure"
    } else if group_name == "deframer_stream_invariants" {
        "deframer_stream_invariant_failure"
    } else if property.contains("valid_generated_input_produces_output") {
        "algorithm_output_failure"
    } else if property.contains("outputs_are_finite_and_bounded")
        || property.contains("counts_are_consistent")
    {
        "algorithm_bounds_failure"
    } else if group_name == "algorithm_metamorphic_invariants" {
        "algorithm_metamorphic_failure"
    } else {
        "property_failure"
    }
}

fn property_failure_action(group_name: &str, property: &str, reason: &str) -> &'static str {
    match reason {
        "panic_safety_failure" if group_name == "parser_frame_invariants" => {
            "Re-run the property suite with the same seed/case, capture the failing bytes from context, and add a parser no-panic regression before trusting new packet parsing."
        }
        "panic_safety_failure" if group_name == "deframer_stream_invariants" => {
            "Re-run the property suite with the same seed/case, capture the split stream from context, and add a deframer no-panic regression before trusting stream capture."
        }
        "parser_frame_invariant_failure" if property.contains("corruption") => {
            "Fix corrupted-frame handling so CRC mismatches stay parseable, warned, and non-panicking, then add the failing frame as a parser regression fixture."
        }
        "parser_frame_invariant_failure" => {
            "Fix the frame builder/parser length, CRC, or payload-preservation invariant and add the failing frame as a regression fixture."
        }
        "deframer_stream_invariant_failure" => {
            "Fix split-stream deframing so extracted frames and dropped-prefix counts match the generated stream, then add the seed/case as a deframer regression."
        }
        "algorithm_output_failure" => {
            "Fix the generated Bull score input or quality gate so valid generated inputs produce an output, then add a deterministic algorithm regression."
        }
        "algorithm_bounds_failure" => {
            "Clamp or correct the Bull score calculation so finite generated inputs stay within documented bounds and component counts remain consistent."
        }
        "algorithm_metamorphic_failure" => {
            "Review the Bull score formula for this monotonic relationship and add a hand-derived regression before changing expected behavior."
        }
        _ => {
            "Inspect the failing property context and add a targeted regression before trusting this parser or calculation path."
        }
    }
}

fn dedupe_property_next_actions(
    actions: Vec<PropertySuiteNextAction>,
) -> Vec<PropertySuiteNextAction> {
    let mut seen = BTreeSet::new();
    let mut deduped = Vec::new();
    for action in actions {
        let key = format!("{}:{}:{}", action.scope, action.reason, action.action);
        if seen.insert(key) {
            deduped.push(action);
        }
    }
    deduped
}

fn parser_frame_properties(rng: &mut DeterministicRng, cases: usize) -> PropertyGroupReport {
    let mut group = GroupBuilder::new("parser_frame_invariants", cases);

    for case_index in 0..cases {
        let payload = random_payload(rng);
        let frame = build_v5_payload_frame(&payload);
        let parsed_result =
            catch_unwind(AssertUnwindSafe(|| parse_frame(DeviceType::Bull, &frame)));
        match parsed_result {
            Ok(Ok(parsed)) => {
                let padded_payload = &frame[8..frame.len() - 4];
                group.check(
                    case_index,
                    "valid_built_frame_has_consistent_lengths",
                    parsed.raw_len == frame.len()
                        && parsed.header_len == 8
                        && parsed.declared_len + parsed.header_len == parsed.raw_len,
                    "parsed frame lengths must match the local builder output",
                    json!({
                        "raw_len": parsed.raw_len,
                        "declared_len": parsed.declared_len,
                        "frame_len": frame.len()
                    }),
                );
                group.check(
                    case_index,
                    "valid_built_frame_crc_passes",
                    parsed.header_crc_valid && parsed.payload_crc_valid,
                    "locally built frames must pass both CRC checks",
                    json!({
                        "header_crc_valid": parsed.header_crc_valid,
                        "payload_crc_valid": parsed.payload_crc_valid
                    }),
                );
                group.check(
                    case_index,
                    "valid_built_frame_preserves_payload_hex",
                    parsed.payload_hex == hex::encode(padded_payload),
                    "parser must preserve the padded payload bytes as hex",
                    json!({
                        "expected_payload_hex": hex::encode(padded_payload),
                        "actual_payload_hex": parsed.payload_hex
                    }),
                );
            }
            Ok(Err(error)) => group.fail(
                case_index,
                "valid_built_frame_parses",
                format!("locally built frame failed to parse: {error}"),
                json!({"frame_hex": hex::encode(&frame)}),
            ),
            Err(_) => group.fail(
                case_index,
                "valid_built_frame_no_panic",
                "parser panicked on a locally built frame",
                json!({"frame_hex": hex::encode(&frame)}),
            ),
        }

        let mut corrupted = frame.clone();
        if corrupted.len() > 12 {
            let offset = 8 + rng.usize(corrupted.len() - 12);
            corrupted[offset] ^= 0x55;
            match catch_unwind(AssertUnwindSafe(|| {
                parse_frame(DeviceType::Bull, &corrupted)
            })) {
                Ok(Ok(parsed)) => {
                    group.check(
                        case_index,
                        "payload_corruption_keeps_parseable_header",
                        parsed.header_crc_valid && !parsed.payload_crc_valid,
                        "payload corruption should not invalidate the header CRC and must fail payload CRC",
                        json!({
                            "header_crc_valid": parsed.header_crc_valid,
                            "payload_crc_valid": parsed.payload_crc_valid,
                            "offset": offset
                        }),
                    );
                    group.check(
                        case_index,
                        "payload_corruption_is_warned",
                        parsed
                            .warnings
                            .iter()
                            .any(|warning| warning == "payload_crc_mismatch"),
                        "payload CRC mismatches must be visible in parser warnings",
                        json!({"warnings": parsed.warnings}),
                    );
                }
                Ok(Err(error)) => group.fail(
                    case_index,
                    "payload_corruption_preserves_safe_result",
                    format!("corrupted frame returned a hard parse error: {error}"),
                    json!({"frame_hex": hex::encode(&corrupted)}),
                ),
                Err(_) => group.fail(
                    case_index,
                    "payload_corruption_no_panic",
                    "parser panicked on a corrupted frame",
                    json!({"frame_hex": hex::encode(&corrupted)}),
                ),
            }
        }

        let noise_len = rng.usize(160);
        let noise = rng.bytes(noise_len);
        if catch_unwind(AssertUnwindSafe(|| parse_frame(DeviceType::Bull, &noise))).is_err() {
            group.fail(
                case_index,
                "arbitrary_bytes_no_panic",
                "parser panicked on arbitrary bytes",
                json!({"byte_len": noise.len()}),
            );
        } else {
            group.count_check();
        }
    }

    group.finish()
}

fn deframer_properties(rng: &mut DeterministicRng, cases: usize) -> PropertyGroupReport {
    let mut group = GroupBuilder::new("deframer_stream_invariants", cases);

    for case_index in 0..cases {
        let frame_count = 1 + rng.usize(5);
        let mut expected_frames = Vec::new();
        let mut stream = Vec::new();
        let mut expected_dropped_prefix_len = 0usize;

        for _ in 0..frame_count {
            let prefix_len = rng.usize(4);
            expected_dropped_prefix_len += prefix_len;
            for _ in 0..prefix_len {
                stream.push(non_frame_start_byte(rng));
            }
            let frame = build_v5_payload_frame(&random_payload(rng));
            stream.extend_from_slice(&frame);
            expected_frames.push(frame);
        }

        let mut accumulator = FrameAccumulator::new(DeviceType::Bull);
        let mut extracted = Vec::new();
        let mut dropped_prefix_len = 0usize;
        let mut offset = 0usize;
        let mut panicked = false;
        while offset < stream.len() {
            let chunk_len = 1 + rng.usize(9);
            let next_offset = (offset + chunk_len).min(stream.len());
            match catch_unwind(AssertUnwindSafe(|| {
                accumulator.feed(&stream[offset..next_offset])
            })) {
                Ok(result) => {
                    dropped_prefix_len += result.dropped_prefix_len;
                    extracted.extend(result.frames);
                }
                Err(_) => {
                    panicked = true;
                    break;
                }
            }
            offset = next_offset;
        }

        group.check(
            case_index,
            "deframer_no_panic",
            !panicked,
            "deframer must not panic on split streams",
            json!({"stream_len": stream.len()}),
        );
        group.check(
            case_index,
            "deframer_reassembles_exact_frames",
            extracted == expected_frames,
            "deframer must extract locally built frames without changing bytes",
            json!({
                "expected_frame_count": expected_frames.len(),
                "actual_frame_count": extracted.len()
            }),
        );
        group.check(
            case_index,
            "deframer_drops_only_prefix_noise",
            dropped_prefix_len == expected_dropped_prefix_len,
            "deframer should only report deliberately inserted non-frame prefix bytes as dropped",
            json!({
                "expected_dropped_prefix_len": expected_dropped_prefix_len,
                "actual_dropped_prefix_len": dropped_prefix_len
            }),
        );
    }

    group.finish()
}

fn algorithm_bounds_properties(rng: &mut DeterministicRng, cases: usize) -> PropertyGroupReport {
    let mut group = GroupBuilder::new("algorithm_bounds_and_quality_invariants", cases * 6);

    for case_index in 0..cases {
        check_hrv_bounds(&mut group, rng, case_index);
        check_sleep_bounds(&mut group, rng, case_index);
        check_sleep_v1_bounds(&mut group, rng, case_index);
        check_strain_bounds(&mut group, rng, case_index);
        check_recovery_bounds(&mut group, rng, case_index);
        check_stress_bounds(&mut group, rng, case_index);
    }

    group.finish()
}

fn algorithm_metamorphic_properties(cases: usize) -> PropertyGroupReport {
    let mut group = GroupBuilder::new("algorithm_metamorphic_invariants", cases);

    for case_index in 0..cases {
        let sleep_short = bull_sleep_v0(&SleepInput {
            start_time: "2026-05-28T00:00:00Z".to_string(),
            end_time: "2026-05-28T08:00:00Z".to_string(),
            sleep_duration_minutes: 300.0,
            sleep_need_minutes: 480.0,
            time_in_bed_minutes: 480.0,
            midpoint_deviation_minutes: 30.0,
            disturbance_count: 3,
            input_ids: Vec::new(),
            ..Default::default()
        })
        .output
        .expect("valid sleep input");
        let sleep_long = bull_sleep_v0(&SleepInput {
            sleep_duration_minutes: 360.0,
            ..base_sleep_input()
        })
        .output
        .expect("valid sleep input");
        group.check(
            case_index,
            "sleep_duration_improvement_non_decreasing",
            sleep_long.score_0_to_100 >= sleep_short.score_0_to_100,
            "increasing sleep duration with the same need and time-in-bed should not lower the score",
            json!({
                "short": sleep_short.score_0_to_100,
                "long": sleep_long.score_0_to_100
            }),
        );

        let sleep_v1_short = bull_sleep_v1(&SleepV1Input {
            sleep: SleepInput {
                sleep_duration_minutes: 300.0,
                wake_after_sleep_onset_minutes: 30.0,
                wake_episode_count: 2,
                stage_minutes: BTreeMap::from([
                    ("core".to_string(), 180.0),
                    ("deep".to_string(), 55.0),
                    ("rem".to_string(), 65.0),
                ]),
                heart_rate_dip_percent: Some(12.0),
                ..base_sleep_input()
            },
            model_status: base_sleep_v1_status_input(),
            bedtime_deviation_minutes: 20.0,
            wake_time_deviation_minutes: 20.0,
            data_coverage_fraction: Some(0.90),
            ..Default::default()
        })
        .output
        .expect("valid sleep v1 input");
        let sleep_v1_long = bull_sleep_v1(&SleepV1Input {
            sleep: SleepInput {
                sleep_duration_minutes: 360.0,
                wake_after_sleep_onset_minutes: 30.0,
                wake_episode_count: 2,
                stage_minutes: BTreeMap::from([
                    ("core".to_string(), 220.0),
                    ("deep".to_string(), 65.0),
                    ("rem".to_string(), 75.0),
                ]),
                heart_rate_dip_percent: Some(12.0),
                ..base_sleep_input()
            },
            model_status: base_sleep_v1_status_input(),
            bedtime_deviation_minutes: 20.0,
            wake_time_deviation_minutes: 20.0,
            data_coverage_fraction: Some(0.90),
            ..Default::default()
        })
        .output
        .expect("valid sleep v1 input");
        group.check(
            case_index,
            "sleep_v1_duration_improvement_non_decreasing",
            sleep_v1_long.score_0_to_100 >= sleep_v1_short.score_0_to_100,
            "raising Sleep V1 duration while holding need and continuity steady should not lower the score",
            json!({
                "short": sleep_v1_short.score_0_to_100,
                "long": sleep_v1_long.score_0_to_100
            }),
        );

        let sleep_v1_low_waso = bull_sleep_v1(&SleepV1Input {
            sleep: SleepInput {
                wake_after_sleep_onset_minutes: 20.0,
                wake_episode_count: 2,
                sleep_latency_minutes: 15.0,
                stage_minutes: BTreeMap::from([
                    ("core".to_string(), 220.0),
                    ("deep".to_string(), 80.0),
                    ("rem".to_string(), 100.0),
                ]),
                heart_rate_dip_percent: Some(12.0),
                ..base_sleep_input()
            },
            model_status: base_sleep_v1_status_input(),
            bedtime_deviation_minutes: 20.0,
            wake_time_deviation_minutes: 20.0,
            data_coverage_fraction: Some(0.90),
            ..Default::default()
        })
        .output
        .expect("valid sleep v1 input");
        let sleep_v1_high_waso = bull_sleep_v1(&SleepV1Input {
            sleep: SleepInput {
                wake_after_sleep_onset_minutes: 90.0,
                wake_episode_count: 2,
                sleep_latency_minutes: 15.0,
                stage_minutes: BTreeMap::from([
                    ("core".to_string(), 220.0),
                    ("deep".to_string(), 80.0),
                    ("rem".to_string(), 100.0),
                ]),
                heart_rate_dip_percent: Some(12.0),
                ..base_sleep_input()
            },
            model_status: base_sleep_v1_status_input(),
            bedtime_deviation_minutes: 20.0,
            wake_time_deviation_minutes: 20.0,
            data_coverage_fraction: Some(0.90),
            ..Default::default()
        })
        .output
        .expect("valid sleep v1 input");
        group.check(
            case_index,
            "sleep_v1_waso_penalty_non_increasing",
            sleep_v1_high_waso.score_0_to_100 <= sleep_v1_low_waso.score_0_to_100,
            "raising Sleep V1 wake-after-sleep-onset while holding other inputs steady should not raise the score",
            json!({
                "low_waso": sleep_v1_low_waso.score_0_to_100,
                "high_waso": sleep_v1_high_waso.score_0_to_100
            }),
        );

        let low_strain = bull_strain_v0(&StrainInput {
            hr_zone_minutes: vec![60.0, 0.0, 0.0, 0.0, 0.0],
            ..base_strain_input()
        })
        .output
        .expect("valid strain input");
        let high_strain = bull_strain_v0(&StrainInput {
            hr_zone_minutes: vec![0.0, 0.0, 0.0, 0.0, 60.0],
            ..base_strain_input()
        })
        .output
        .expect("valid strain input");
        group.check(
            case_index,
            "strain_zone_shift_non_decreasing",
            high_strain.score_0_to_21 >= low_strain.score_0_to_21,
            "moving the same minutes from zone 1 to zone 5 should not lower strain",
            json!({
                "low_zone_score": low_strain.score_0_to_21,
                "high_zone_score": high_strain.score_0_to_21
            }),
        );

        let recovery_low_hrv = bull_recovery_v0(&RecoveryInput {
            hrv_rmssd_ms: 40.0,
            ..base_recovery_input()
        })
        .output
        .expect("valid recovery input");
        let recovery_high_hrv = bull_recovery_v0(&RecoveryInput {
            hrv_rmssd_ms: 60.0,
            ..base_recovery_input()
        })
        .output
        .expect("valid recovery input");
        group.check(
            case_index,
            "recovery_hrv_improvement_non_decreasing",
            recovery_high_hrv.score_0_to_100 >= recovery_low_hrv.score_0_to_100,
            "raising HRV while holding other recovery inputs fixed should not lower recovery",
            json!({
                "low_hrv_score": recovery_low_hrv.score_0_to_100,
                "high_hrv_score": recovery_high_hrv.score_0_to_100
            }),
        );

        let low_motion_stress = bull_stress_v0(&StressInput {
            motion_intensity_0_to_1: 0.0,
            ..base_stress_input()
        })
        .output
        .expect("valid stress input");
        let high_motion_stress = bull_stress_v0(&StressInput {
            motion_intensity_0_to_1: 1.0,
            ..base_stress_input()
        })
        .output
        .expect("valid stress input");
        group.check(
            case_index,
            "stress_motion_context_reduces_hr_contribution",
            high_motion_stress.motion_adjusted_hr_score
                <= low_motion_stress.motion_adjusted_hr_score,
            "higher motion context should not increase the motion-adjusted HR stress contribution",
            json!({
                "low_motion_hr_score": low_motion_stress.motion_adjusted_hr_score,
                "high_motion_hr_score": high_motion_stress.motion_adjusted_hr_score
            }),
        );

        let hrv = bull_hrv_v0(&HrvInput {
            start_time: "2026-05-28T00:00:00Z".to_string(),
            end_time: "2026-05-28T00:01:00Z".to_string(),
            rr_intervals_ms: vec![800.0, 800.0, 800.0, 800.0],
            input_ids: Vec::new(),
        })
        .output
        .expect("valid HRV input");
        group.check(
            case_index,
            "constant_rr_intervals_have_zero_variability",
            close_to(hrv.rmssd_ms, 0.0)
                && close_to(hrv.sdnn_ms, 0.0)
                && close_to(hrv.pnn50_fraction, 0.0),
            "constant RR intervals must have zero RMSSD, zero SDNN, and zero pNN50",
            json!({
                "rmssd_ms": hrv.rmssd_ms,
                "sdnn_ms": hrv.sdnn_ms,
                "pnn50_fraction": hrv.pnn50_fraction
            }),
        );
    }

    group.finish()
}

fn check_hrv_bounds(group: &mut GroupBuilder, rng: &mut DeterministicRng, case_index: usize) {
    let interval_count = 2 + rng.usize(80);
    let mut intervals = Vec::with_capacity(interval_count);
    let mut expected_valid_count = 0usize;
    for _ in 0..interval_count {
        let value = match rng.usize(10) {
            0 => 250.0,
            1 => 2200.0,
            _ => {
                expected_valid_count += 1;
                rng.f64(300.0, 2000.0)
            }
        };
        intervals.push(value);
    }
    if expected_valid_count < 2 {
        intervals.push(800.0);
        intervals.push(810.0);
        expected_valid_count += 2;
    }
    let result = bull_hrv_v0(&HrvInput {
        start_time: "2026-05-28T00:00:00Z".to_string(),
        end_time: "2026-05-28T00:05:00Z".to_string(),
        rr_intervals_ms: intervals,
        input_ids: Vec::new(),
    });
    let Some(output) = result.output else {
        group.fail(
            case_index,
            "hrv_valid_generated_input_produces_output",
            "generated HRV input with at least two valid intervals produced no output",
            json!({"errors": result.errors, "expected_valid_count": expected_valid_count}),
        );
        return;
    };
    group.check(
        case_index,
        "hrv_counts_are_consistent",
        output.valid_interval_count == expected_valid_count
            && output.interval_count == output.valid_interval_count + output.invalid_interval_count,
        "HRV output counts must match valid/invalid generated intervals",
        json!({
            "output": output,
            "expected_valid_count": expected_valid_count
        }),
    );
    group.check(
        case_index,
        "hrv_outputs_are_finite_and_bounded",
        output.mean_nn_ms.is_finite()
            && output.rmssd_ms.is_finite()
            && output.sdnn_ms.is_finite()
            && output.pnn50_fraction.is_finite()
            && output.rmssd_ms >= 0.0
            && output.sdnn_ms >= 0.0
            && (0.0..=1.0).contains(&output.pnn50_fraction),
        "HRV metrics must be finite, non-negative, and pNN50 must stay in [0,1]",
        json!({"output": output}),
    );
}

fn check_sleep_bounds(group: &mut GroupBuilder, rng: &mut DeterministicRng, case_index: usize) {
    let time_in_bed = rng.f64(240.0, 660.0);
    let sleep_duration = rng.f64(1.0, time_in_bed);
    let result = bull_sleep_v0(&SleepInput {
        start_time: "2026-05-28T00:00:00Z".to_string(),
        end_time: "2026-05-28T08:00:00Z".to_string(),
        sleep_duration_minutes: sleep_duration,
        sleep_need_minutes: rng.f64(300.0, 600.0),
        time_in_bed_minutes: time_in_bed,
        midpoint_deviation_minutes: rng.f64(0.0, 240.0),
        disturbance_count: rng.usize(30) as u32,
        input_ids: Vec::new(),
        ..Default::default()
    });
    let Some(output) = result.output else {
        group.fail(
            case_index,
            "sleep_valid_generated_input_produces_output",
            "generated sleep input produced no output",
            json!({"errors": result.errors}),
        );
        return;
    };
    group.check(
        case_index,
        "sleep_outputs_are_finite_and_bounded",
        output.score_0_to_100.is_finite()
            && (0.0..=100.0).contains(&output.score_0_to_100)
            && output.efficiency_fraction.is_finite()
            && (0.0..=1.0).contains(&output.efficiency_fraction)
            && output.sleep_debt_minutes.is_finite()
            && output.sleep_debt_minutes >= 0.0,
        "sleep output must stay in bounded score, efficiency, and debt ranges",
        json!({"output": output}),
    );
}

fn check_sleep_v1_bounds(group: &mut GroupBuilder, rng: &mut DeterministicRng, case_index: usize) {
    let time_in_bed = rng.f64(240.0, 660.0);
    let sleep_duration = rng.f64(time_in_bed * 0.45, time_in_bed * 0.95);
    let stage_awake = (time_in_bed - sleep_duration).max(0.0);
    let sleep_latency_minutes = rng.f64(0.0, stage_awake.min(90.0) * 0.50);
    let wake_after_sleep_onset_minutes = rng.f64(
        0.0,
        (stage_awake - sleep_latency_minutes).max(0.0).min(180.0),
    );
    let deep = sleep_duration * rng.f64(0.05, 0.25);
    let rem = sleep_duration * rng.f64(0.10, 0.30);
    let core = (sleep_duration - deep - rem).max(0.0);
    let start_unix_ms = 1_779_926_400_000_i64;
    let end_unix_ms = start_unix_ms + (time_in_bed * 60_000.0).round() as i64;
    let result = bull_sleep_v1(&SleepV1Input {
        sleep: SleepInput {
            start_time: format!("unix_ms:{start_unix_ms}"),
            end_time: format!("unix_ms:{end_unix_ms}"),
            sleep_duration_minutes: sleep_duration,
            sleep_need_minutes: rng.f64(300.0, 600.0),
            time_in_bed_minutes: time_in_bed,
            midpoint_deviation_minutes: rng.f64(0.0, 240.0),
            disturbance_count: rng.usize(30) as u32,
            sleep_latency_minutes,
            wake_after_sleep_onset_minutes,
            wake_episode_count: rng.usize(12) as u32,
            stage_minutes: BTreeMap::from([
                ("awake".to_string(), stage_awake),
                ("core".to_string(), core),
                ("deep".to_string(), deep),
                ("rem".to_string(), rem),
            ]),
            heart_rate_dip_percent: Some(rng.f64(0.0, 24.0)),
            input_ids: Vec::new(),
        },
        model_status: SleepModelStatusInput {
            sleep_permission_granted: true,
            trusted_bull_sleep_nights: rng.usize(12) as u32,
            imported_platform_sleep_nights: rng.usize(20) as u32,
            motion_coverage_fraction: Some(rng.f64(0.70, 1.0)),
            heart_rate_coverage_fraction: Some(rng.f64(0.50, 1.0)),
            ..Default::default()
        },
        rolling_sleep_debt_minutes: rng.f64(0.0, 600.0),
        bedtime_deviation_minutes: rng.f64(0.0, 180.0),
        wake_time_deviation_minutes: rng.f64(0.0, 180.0),
        sleep_hr_average_bpm: Some(rng.f64(45.0, 85.0)),
        sleep_hr_min_bpm: Some(rng.f64(38.0, 75.0)),
        sleep_hr_trend_bpm_per_hour: Some(rng.f64(-4.0, 4.0)),
        naps_minutes: rng.f64(0.0, 120.0),
        prior_day_strain: Some(rng.f64(0.0, 21.0)),
        data_coverage_fraction: Some(rng.f64(0.50, 1.0)),
        ..Default::default()
    });
    let Some(output) = result.output else {
        group.fail(
            case_index,
            "sleep_v1_valid_generated_input_produces_output",
            "generated Sleep V1 input produced no output",
            json!({"errors": result.errors}),
        );
        return;
    };
    group.check(
        case_index,
        "sleep_v1_outputs_are_finite_and_bounded",
        output.score_0_to_100.is_finite()
            && (0.0..=100.0).contains(&output.score_0_to_100)
            && output.sleep_efficiency_fraction.is_finite()
            && (0.0..=1.0).contains(&output.sleep_efficiency_fraction)
            && output.sleep_debt_minutes.is_finite()
            && output.sleep_debt_minutes >= 0.0
            && output.rolling_sleep_debt_minutes.is_finite()
            && output.rolling_sleep_debt_minutes >= 0.0
            && output.confidence_0_to_1.is_finite()
            && (0.0..=1.0).contains(&output.confidence_0_to_1)
            && output.components.iter().all(|component| {
                component.score_0_to_100.is_finite()
                    && (0.0..=100.0).contains(&component.score_0_to_100)
                    && component.weight.is_finite()
                    && component.weight >= 0.0
                    && component.contribution.is_finite()
                    && component.contribution >= 0.0
            }),
        "Sleep V1 output, confidence, and component scores must stay finite and bounded",
        json!({"output": output}),
    );
}

fn check_strain_bounds(group: &mut GroupBuilder, rng: &mut DeterministicRng, case_index: usize) {
    let duration = rng.f64(20.0, 180.0);
    let mut remaining = duration;
    let mut zones = Vec::new();
    for zone_index in 0..5 {
        let value = if zone_index == 4 {
            remaining
        } else {
            rng.f64(0.0, remaining)
        };
        zones.push(value);
        remaining -= value;
    }
    let resting_hr = rng.f64(45.0, 75.0);
    let max_hr = rng.f64(165.0, 210.0);
    let result = bull_strain_v0(&StrainInput {
        start_time: "2026-05-28T12:00:00Z".to_string(),
        end_time: "2026-05-28T13:00:00Z".to_string(),
        duration_minutes: duration,
        resting_hr_bpm: resting_hr,
        average_hr_bpm: rng.f64(resting_hr, max_hr),
        max_hr_bpm: max_hr,
        hr_zone_minutes: zones,
        input_ids: Vec::new(),
        ..Default::default()
    });
    let Some(output) = result.output else {
        group.fail(
            case_index,
            "strain_valid_generated_input_produces_output",
            "generated strain input produced no output",
            json!({"errors": result.errors}),
        );
        return;
    };
    group.check(
        case_index,
        "strain_outputs_are_finite_and_bounded",
        output.score_0_to_21.is_finite()
            && (0.0..=21.0).contains(&output.score_0_to_21)
            && output.zone_load.is_finite()
            && output.zone_load >= 0.0
            && output.average_hr_reserve_fraction.is_finite()
            && (0.0..=1.0).contains(&output.average_hr_reserve_fraction),
        "strain output must stay in bounded score and HR reserve ranges",
        json!({"output": output}),
    );
}

fn check_recovery_bounds(group: &mut GroupBuilder, rng: &mut DeterministicRng, case_index: usize) {
    let result = bull_recovery_v0(&RecoveryInput {
        start_time: "2026-05-28T06:00:00Z".to_string(),
        end_time: "2026-05-28T06:05:00Z".to_string(),
        hrv_rmssd_ms: rng.f64(5.0, 140.0),
        hrv_baseline_rmssd_ms: rng.f64(10.0, 120.0),
        resting_hr_bpm: rng.f64(40.0, 95.0),
        resting_hr_baseline_bpm: rng.f64(40.0, 95.0),
        respiratory_rate_rpm: rng.f64(10.0, 24.0),
        respiratory_rate_baseline_rpm: rng.f64(10.0, 24.0),
        skin_temp_delta_c: rng.f64(-3.0, 3.0),
        sleep_score_0_to_100: rng.f64(0.0, 100.0),
        prior_strain_0_to_21: rng.f64(0.0, 21.0),
        input_ids: Vec::new(),
    });
    let Some(output) = result.output else {
        group.fail(
            case_index,
            "recovery_valid_generated_input_produces_output",
            "generated recovery input produced no output",
            json!({"errors": result.errors}),
        );
        return;
    };
    group.check(
        case_index,
        "recovery_outputs_are_finite_and_bounded",
        output.score_0_to_100.is_finite() && (0.0..=100.0).contains(&output.score_0_to_100),
        "recovery score must stay finite and bounded in [0,100]",
        json!({"output": output}),
    );
}

fn check_stress_bounds(group: &mut GroupBuilder, rng: &mut DeterministicRng, case_index: usize) {
    let resting_hr = rng.f64(45.0, 80.0);
    let result = bull_stress_v0(&StressInput {
        start_time: "2026-05-28T12:00:00Z".to_string(),
        end_time: "2026-05-28T12:05:00Z".to_string(),
        heart_rate_bpm: rng.f64(resting_hr, 190.0),
        resting_hr_bpm: resting_hr,
        hrv_rmssd_ms: rng.f64(0.0, 120.0),
        hrv_baseline_rmssd_ms: rng.f64(10.0, 120.0),
        motion_intensity_0_to_1: rng.f64(-0.5, 1.5),
        input_ids: Vec::new(),
    });
    let Some(output) = result.output else {
        group.fail(
            case_index,
            "stress_valid_generated_input_produces_output",
            "generated stress input produced no output",
            json!({"errors": result.errors}),
        );
        return;
    };
    group.check(
        case_index,
        "stress_outputs_are_finite_and_bounded",
        output.score_0_to_100.is_finite()
            && (0.0..=100.0).contains(&output.score_0_to_100)
            && output.heart_rate_elevation_score.is_finite()
            && (0.0..=100.0).contains(&output.heart_rate_elevation_score)
            && output.hrv_suppression_score.is_finite()
            && (0.0..=100.0).contains(&output.hrv_suppression_score)
            && output.motion_adjusted_hr_score.is_finite()
            && (0.0..=100.0).contains(&output.motion_adjusted_hr_score)
            && output.motion_adjusted_hr_score <= output.heart_rate_elevation_score + 0.000001,
        "stress output and sub-scores must stay bounded, and motion adjustment cannot exceed HR elevation",
        json!({"output": output}),
    );
}

fn random_payload(rng: &mut DeterministicRng) -> Vec<u8> {
    let packet_type = match rng.usize(9) {
        0 => 35,
        1 => 36,
        2 => 40,
        3 => 43,
        4 => 47,
        5 => 48,
        6 => 51,
        7 => 52,
        _ => rng.u8(),
    };
    let len = rng.usize(180);
    let mut payload = Vec::with_capacity(len.max(1));
    payload.push(packet_type);
    payload.extend(rng.bytes(len.saturating_sub(1)));
    payload
}

fn non_frame_start_byte(rng: &mut DeterministicRng) -> u8 {
    let mut byte = rng.u8();
    if byte == 0xaa {
        byte = 0xab;
    }
    byte
}

fn base_sleep_input() -> SleepInput {
    SleepInput {
        start_time: "2026-05-28T00:00:00Z".to_string(),
        end_time: "2026-05-28T08:00:00Z".to_string(),
        sleep_duration_minutes: 300.0,
        sleep_need_minutes: 480.0,
        time_in_bed_minutes: 480.0,
        midpoint_deviation_minutes: 30.0,
        disturbance_count: 3,
        input_ids: Vec::new(),
        ..Default::default()
    }
}

fn base_sleep_v1_status_input() -> SleepModelStatusInput {
    SleepModelStatusInput {
        sleep_permission_granted: true,
        trusted_bull_sleep_nights: 7,
        imported_platform_sleep_nights: 7,
        motion_coverage_fraction: Some(0.90),
        heart_rate_coverage_fraction: Some(0.85),
        ..Default::default()
    }
}

fn base_strain_input() -> StrainInput {
    StrainInput {
        start_time: "2026-05-28T12:00:00Z".to_string(),
        end_time: "2026-05-28T13:00:00Z".to_string(),
        duration_minutes: 60.0,
        resting_hr_bpm: 60.0,
        average_hr_bpm: 120.0,
        max_hr_bpm: 180.0,
        hr_zone_minutes: vec![60.0, 0.0, 0.0, 0.0, 0.0],
        input_ids: Vec::new(),
        ..Default::default()
    }
}

fn base_recovery_input() -> RecoveryInput {
    RecoveryInput {
        start_time: "2026-05-28T06:00:00Z".to_string(),
        end_time: "2026-05-28T06:05:00Z".to_string(),
        hrv_rmssd_ms: 50.0,
        hrv_baseline_rmssd_ms: 50.0,
        resting_hr_bpm: 58.0,
        resting_hr_baseline_bpm: 58.0,
        respiratory_rate_rpm: 14.0,
        respiratory_rate_baseline_rpm: 14.0,
        skin_temp_delta_c: 0.0,
        sleep_score_0_to_100: 80.0,
        prior_strain_0_to_21: 8.0,
        input_ids: Vec::new(),
    }
}

fn base_stress_input() -> StressInput {
    StressInput {
        start_time: "2026-05-28T12:00:00Z".to_string(),
        end_time: "2026-05-28T12:05:00Z".to_string(),
        heart_rate_bpm: 110.0,
        resting_hr_bpm: 60.0,
        hrv_rmssd_ms: 35.0,
        hrv_baseline_rmssd_ms: 50.0,
        motion_intensity_0_to_1: 0.0,
        input_ids: Vec::new(),
    }
}

fn close_to(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() <= 0.000001
}

struct GroupBuilder {
    name: String,
    cases: usize,
    checks: usize,
    failures: Vec<PropertyFailure>,
}

impl GroupBuilder {
    fn new(name: &str, cases: usize) -> Self {
        Self {
            name: name.to_string(),
            cases,
            checks: 0,
            failures: Vec::new(),
        }
    }

    fn count_check(&mut self) {
        self.checks += 1;
    }

    fn check(
        &mut self,
        case_index: usize,
        property: &str,
        passed: bool,
        message: &str,
        context: Value,
    ) {
        self.checks += 1;
        if !passed {
            self.fail(case_index, property, message, context);
        }
    }

    fn fail(
        &mut self,
        case_index: usize,
        property: &str,
        message: impl Into<String>,
        context: Value,
    ) {
        self.failures.push(PropertyFailure {
            case_index,
            property: property.to_string(),
            message: message.into(),
            context,
        });
    }

    fn finish(self) -> PropertyGroupReport {
        let next_actions = property_group_next_actions(&self.name, &self.failures);
        PropertyGroupReport {
            name: self.name,
            pass: self.failures.is_empty(),
            cases: self.cases,
            checks: self.checks,
            failures: self.failures,
            next_actions,
        }
    }
}

struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    fn u8(&mut self) -> u8 {
        (self.next_u64() >> 56) as u8
    }

    fn usize(&mut self, upper_exclusive: usize) -> usize {
        if upper_exclusive == 0 {
            return 0;
        }
        (self.next_u64() as usize) % upper_exclusive
    }

    fn f64(&mut self, min: f64, max: f64) -> f64 {
        if max <= min {
            return min;
        }
        let unit = (self.next_u64() >> 11) as f64 / ((1u64 << 53) as f64);
        min + (max - min) * unit
    }

    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.u8()).collect()
    }
}
