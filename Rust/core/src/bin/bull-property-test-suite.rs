use bull_core::{
    property_tests::{
        DEFAULT_CASES_PER_GROUP, DEFAULT_PROPERTY_SEED, PropertySuiteOptions, run_property_suite,
    },
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
    let seed = optional_u64(&args, "--seed")?.unwrap_or(DEFAULT_PROPERTY_SEED);
    let cases_per_group = optional_usize(&args, "--cases")?.unwrap_or(DEFAULT_CASES_PER_GROUP);
    let output = path_value(&args, "--output")?;

    let report = run_property_suite(PropertySuiteOptions {
        seed,
        cases_per_group,
    })?;
    write_json_report(&report, output.as_deref())?;
    if report.pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
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

fn optional_usize(args: &[String], name: &str) -> bull_core::BullResult<Option<usize>> {
    Ok(value(args, name)?
        .map(|raw| {
            raw.parse::<usize>().map_err(|source| {
                bull_core::BullError::message(format!("invalid {name}: {source}"))
            })
        })
        .transpose()?)
}
