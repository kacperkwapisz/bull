use bull_core::{
    export::validate_export_bundle,
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
    let bundle = default_path(&args, "--bundle", "exports/latest")?;
    let output = path_value(&args, "--output")?;
    let report = validate_export_bundle(&bundle)?;
    write_json_report(&report, output.as_deref())?;
    if report.pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
}
