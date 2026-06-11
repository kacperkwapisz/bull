use bull_core::{
    fixtures::build_fixture_index,
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
    let fixtures = default_path(&args, "--fixtures", "fixtures")?;
    let output = path_value(&args, "--output")?;
    let report = build_fixture_index(&fixtures)?;
    write_json_report(&report, output.as_deref())?;
    if report.pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
}
