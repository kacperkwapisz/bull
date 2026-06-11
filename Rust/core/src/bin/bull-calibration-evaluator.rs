use std::fs;

use bull_core::{
    BullError,
    calibration::{
        CalibrationDataset, CalibrationOptions, calibration_run_record, evaluate_linear_calibration,
    },
    report::write_json_report,
    store::{AlgorithmDefinitionRecord, BullStore},
    tool_args::{args, default_path, path_value, value},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

fn run() -> bull_core::BullResult<()> {
    let args = args();
    let input_path = default_path(
        &args,
        "--input",
        "fixtures/synthetic/recovery_calibration_linear.json",
    )?;
    let output = path_value(&args, "--output")?;
    let db = path_value(&args, "--db")?;
    let run_id = value(&args, "--run-id")?.unwrap_or_else(|| "calibration-demo-run".to_string());
    let split_at =
        value(&args, "--split-at")?.unwrap_or_else(|| "2026-05-04T00:00:00Z".to_string());
    let metric_family = value(&args, "--metric-family")?.unwrap_or_else(|| "recovery".to_string());
    let algorithm_id =
        value(&args, "--algorithm-id")?.unwrap_or_else(|| "bull.recovery.v0".to_string());
    let algorithm_version =
        value(&args, "--algorithm-version")?.unwrap_or_else(|| "0.1.0".to_string());

    let input_raw =
        fs::read_to_string(&input_path).map_err(|source| BullError::io(&input_path, source))?;
    let dataset: CalibrationDataset =
        serde_json::from_str(&input_raw).map_err(|source| BullError::json(&input_path, source))?;
    let report = evaluate_linear_calibration(
        &dataset,
        &CalibrationOptions {
            metric_family,
            algorithm_id,
            algorithm_version,
            split_at,
            min_train_rows: 2,
            min_holdout_rows: 1,
        },
    );

    if report.pass
        && let Some(db_path) = db.as_deref()
    {
        let store = BullStore::open(db_path)?;
        store.upsert_algorithm_definition(&AlgorithmDefinitionRecord {
            algorithm_id: report.algorithm_id.clone(),
            version: report.algorithm_version.clone(),
            metric_family: report.metric_family.clone(),
            display_name: format!("{} {}", report.algorithm_id, report.algorithm_version),
            implementation: "calibrated-local".to_string(),
            license: "UNLICENSED".to_string(),
            input_schema: "bull.calibration-input.v1".to_string(),
            output_schema: "bull.calibration-output.v1".to_string(),
            input_requirements_json: "{}".to_string(),
            params_json: "{}".to_string(),
            quality_gates_json: "[]".to_string(),
            status: "calibration-target".to_string(),
        })?;
        let record = calibration_run_record(&run_id, &report)?;
        store.insert_calibration_run(&record)?;
    }

    write_json_report(&report, output.as_deref())?;
    if report.pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
}
