use bull_core::{
    fixtures::{build_fixture_index, load_fixture_index, run_parser_fixtures},
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
    let index_path = path_value(&args, "--index")?;
    let output = path_value(&args, "--output")?;

    let index = match index_path {
        Some(path) => load_fixture_index(&path)?,
        None => build_fixture_index(&fixtures)?,
    };
    let report = run_parser_fixtures(&fixtures, &index);
    write_json_report(&report, output.as_deref())?;
    if report.pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
}
