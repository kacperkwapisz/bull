use bull_core::{
    export::{RawExportFilters, RawExportOptions, export_raw_timeframe},
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
    let db = default_path(&args, "--db", "bull.sqlite")?;
    let output_dir = default_path(&args, "--output-dir", "exports/latest.bullbundle")?;
    let zip_output_path = path_value(&args, "--zip-output")?;
    let report_output = path_value(&args, "--output")?;
    let start = value(&args, "--start")?.unwrap_or_else(|| "0000-01-01T00:00:00Z".to_string());
    let end = value(&args, "--end")?.unwrap_or_else(|| "9999-12-31T23:59:59Z".to_string());
    let app_version = value(&args, "--app-version")?.unwrap_or_else(|| "bull-app/dev".to_string());
    let core_version = value(&args, "--core-version")?.unwrap_or_else(|| {
        format!(
            "bull-core/{}",
            option_env!("CARGO_PKG_VERSION").unwrap_or("unknown")
        )
    });
    let data_families = value(&args, "--data-families")?
        .map(|families| {
            families
                .split(',')
                .map(|family| family.trim().to_string())
                .filter(|family| !family.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let filters = RawExportFilters {
        include_raw_bytes: !args.iter().any(|arg| arg == "--exclude-raw-bytes"),
        capture_session_ids: csv_values(&args, "--capture-session-ids")?,
        packet_type_names: csv_values(&args, "--packet-type-names")?,
        sensor_source_signals: csv_values(&args, "--sensor-source-signals")?,
        metric_families: csv_values(&args, "--metric-families")?,
        algorithm_ids: csv_values(&args, "--algorithm-ids")?,
        algorithm_versions: csv_values(&args, "--algorithm-versions")?,
    };

    let store = BullStore::open(&db)?;
    let report = export_raw_timeframe(
        &store,
        RawExportOptions {
            output_dir: &output_dir,
            start: &start,
            end: &end,
            app_version: &app_version,
            core_version: &core_version,
            data_families,
            filters,
            sqlite_source_path: Some(&db),
            zip_output_path: zip_output_path.as_deref(),
        },
    )?;
    write_json_report(&report, report_output.as_deref())?;
    if report.pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

fn csv_values(args: &[String], key: &str) -> bull_core::BullResult<Vec<String>> {
    Ok(value(args, key)?
        .map(|values| {
            values
                .split(',')
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default())
}
