use std::path::Path;

use bull_core::{
    BullError,
    report::write_json_report,
    sleep_validation::{
        SLEEP_WINDOW_LABEL_VALIDATION_INPUT_SCHEMA, SleepWindowLabelValidationEvidenceInput,
        SleepWindowLabelValidationOptions, run_sleep_window_label_validation_for_store,
    },
    store::BullStore,
    tool_args::{args, flag, path_value, value},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

fn run() -> bull_core::BullResult<()> {
    let args = args();
    let Some(database_path) = value(&args, "--db")? else {
        return Err(BullError::message("missing --db <bull.sqlite>"));
    };
    let Some(start) = value(&args, "--start")? else {
        return Err(BullError::message("missing --start <RFC3339 UTC>"));
    };
    let Some(end) = value(&args, "--end")? else {
        return Err(BullError::message("missing --end <RFC3339 UTC>"));
    };
    let output = path_value(&args, "--output")?;
    let input_output = path_value(&args, "--input-output")?;
    let defaults = SleepWindowLabelValidationOptions::default();
    let options = SleepWindowLabelValidationOptions {
        min_owned_captures_per_summary: optional_usize(&args, "--min-owned-captures")?
            .unwrap_or(defaults.min_owned_captures_per_summary),
        require_trusted_evidence: flag(&args, "--require-trusted-evidence"),
        sleep_need_minutes: optional_f64(&args, "--sleep-need-minutes")?
            .unwrap_or(defaults.sleep_need_minutes),
        low_motion_threshold_0_to_1: optional_f64(&args, "--low-motion-threshold")?
            .unwrap_or(defaults.low_motion_threshold_0_to_1),
        disturbance_motion_threshold_0_to_1: optional_f64(&args, "--disturbance-motion-threshold")?
            .unwrap_or(defaults.disturbance_motion_threshold_0_to_1),
        target_midpoint_minutes_since_midnight: optional_f64(&args, "--target-midpoint-minutes")?
            .unwrap_or(defaults.target_midpoint_minutes_since_midnight),
        start_tolerance_minutes: optional_f64(&args, "--start-tolerance-minutes")?
            .unwrap_or(defaults.start_tolerance_minutes),
        end_tolerance_minutes: optional_f64(&args, "--end-tolerance-minutes")?
            .unwrap_or(defaults.end_tolerance_minutes),
        duration_tolerance_minutes: optional_f64(&args, "--duration-tolerance-minutes")?
            .unwrap_or(defaults.duration_tolerance_minutes),
        min_label_confidence: optional_f64(&args, "--min-label-confidence")?
            .unwrap_or(defaults.min_label_confidence),
    };

    if let Some(input_output) = input_output.as_deref() {
        let input = SleepWindowLabelValidationEvidenceInput {
            schema: SLEEP_WINDOW_LABEL_VALIDATION_INPUT_SCHEMA.to_string(),
            database_path: database_path.clone(),
            start: start.clone(),
            end: end.clone(),
            options: options.clone(),
        };
        write_json_report(&input, Some(input_output))?;
    }

    let store = BullStore::open(Path::new(&database_path))?;
    let report =
        run_sleep_window_label_validation_for_store(&store, &database_path, &start, &end, options)?;
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
