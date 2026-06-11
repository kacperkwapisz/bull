use bull_core::{
    report::write_json_report,
    sleep_validation::{
        SleepV1EvidenceFolderOptions, validate_sleep_v1_evidence_folder_with_options,
    },
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
    let Some(evidence_dir) = path_value(&args, "--evidence-dir")? else {
        eprintln!("provide --evidence-dir <sleep-v1-validation-folder>");
        std::process::exit(2);
    };
    let output = path_value(&args, "--output")?;
    let report = validate_sleep_v1_evidence_folder_with_options(
        &evidence_dir,
        SleepV1EvidenceFolderOptions {
            expected_evidence_manifest_sha256: value(&args, "--expected-manifest-sha256")?,
        },
    )?;
    write_json_report(&report, output.as_deref())?;
    if report.pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
}
