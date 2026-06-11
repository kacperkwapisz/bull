use std::{fs, path::Path};

use bull_core::{
    BullError,
    report::write_json_report,
    tool_args::{args, default_path, path_value},
    ui_coverage::{UiCoverageAuditInput, run_ui_coverage_audit},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

fn run() -> bull_core::BullResult<()> {
    let args = args();
    let input_path = default_path(&args, "--input", "../apk-ui-inventory/coverage-map.json")?;
    let output = path_value(&args, "--output")?;
    let input_raw =
        fs::read_to_string(&input_path).map_err(|source| BullError::io(&input_path, source))?;
    let input: UiCoverageAuditInput =
        serde_json::from_str(&input_raw).map_err(|source| BullError::json(&input_path, source))?;
    let base_dir = input_path.parent().unwrap_or_else(|| Path::new("."));
    let report = run_ui_coverage_audit(&input, base_dir)?;

    write_json_report(&report, output.as_deref())?;
    if report.pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
}
