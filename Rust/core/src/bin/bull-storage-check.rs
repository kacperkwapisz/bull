use bull_core::{
    capture_import::ensure_database_parent,
    report::write_json_report,
    storage_check::{StorageCheckOptions, check_storage_database},
    tool_args::{args, default_path, flag, path_value},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

fn run() -> bull_core::BullResult<()> {
    let args = args();
    let db = default_path(&args, "--db", "bull.sqlite")?;
    let output = path_value(&args, "--output")?;
    let run_self_test = flag(&args, "--self-test");

    ensure_database_parent(&db)?;
    let report = check_storage_database(StorageCheckOptions {
        database_path: &db,
        run_self_test,
    })?;
    write_json_report(&report, output.as_deref())?;
    if report.pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
}
