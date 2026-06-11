use bull_core::{
    BullError,
    bridge::{BRIDGE_REQUEST_SCHEMA, BridgeRequest, handle_bridge_request},
    report::write_json_report,
    tool_args::{args, flag, path_value, value},
};
use serde_json::json;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

fn run() -> bull_core::BullResult<()> {
    let args = args();
    let database_path = path_value(&args, "--database")?
        .ok_or_else(|| BullError::message("--database is required"))?;
    let output = path_value(&args, "--output")?;
    let start = value(&args, "--start")?.unwrap_or_else(|| "0000".to_string());
    let end = value(&args, "--end")?.unwrap_or_else(|| "9999".to_string());
    let timezone = value(&args, "--timezone")?;
    let min_owned_captures = optional_usize(&args, "--min-owned-captures")?;

    let mut request_args = json!({
        "database_path": database_path.display().to_string(),
        "start": start,
        "end": end,
        "require_owned_captures": flag(&args, "--require-owned-captures"),
        "require_scores_ready": flag(&args, "--require-scores-ready")
    });
    if let Some(timezone) = timezone {
        request_args["timezone"] = json!(timezone);
    }
    if let Some(min_owned_captures) = min_owned_captures {
        request_args["min_owned_captures"] = json!(min_owned_captures);
    }

    let response = handle_bridge_request(BridgeRequest {
        schema: BRIDGE_REQUEST_SCHEMA.to_string(),
        request_id: "capture-arrival-plan-cli".to_string(),
        method: "capture.arrival_plan".to_string(),
        args: request_args,
    });
    if !response.ok {
        let message = response
            .error
            .map(|error| format!("{}: {}", error.code, error.message))
            .unwrap_or_else(|| "capture arrival plan failed".to_string());
        return Err(BullError::message(message));
    }
    let report = response
        .result
        .ok_or_else(|| BullError::message("capture arrival plan missing result"))?;
    let pass = report
        .get("pass")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    write_json_report(&report, output.as_deref())?;
    if pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

fn optional_usize(args: &[String], name: &str) -> bull_core::BullResult<Option<usize>> {
    Ok(value(args, name)?
        .map(|raw| {
            raw.parse::<usize>()
                .map_err(|source| BullError::message(format!("invalid {name}: {source}")))
        })
        .transpose()?)
}
