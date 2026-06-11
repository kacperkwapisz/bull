use std::fs;

use bull_core::{
    BullError,
    health_sync::{HealthSyncDryRunInput, run_health_sync_dry_run},
    report::write_json_report,
    tool_args::{args, default_path, path_value},
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
        "fixtures/synthetic/health_sync_dry_run_healthkit.json",
    )?;
    let output = path_value(&args, "--output")?;
    let input_raw =
        fs::read_to_string(&input_path).map_err(|source| BullError::io(&input_path, source))?;
    let input: HealthSyncDryRunInput =
        serde_json::from_str(&input_raw).map_err(|source| BullError::json(&input_path, source))?;
    let report = run_health_sync_dry_run(&input);

    write_json_report(&report, output.as_deref())?;
    if report.pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
}
