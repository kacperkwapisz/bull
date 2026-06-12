use bull_core::{
    BullError,
    report::write_json_report,
    rr_hr_consistency::{
        RrHrConsistencyOptions, RrHrConsistencyVerdict, run_rr_hr_consistency_report,
    },
    store::BullStore,
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
    let output = path_value(&args, "--output")?;
    let database_path = path_value(&args, "--database")?
        .ok_or_else(|| BullError::message("--database is required"))?;
    let start = value(&args, "--start")?.unwrap_or_else(|| "0000".to_string());
    let end = value(&args, "--end")?.unwrap_or_else(|| "9999".to_string());

    let defaults = RrHrConsistencyOptions::default();
    let options = RrHrConsistencyOptions {
        max_hr_abs_error_bpm: optional_f64(&args, "--max-hr-abs-error-bpm")?
            .unwrap_or(defaults.max_hr_abs_error_bpm),
        max_hr_fractional_error: optional_f64(&args, "--max-hr-fractional-error")?
            .unwrap_or(defaults.max_hr_fractional_error),
        min_rr_intervals_per_frame: optional_usize(&args, "--min-rr-intervals-per-frame")?
            .unwrap_or(defaults.min_rr_intervals_per_frame),
        min_eligible_frames: optional_usize(&args, "--min-eligible-frames")?
            .unwrap_or(defaults.min_eligible_frames),
        consistency_pass_ratio: optional_f64(&args, "--consistency-pass-ratio")?
            .unwrap_or(defaults.consistency_pass_ratio),
    };

    let store = BullStore::open(&database_path)?;
    let decoded_rows = store.decoded_frames_between(&start, &end)?;
    let report = run_rr_hr_consistency_report(&decoded_rows, options)?;
    let verified = report.verdict == RrHrConsistencyVerdict::Verified;

    write_json_report(&report, output.as_deref())?;
    if verified {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

fn optional_usize(args: &[String], name: &str) -> bull_core::BullResult<Option<usize>> {
    value(args, name)?
        .map(|raw| {
            raw.parse::<usize>()
                .map_err(|source| BullError::message(format!("invalid {name}: {source}")))
        })
        .transpose()
}

fn optional_f64(args: &[String], name: &str) -> bull_core::BullResult<Option<f64>> {
    value(args, name)?
        .map(|raw| {
            raw.parse::<f64>()
                .map_err(|source| BullError::message(format!("invalid {name}: {source}")))
        })
        .transpose()
}
