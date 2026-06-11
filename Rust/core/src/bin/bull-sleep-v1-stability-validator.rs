use std::{fs, path::Path};

use bull_core::{
    BullError,
    metrics::SleepV1Input,
    report::write_json_report,
    sleep_validation::{
        SleepV1ExplanationStabilityOptions, validate_sleep_v1_explanation_and_stability,
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
    let Some(input_path) = path_value(&args, "--input")? else {
        return Err(BullError::message(
            "missing --input <bull.sleep-v1-input.json>",
        ));
    };
    let output = path_value(&args, "--output")?;
    let input = read_json::<SleepV1Input>(&input_path)?;
    let defaults = SleepV1ExplanationStabilityOptions::default();
    let report = validate_sleep_v1_explanation_and_stability(
        &input,
        SleepV1ExplanationStabilityOptions {
            max_repeated_run_delta: optional_f64(&args, "--max-repeated-run-delta")?
                .unwrap_or(defaults.max_repeated_run_delta),
            max_small_perturbation_delta: optional_f64(&args, "--max-small-perturbation-delta")?
                .unwrap_or(defaults.max_small_perturbation_delta),
            perturb_sleep_duration_minutes: optional_f64(
                &args,
                "--perturb-sleep-duration-minutes",
            )?
            .unwrap_or(defaults.perturb_sleep_duration_minutes),
            min_v1_component_count: optional_usize(&args, "--min-v1-component-count")?
                .unwrap_or(defaults.min_v1_component_count),
            min_explanation_quality_signal_count: optional_usize(
                &args,
                "--min-explanation-quality-signal-count",
            )?
            .unwrap_or(defaults.min_explanation_quality_signal_count),
        },
    );
    write_json_report(&report, output.as_deref())?;
    if report.pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

fn optional_f64(args: &[String], name: &str) -> bull_core::BullResult<Option<f64>> {
    value(args, name)?.map_or(Ok(None), |raw| {
        raw.parse::<f64>()
            .map(Some)
            .map_err(|error| BullError::message(format!("invalid {name} value {raw}: {error}")))
    })
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
