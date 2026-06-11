use std::{fs, path::Path};

use bull_core::{
    BullError,
    algorithm_compare::AlgorithmComparisonReport,
    historical_sync::HistoricalSyncPhysicalValidationReport,
    report::write_json_report,
    sleep_validation::{
        SleepStageLabelValidationReport, SleepV1ExplanationStabilityReport,
        SleepV1ReleaseGateInput, SleepWindowLabelValidationReport, validate_sleep_v1_release_gates,
    },
    tool_args::{args, path_value, value},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

fn run() -> bull_core::BullResult<()> {
    let args = args();
    let output = path_value(&args, "--output")?;
    let input_output = path_value(&args, "--input-output")?;
    let input = match path_value(&args, "--input")? {
        Some(path) => read_json::<SleepV1ReleaseGateInput>(&path)?,
        None => release_gate_input_from_report_paths(&args)?,
    };
    if let Some(input_output) = input_output.as_deref() {
        write_json_report(&input, Some(input_output))?;
    }
    let report = validate_sleep_v1_release_gates(&input);
    write_json_report(&report, output.as_deref())?;
    if report.pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

fn release_gate_input_from_report_paths(
    args: &[String],
) -> bull_core::BullResult<SleepV1ReleaseGateInput> {
    let defaults = SleepV1ReleaseGateInput::default();
    let physical_historical_sync = optional_json::<HistoricalSyncPhysicalValidationReport>(
        args,
        "--physical-historical-sync-report",
    )?;
    let sleep_window_label_validation =
        optional_json::<SleepWindowLabelValidationReport>(args, "--sleep-window-label-report")?;
    let sleep_stage_label_validation =
        optional_json::<SleepStageLabelValidationReport>(args, "--sleep-stage-label-report")?;
    let explanation_stability =
        optional_json::<SleepV1ExplanationStabilityReport>(args, "--explanation-stability-report")?;
    let benchmark_comparisons = benchmark_reports(args)?;

    Ok(SleepV1ReleaseGateInput {
        physical_historical_sync,
        sleep_window_label_validation,
        sleep_stage_label_validation,
        explanation_stability,
        benchmark_comparisons,
        min_hand_reviewed_window_comparisons: optional_usize(
            args,
            "--min-hand-reviewed-window-comparisons",
        )?
        .unwrap_or(defaults.min_hand_reviewed_window_comparisons),
        min_stage_label_comparisons: optional_usize(args, "--min-stage-label-comparisons")?
            .unwrap_or(defaults.min_stage_label_comparisons),
        min_benchmark_comparisons: optional_usize(args, "--min-benchmark-comparisons")?
            .unwrap_or(defaults.min_benchmark_comparisons),
    })
}

fn optional_json<T: serde::de::DeserializeOwned>(
    args: &[String],
    name: &str,
) -> bull_core::BullResult<Option<T>> {
    path_value(args, name)?.map_or(Ok(None), |path| read_json(&path).map(Some))
}

fn benchmark_reports(args: &[String]) -> bull_core::BullResult<Vec<AlgorithmComparisonReport>> {
    let Some(raw_paths) = value(args, "--benchmark-comparison-reports")? else {
        return Ok(Vec::new());
    };
    raw_paths
        .split(',')
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(|path| read_json(Path::new(path)))
        .collect()
}

fn optional_usize(args: &[String], name: &str) -> bull_core::BullResult<Option<usize>> {
    value(args, name)?.map_or(Ok(None), |raw| {
        raw.parse::<usize>()
            .map(Some)
            .map_err(|error| BullError::message(format!("invalid {name} value {raw}: {error}")))
    })
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> bull_core::BullResult<T> {
    let raw = fs::read_to_string(path).map_err(|source| BullError::io(path, source))?;
    serde_json::from_str(&raw).map_err(|source| BullError::json(path, source))
}
