use std::fs;

use bull_core::{
    BullError,
    debug_ws::{DebugWsContractInput, validate_debug_ws_contract},
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
        "fixtures/synthetic/debug_ws_contract_valid.json",
    )?;
    let output = path_value(&args, "--output")?;
    let input_raw =
        fs::read_to_string(&input_path).map_err(|source| BullError::io(&input_path, source))?;
    let input: DebugWsContractInput =
        serde_json::from_str(&input_raw).map_err(|source| BullError::json(&input_path, source))?;
    let report = validate_debug_ws_contract(&input);

    write_json_report(&report, output.as_deref())?;
    if report.pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
}
