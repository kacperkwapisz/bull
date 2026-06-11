use std::{collections::BTreeSet, fs, path::PathBuf, time::Instant};

use bull_core::{
    BullError,
    algorithm_compare::{
        AlgorithmComparisonReport, compare_hrv_bull_to_reference,
        compare_sleep_bull_to_external_reference_report, compare_sleep_bull_to_reference,
        compare_sleep_v1_bull_to_reference, compare_strain_bull_to_reference,
        compare_stress_bull_to_reference,
    },
    metrics::{
        AlgorithmRunResult, BULL_HRV_V0_ID, BULL_SLEEP_V1_ID, HrvInput, RecoveryInput,
        SleepInput, SleepV1Input, StrainInput, StressInput, algorithm_run_record,
        built_in_algorithm_definitions, bull_hrv_v0, bull_recovery_v0, bull_sleep_v0,
        bull_sleep_v1, bull_strain_v0, bull_stress_v0,
    },
    report::write_json_report,
    store::{AlgorithmRunRecord, BullStore},
    tool_args::{args, flag, path_value, value},
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::json;

#[derive(Debug, Serialize)]
struct AlgoBenchmarkReport {
    schema: String,
    generated_by: String,
    family: String,
    algorithm_id: String,
    algorithm_version: String,
    runtime_ms: f64,
    data_coverage: BenchmarkDataCoverage,
    score_field: Option<BenchmarkScoreField>,
    label_comparison: Option<BenchmarkLabelComparison>,
    pass: bool,
    stored_run_id: Option<String>,
    output: Option<serde_json::Value>,
    quality_flags: Vec<String>,
    errors: Vec<String>,
    next_actions: Vec<BenchmarkNextAction>,
}

#[derive(Debug, Clone, Serialize)]
struct BenchmarkDataCoverage {
    input_path: String,
    input_bytes: u64,
    input_ids_count: usize,
    start_time: String,
    end_time: String,
    output_present: bool,
    quality_flag_count: usize,
    error_count: usize,
}

#[derive(Debug, Clone, Serialize)]
struct BenchmarkScoreField {
    field: String,
    unit: String,
    value: f64,
}

#[derive(Debug, Clone)]
struct BenchmarkLabelArgs {
    value: f64,
    unit: String,
    source: String,
    provenance: serde_json::Value,
    max_absolute_error: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
struct BenchmarkLabelComparison {
    label_value: f64,
    label_unit: String,
    label_source: String,
    label_provenance: serde_json::Value,
    official_labels_are_labels: bool,
    prediction_field: Option<String>,
    prediction_unit: Option<String>,
    prediction_value: Option<f64>,
    signed_error: Option<f64>,
    absolute_error: Option<f64>,
    max_absolute_error: Option<f64>,
    error_within_threshold: Option<bool>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct BenchmarkNextAction {
    scope: String,
    reason: String,
    action: String,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

fn run() -> bull_core::BullResult<()> {
    let args = args();
    let family = value(&args, "--family")?.unwrap_or_else(|| "hrv".to_string());
    let output = path_value(&args, "--output")?;
    let label_args = benchmark_label_args(&args)?;

    if flag(&args, "--compare-reference") {
        let algorithm_id = value(&args, "--algorithm")?;
        let input_path = input_path_for_family(&args, &family)?;
        let reference_report = path_value(&args, "--reference-report")?;
        let started = Instant::now();
        let report = run_reference_comparison(
            &family,
            algorithm_id.as_deref(),
            &input_path,
            reference_report.as_ref(),
        )?;
        let mut report = report;
        report.runtime_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
        let cli_data_coverage = serde_json::to_value(data_coverage_from_report(
            &input_path,
            &report.start_time,
            &report.end_time,
            report.bull_output.is_some() && report.reference_output.is_some(),
            report.bull_quality_flags.len() + report.reference_quality_flags.len(),
            report.errors.len(),
        ))
        .map_err(|error| {
            BullError::message(format!(
                "cannot serialize comparison data coverage: {error}"
            ))
        })?;
        report.data_coverage = Some(merge_data_coverage(
            report.data_coverage.take(),
            cli_data_coverage,
        ));
        let pass = report.pass;
        write_json_report(&report, output.as_deref())?;
        if pass {
            return Ok(());
        }
        std::process::exit(1);
    }

    let algorithm_id = value(&args, "--algorithm")?
        .or_else(|| default_algorithm_for_family(&family))
        .unwrap_or_else(|| BULL_HRV_V0_ID.to_string());
    let input_path = input_path(&args, &algorithm_id)?;
    let db = path_value(&args, "--db")?;

    let started = Instant::now();
    let run = run_benchmark_algorithm(&algorithm_id, &input_path)?;
    let runtime_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut stored_run_id = None;

    if let Some(db_path) = db {
        let run_id = value(&args, "--run-id")?.unwrap_or_else(|| run.default_run_id());
        let store = BullStore::open(&db_path)?;
        for definition in built_in_algorithm_definitions() {
            store.upsert_algorithm_definition(&definition)?;
        }
        let record = run.record(&run_id);
        store.insert_algorithm_run(&record)?;
        stored_run_id = Some(run_id);
    }

    let (label_comparison, label_errors) = label_args.map_or((None, Vec::new()), |label| {
        let (comparison, errors) = compare_label_to_score(&label, run.score_field.as_ref());
        (Some(comparison), errors)
    });
    let mut errors = run.errors.clone();
    errors.extend(label_errors);
    let next_actions = benchmark_next_actions(&errors);

    let report = AlgoBenchmarkReport {
        schema: "bull.algo-benchmark-report.v1".to_string(),
        generated_by: "bull-algo-benchmark".to_string(),
        family: run.family,
        algorithm_id: run.algorithm_id,
        algorithm_version: run.algorithm_version,
        runtime_ms,
        data_coverage: run.data_coverage,
        score_field: run.score_field,
        label_comparison,
        pass: errors.is_empty(),
        stored_run_id,
        output: run.output,
        quality_flags: run.quality_flags,
        errors,
        next_actions,
    };

    write_json_report(&report, output.as_deref())?;
    if report.pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

fn benchmark_next_actions(errors: &[String]) -> Vec<BenchmarkNextAction> {
    errors
        .iter()
        .map(|error| benchmark_error_action(error))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn benchmark_error_action(error: &str) -> BenchmarkNextAction {
    if error == "label_value_must_be_finite" {
        BenchmarkNextAction {
            scope: "label.value".to_string(),
            reason: "label_value_must_be_finite".to_string(),
            action: "Provide a finite user-owned label value before benchmarking label error."
                .to_string(),
        }
    } else if let Some(source) = error.strip_prefix("unsupported_label_source:") {
        BenchmarkNextAction {
            scope: "label.source".to_string(),
            reason: "unsupported_label_source".to_string(),
            action: format!(
                "Replace label source `{source}` with manual, passive official capture, user export, screenshot import, or synthetic fixture evidence."
            ),
        }
    } else if error == "label_provenance_must_be_non_empty_object" {
        BenchmarkNextAction {
            scope: "label.provenance".to_string(),
            reason: "label_provenance_missing".to_string(),
            action: "Attach non-empty provenance JSON for the user-owned label.".to_string(),
        }
    } else if error == "label_provenance_must_mark_official_labels_as_labels" {
        BenchmarkNextAction {
            scope: "label.provenance.official_labels_are_labels".to_string(),
            reason: "official_label_marker_missing".to_string(),
            action:
                "Set official_labels_are_labels=true so official values are treated only as labels."
                    .to_string(),
        }
    } else if let Some(detail) = error.strip_prefix("label_error_exceeds_threshold:") {
        BenchmarkNextAction {
            scope: "label.threshold".to_string(),
            reason: "label_error_exceeds_threshold".to_string(),
            action: format!(
                "Keep the benchmark blocked, inspect the score formula or label provenance, and only change thresholds with evidence; observed {detail}."
            ),
        }
    } else if error.starts_with("label_unit_mismatch:") {
        BenchmarkNextAction {
            scope: "label.unit".to_string(),
            reason: "label_unit_mismatch".to_string(),
            action: "Use a label unit that matches the selected score field before comparing label error."
                .to_string(),
        }
    } else if error == "label_prediction_missing" {
        BenchmarkNextAction {
            scope: "score_field".to_string(),
            reason: "label_prediction_missing".to_string(),
            action: "Run an algorithm/family that emits the selected score field before comparing to labels."
                .to_string(),
        }
    } else {
        BenchmarkNextAction {
            scope: "algorithm".to_string(),
            reason: "algorithm_run_error".to_string(),
            action: format!(
                "Fix the benchmark input or algorithm requirement `{error}` before trusting this benchmark."
            ),
        }
    }
}

fn run_reference_comparison(
    family: &str,
    algorithm_id: Option<&str>,
    input_path: &PathBuf,
    reference_report_path: Option<&PathBuf>,
) -> bull_core::BullResult<AlgorithmComparisonReport> {
    match family {
        "hrv" => {
            reject_external_reference_report_for_family(family, reference_report_path)?;
            let input = read_typed_input::<HrvInput>(input_path)?;
            compare_hrv_bull_to_reference(&input)
        }
        "sleep" => {
            if algorithm_id == Some(BULL_SLEEP_V1_ID) {
                reject_external_reference_report_for_sleep_v1(reference_report_path)?;
                let input = read_typed_input::<SleepV1Input>(input_path)?;
                compare_sleep_v1_bull_to_reference(&input)
            } else if let Some(path) = reference_report_path {
                let input = read_typed_input::<SleepInput>(input_path)?;
                let reference_report = read_json_value(path)?;
                compare_sleep_bull_to_external_reference_report(&input, &reference_report)
            } else {
                let input = read_typed_input::<SleepInput>(input_path)?;
                compare_sleep_bull_to_reference(&input)
            }
        }
        "strain" => {
            reject_external_reference_report_for_family(family, reference_report_path)?;
            let input = read_typed_input::<StrainInput>(input_path)?;
            compare_strain_bull_to_reference(&input)
        }
        "stress" => {
            reject_external_reference_report_for_family(family, reference_report_path)?;
            let input = read_typed_input::<StressInput>(input_path)?;
            compare_stress_bull_to_reference(&input)
        }
        other => Err(BullError::message(format!(
            "unsupported reference comparison family {other}; use --family hrv|sleep|strain|stress"
        ))),
    }
}

fn reject_external_reference_report_for_sleep_v1(
    reference_report_path: Option<&PathBuf>,
) -> bull_core::BullResult<()> {
    if reference_report_path.is_some() {
        return Err(BullError::message(
            "--reference-report currently compares external sleep reports against bull.sleep.v0; omit it for bull.sleep.v1 reference comparison",
        ));
    }
    Ok(())
}

fn reject_external_reference_report_for_family(
    family: &str,
    reference_report_path: Option<&PathBuf>,
) -> bull_core::BullResult<()> {
    if reference_report_path.is_some() {
        return Err(BullError::message(format!(
            "--reference-report currently supports sleep comparisons; got family {family}"
        )));
    }
    Ok(())
}

#[derive(Debug)]
struct BenchmarkRun {
    family: String,
    algorithm_id: String,
    algorithm_version: String,
    start_time: String,
    end_time: String,
    output: Option<serde_json::Value>,
    quality_flags: Vec<String>,
    errors: Vec<String>,
    output_json: String,
    quality_flags_json: String,
    provenance_json: String,
    data_coverage: BenchmarkDataCoverage,
    score_field: Option<BenchmarkScoreField>,
}

impl BenchmarkRun {
    fn default_run_id(&self) -> String {
        format!(
            "{}:{}:{}",
            self.algorithm_id, self.start_time, self.end_time
        )
    }

    fn record(&self, run_id: &str) -> AlgorithmRunRecord {
        AlgorithmRunRecord {
            run_id: run_id.to_string(),
            algorithm_id: self.algorithm_id.clone(),
            version: self.algorithm_version.clone(),
            start_time: self.start_time.clone(),
            end_time: self.end_time.clone(),
            output_json: self.output_json.clone(),
            quality_flags_json: self.quality_flags_json.clone(),
            provenance_json: self.provenance_json.clone(),
        }
    }
}

fn run_benchmark_algorithm(
    algorithm_id: &str,
    input_path: &PathBuf,
) -> bull_core::BullResult<BenchmarkRun> {
    match algorithm_id {
        "bull.hrv.v0" => run_typed(input_path, |input: HrvInput| bull_hrv_v0(&input)),
        "bull.sleep.v0" => run_typed(input_path, |input: SleepInput| bull_sleep_v0(&input)),
        "bull.sleep.v1" => run_typed(input_path, |input: SleepV1Input| bull_sleep_v1(&input)),
        "bull.strain.v0" => run_typed(input_path, |input: StrainInput| bull_strain_v0(&input)),
        "bull.recovery.v0" => {
            run_typed(input_path, |input: RecoveryInput| bull_recovery_v0(&input))
        }
        "bull.stress.v0" => run_typed(input_path, |input: StressInput| bull_stress_v0(&input)),
        other => Err(BullError::message(format!(
            "unsupported algorithm {other}; use metrics.built_in_definitions or --algorithm bull.hrv.v0|bull.sleep.v0|bull.sleep.v1|bull.strain.v0|bull.recovery.v0|bull.stress.v0"
        ))),
    }
}

fn run_typed<I, O>(
    input_path: &PathBuf,
    run: impl FnOnce(I) -> AlgorithmRunResult<O>,
) -> bull_core::BullResult<BenchmarkRun>
where
    I: DeserializeOwned,
    O: Serialize,
{
    let input_raw =
        fs::read_to_string(input_path).map_err(|source| BullError::io(input_path, source))?;
    let input_value: serde_json::Value =
        serde_json::from_str(&input_raw).map_err(|source| BullError::json(input_path, source))?;
    let input: I = serde_json::from_value(input_value.clone())
        .map_err(|error| BullError::message(format!("cannot parse benchmark input: {error}")))?;
    let result = run(input);
    let record = algorithm_run_record("__benchmark_pending_run_id__", &result)?;
    let output = result
        .output
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|error| BullError::message(format!("cannot serialize output: {error}")))?;
    let score_field = output
        .as_ref()
        .and_then(|value| score_field_for_family(&result.family, value));
    let data_coverage = BenchmarkDataCoverage {
        input_path: input_path.display().to_string(),
        input_bytes: input_raw.len() as u64,
        input_ids_count: input_value
            .get("input_ids")
            .and_then(|value| value.as_array())
            .map(|ids| ids.len())
            .unwrap_or(0),
        start_time: result.start_time.clone(),
        end_time: result.end_time.clone(),
        output_present: output.is_some(),
        quality_flag_count: result.quality_flags.len(),
        error_count: result.errors.len(),
    };
    Ok(BenchmarkRun {
        family: result.family,
        algorithm_id: result.algorithm_id,
        algorithm_version: result.algorithm_version,
        start_time: result.start_time,
        end_time: result.end_time,
        output,
        quality_flags: result.quality_flags,
        errors: result.errors,
        output_json: record.output_json,
        quality_flags_json: record.quality_flags_json,
        provenance_json: record.provenance_json,
        data_coverage,
        score_field,
    })
}

fn benchmark_label_args(args: &[String]) -> bull_core::BullResult<Option<BenchmarkLabelArgs>> {
    let Some(value_raw) = value(args, "--label-value")? else {
        return Ok(None);
    };
    let label_value = value_raw
        .parse::<f64>()
        .map_err(|error| BullError::message(format!("invalid --label-value: {error}")))?;
    let Some(unit) = value(args, "--label-unit")? else {
        return Err(BullError::message(
            "--label-unit is required when --label-value is provided",
        ));
    };
    let Some(source) = value(args, "--label-source")? else {
        return Err(BullError::message(
            "--label-source is required when --label-value is provided",
        ));
    };
    let Some(provenance_raw) = value(args, "--label-provenance-json")? else {
        return Err(BullError::message(
            "--label-provenance-json is required when --label-value is provided",
        ));
    };
    let provenance = serde_json::from_str(&provenance_raw)
        .map_err(|error| BullError::message(format!("invalid label provenance JSON: {error}")))?;
    let max_absolute_error = value(args, "--max-absolute-error")?
        .map(|raw| {
            raw.parse::<f64>().map_err(|error| {
                BullError::message(format!("invalid --max-absolute-error: {error}"))
            })
        })
        .transpose()?;
    Ok(Some(BenchmarkLabelArgs {
        value: label_value,
        unit,
        source,
        provenance,
        max_absolute_error,
    }))
}

fn compare_label_to_score(
    label: &BenchmarkLabelArgs,
    score_field: Option<&BenchmarkScoreField>,
) -> (BenchmarkLabelComparison, Vec<String>) {
    let mut errors = Vec::new();
    if !label.value.is_finite() {
        errors.push("label_value_must_be_finite".to_string());
    }
    if !is_allowed_label_source(&label.source) {
        errors.push(format!("unsupported_label_source:{}", label.source));
    }
    let official_labels_are_labels = label
        .provenance
        .get("official_labels_are_labels")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if !label
        .provenance
        .as_object()
        .map(|object| !object.is_empty())
        .unwrap_or(false)
    {
        errors.push("label_provenance_must_be_non_empty_object".to_string());
    }
    if !official_labels_are_labels {
        errors.push("label_provenance_must_mark_official_labels_as_labels".to_string());
    }

    let mut prediction_field = None;
    let mut prediction_unit = None;
    let mut prediction_value = None;
    let mut signed_error = None;
    let mut absolute_error = None;
    let mut error_within_threshold = None;

    if let Some(score) = score_field {
        prediction_field = Some(score.field.clone());
        prediction_unit = Some(score.unit.clone());
        prediction_value = Some(score.value);
        if score.unit == label.unit {
            let error = score.value - label.value;
            signed_error = Some(error);
            absolute_error = Some(error.abs());
            if let Some(max_error) = label.max_absolute_error {
                let within = error.abs() <= max_error;
                error_within_threshold = Some(within);
                if !within {
                    errors.push(format!(
                        "label_error_exceeds_threshold:{}>{}",
                        error.abs(),
                        max_error
                    ));
                }
            }
        } else {
            errors.push(format!(
                "label_unit_mismatch:prediction={} label={}",
                score.unit, label.unit
            ));
        }
    } else {
        errors.push("label_prediction_missing".to_string());
    }

    (
        BenchmarkLabelComparison {
            label_value: label.value,
            label_unit: label.unit.clone(),
            label_source: label.source.clone(),
            label_provenance: label.provenance.clone(),
            official_labels_are_labels: true,
            prediction_field,
            prediction_unit,
            prediction_value,
            signed_error,
            absolute_error,
            max_absolute_error: label.max_absolute_error,
            error_within_threshold,
        },
        errors,
    )
}

fn score_field_for_family(family: &str, output: &serde_json::Value) -> Option<BenchmarkScoreField> {
    let (field, unit) = match family {
        "hrv" => ("rmssd_ms", "ms"),
        "strain" => ("score_0_to_21", "score_0_to_21"),
        "sleep" | "recovery" | "stress" => ("score_0_to_100", "score_0_to_100"),
        _ => return None,
    };
    output
        .get(field)
        .and_then(|value| value.as_f64())
        .map(|value| BenchmarkScoreField {
            field: format!("output.{field}"),
            unit: unit.to_string(),
            value,
        })
}

fn is_allowed_label_source(source: &str) -> bool {
    matches!(
        source,
        "manual" | "passive_official_capture" | "user_export" | "screenshot_import" | "synthetic"
    )
}

fn data_coverage_from_report(
    input_path: &PathBuf,
    start_time: &str,
    end_time: &str,
    output_present: bool,
    quality_flag_count: usize,
    error_count: usize,
) -> BenchmarkDataCoverage {
    let input_raw = fs::read_to_string(input_path).unwrap_or_default();
    let input_value = serde_json::from_str::<serde_json::Value>(&input_raw).unwrap_or(json!({}));
    BenchmarkDataCoverage {
        input_path: input_path.display().to_string(),
        input_bytes: input_raw.len() as u64,
        input_ids_count: input_value
            .get("input_ids")
            .and_then(|value| value.as_array())
            .map(|ids| ids.len())
            .unwrap_or(0),
        start_time: start_time.to_string(),
        end_time: end_time.to_string(),
        output_present,
        quality_flag_count,
        error_count,
    }
}

fn merge_data_coverage(
    existing: Option<serde_json::Value>,
    cli_data_coverage: serde_json::Value,
) -> serde_json::Value {
    let Some(mut existing) = existing else {
        return cli_data_coverage;
    };
    let Some(existing_object) = existing.as_object_mut() else {
        return cli_data_coverage;
    };
    let Some(cli_object) = cli_data_coverage.as_object() else {
        return existing;
    };
    for (key, value) in cli_object {
        existing_object.insert(key.clone(), value.clone());
    }
    existing
}

fn read_typed_input<I>(input_path: &PathBuf) -> bull_core::BullResult<I>
where
    I: DeserializeOwned,
{
    let input_raw =
        fs::read_to_string(input_path).map_err(|source| BullError::io(input_path, source))?;
    serde_json::from_str(&input_raw).map_err(|source| BullError::json(input_path, source))
}

fn read_json_value(path: &PathBuf) -> bull_core::BullResult<serde_json::Value> {
    let raw = fs::read_to_string(path).map_err(|source| BullError::io(path, source))?;
    serde_json::from_str(&raw).map_err(|source| BullError::json(path, source))
}

fn input_path(args: &[String], algorithm_id: &str) -> bull_core::BullResult<PathBuf> {
    if let Some(path) = path_value(args, "--input")? {
        return Ok(path);
    }
    if algorithm_id == BULL_HRV_V0_ID {
        return Ok(PathBuf::from(
            "fixtures/synthetic/hrv_bull_v0_hand_derived.json",
        ));
    }
    Err(BullError::message(
        "--input is required for non-HRV benchmark algorithms",
    ))
}

fn input_path_for_family(args: &[String], family: &str) -> bull_core::BullResult<PathBuf> {
    if let Some(path) = path_value(args, "--input")? {
        return Ok(path);
    }
    match family {
        "hrv" => Ok(PathBuf::from(
            "fixtures/synthetic/hrv_bull_v0_hand_derived.json",
        )),
        "sleep" => Ok(PathBuf::from(
            "fixtures/synthetic/sleep_bull_v0_hand_derived.json",
        )),
        "strain" => Ok(PathBuf::from(
            "fixtures/synthetic/strain_bull_v0_hand_derived.json",
        )),
        other => Err(BullError::message(format!(
            "--input is required for unsupported comparison family {other}"
        ))),
    }
}

fn default_algorithm_for_family(family: &str) -> Option<String> {
    match family {
        "hrv" => Some("bull.hrv.v0".to_string()),
        "sleep" => Some("bull.sleep.v0".to_string()),
        "strain" => Some("bull.strain.v0".to_string()),
        "recovery" => Some("bull.recovery.v0".to_string()),
        "stress" => Some("bull.stress.v0".to_string()),
        _ => None,
    }
}
