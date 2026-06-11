use std::fs;

use bull_core::{
    BullError,
    historical_sync::{
        HistoricalSyncGeneration, HistoricalSyncPhysicalValidationInput,
        historical_sync_physical_evidence_template, validate_historical_sync_physical_evidence,
    },
    report::write_json_report,
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
    let output = path_value(&args, "--output")?;

    if flag(&args, "--template") {
        let generation = generation_arg(&args)?;
        let capture_session_id = value(&args, "--capture-session-id")?.unwrap_or_default();
        let template = historical_sync_physical_evidence_template(generation, capture_session_id);
        return write_json_report(&template, output.as_deref());
    }

    let Some(evidence_path) = path_value(&args, "--evidence")? else {
        return Err(BullError::message(
            "provide --template or --evidence <historical-sync-physical-validation.json>",
        ));
    };
    let json = fs::read_to_string(&evidence_path)
        .map_err(|source| BullError::io(&evidence_path, source))?;
    let input = serde_json::from_str::<HistoricalSyncPhysicalValidationInput>(&json)
        .map_err(|error| BullError::message(format!("invalid physical evidence JSON: {error}")))?;
    let report = validate_historical_sync_physical_evidence(&input);
    write_json_report(&report, output.as_deref())?;
    if report.pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

fn generation_arg(args: &[String]) -> bull_core::BullResult<HistoricalSyncGeneration> {
    match value(args, "--generation")?
        .unwrap_or_else(|| "gen5".to_string())
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "gen4" | "4" => Ok(HistoricalSyncGeneration::Gen4),
        "gen5" | "5" => Ok(HistoricalSyncGeneration::Gen5),
        value => Err(BullError::message(format!(
            "unsupported historical sync generation {value}; use gen4 or gen5"
        ))),
    }
}
