use bull_core::{
    BullError,
    capture_sanitize::{CaptureSanitizeOptions, sanitize_capture_path},
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
    let input =
        path_value(&args, "--input")?.ok_or_else(|| BullError::message("--input is required"))?;
    let output_dir = path_value(&args, "--output")?
        .ok_or_else(|| BullError::message("--output is required"))?;
    let report_output = path_value(&args, "--report")?;
    let salt = value(&args, "--salt")?.unwrap_or_else(|| "bull-capture-sanitize-v1".to_string());

    let report = sanitize_capture_path(CaptureSanitizeOptions {
        input_path: &input,
        output_path: &output_dir,
        salt: &salt,
    })?;
    write_json_report(&report, report_output.as_deref())?;
    if report.pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
}
