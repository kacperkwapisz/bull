use bull_core::{
    capture_import::{CaptureImportOptions, ensure_database_parent, import_fixture_index},
    fixtures::{build_fixture_index, load_fixture_index},
    report::write_json_report,
    store::BullStore,
    tool_args::{args, default_path, path_value, value},
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
    let db = default_path(&args, "--db", "bull.sqlite")?;
    let index_path = path_value(&args, "--index")?;
    let output = path_value(&args, "--output")?;
    let parser_version = value(&args, "--parser-version")?.unwrap_or_else(|| {
        format!(
            "bull-core/{}",
            option_env!("CARGO_PKG_VERSION").unwrap_or("unknown")
        )
    });

    ensure_database_parent(&db)?;
    let store = BullStore::open(&db)?;
    let index = match index_path {
        Some(path) => load_fixture_index(&path)?,
        None => build_fixture_index(&fixtures)?,
    };

    let report = import_fixture_index(
        &store,
        &index,
        CaptureImportOptions {
            fixture_root: &fixtures,
            database_path: &db,
            parser_version: &parser_version,
        },
    );
    write_json_report(&report, output.as_deref())?;
    if report.pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
}
