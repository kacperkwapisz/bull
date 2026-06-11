use bull_core::{
    perf_budget::{DEFAULT_PERF_SCALE, PerfBudgetOptions, PerfBudgets, run_perf_budget},
    report::write_json_report,
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
    let scale = optional_usize(&args, "--scale")?.unwrap_or(DEFAULT_PERF_SCALE);
    let output = path_value(&args, "--output")?;
    let budgets = PerfBudgets {
        parser_max_duration_ms: optional_u64(&args, "--max-parser-ms")?
            .unwrap_or_else(|| PerfBudgets::default().parser_max_duration_ms),
        deframer_max_duration_ms: optional_u64(&args, "--max-deframer-ms")?
            .unwrap_or_else(|| PerfBudgets::default().deframer_max_duration_ms),
        algorithms_max_duration_ms: optional_u64(&args, "--max-algorithms-ms")?
            .unwrap_or_else(|| PerfBudgets::default().algorithms_max_duration_ms),
        export_max_duration_ms: optional_u64(&args, "--max-export-ms")?
            .unwrap_or_else(|| PerfBudgets::default().export_max_duration_ms),
        parser_max_estimated_peak_bytes: optional_mib(&args, "--max-parser-mib")?
            .unwrap_or_else(|| PerfBudgets::default().parser_max_estimated_peak_bytes),
        deframer_max_estimated_peak_bytes: optional_mib(&args, "--max-deframer-mib")?
            .unwrap_or_else(|| PerfBudgets::default().deframer_max_estimated_peak_bytes),
        algorithms_max_estimated_peak_bytes: optional_mib(&args, "--max-algorithms-mib")?
            .unwrap_or_else(|| PerfBudgets::default().algorithms_max_estimated_peak_bytes),
        export_max_estimated_peak_bytes: optional_mib(&args, "--max-export-mib")?
            .unwrap_or_else(|| PerfBudgets::default().export_max_estimated_peak_bytes),
    };

    let report = run_perf_budget(PerfBudgetOptions { scale, budgets })?;
    write_json_report(&report, output.as_deref())?;
    if report.pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

fn optional_usize(args: &[String], name: &str) -> bull_core::BullResult<Option<usize>> {
    Ok(value(args, name)?
        .map(|raw| {
            raw.parse::<usize>().map_err(|source| {
                bull_core::BullError::message(format!("invalid {name}: {source}"))
            })
        })
        .transpose()?)
}

fn optional_u64(args: &[String], name: &str) -> bull_core::BullResult<Option<u64>> {
    Ok(value(args, name)?
        .map(|raw| {
            raw.parse::<u64>().map_err(|source| {
                bull_core::BullError::message(format!("invalid {name}: {source}"))
            })
        })
        .transpose()?)
}

fn optional_mib(args: &[String], name: &str) -> bull_core::BullResult<Option<u64>> {
    Ok(optional_u64(args, name)?.map(|value| value * 1024 * 1024))
}
