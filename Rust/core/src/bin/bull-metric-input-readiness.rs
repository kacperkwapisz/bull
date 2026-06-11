use std::fs;

use bull_core::{
    BullError,
    capture_correlation::{
        CaptureCorrelationOptions, CaptureCorrelationReport,
        DEFAULT_MIN_OWNED_CAPTURES_PER_SUMMARY, run_capture_correlation_for_store,
    },
    metric_readiness::{MetricInputReadinessOptions, run_metric_input_readiness},
    report::write_json_report,
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
    let output = path_value(&args, "--output")?;
    let correlation = if let Some(path) = path_value(&args, "--capture-correlation")? {
        let raw = fs::read_to_string(&path).map_err(|source| BullError::io(&path, source))?;
        serde_json::from_str::<CaptureCorrelationReport>(&raw)
            .map_err(|source| BullError::json(&path, source))?
    } else {
        let database_path = path_value(&args, "--database")?
            .ok_or_else(|| BullError::message("--database is required"))?;
        let store = BullStore::open(&database_path)?;
        run_capture_correlation_for_store(
            &store,
            &database_path.display().to_string(),
            &value(&args, "--start")?.unwrap_or_else(|| "0000".to_string()),
            &value(&args, "--end")?.unwrap_or_else(|| "9999".to_string()),
            CaptureCorrelationOptions {
                min_owned_captures_per_summary: optional_usize(&args, "--min-owned-captures")?
                    .unwrap_or(DEFAULT_MIN_OWNED_CAPTURES_PER_SUMMARY),
                require_owned_captures: flag(&args, "--require-owned-captures"),
            },
        )?
    };
    let report = run_metric_input_readiness(
        &correlation,
        MetricInputReadinessOptions {
            require_scores_ready: flag(&args, "--require-scores-ready"),
        },
    );
    let pass = report.pass;

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
