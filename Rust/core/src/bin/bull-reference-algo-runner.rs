use std::{fs, path::Path, process::Command};

use bull_core::{
    BullError,
    metrics::{AlgorithmRunResult, HrvInput, SleepInput, StrainInput, StressInput},
    reference::{
        REFERENCE_HRV_PROVIDER, REFERENCE_SLEEP_ACTIGRAPHY_PROVIDER,
        REFERENCE_STRAIN_EDWARDS_PROVIDER, REFERENCE_STRESS_HRV_HR_PROVIDER,
        hrv_reference_run_record, reference_algorithm_definitions, reference_hrv_time_domain,
        reference_sleep_actigraphy_summary, reference_strain_edwards_load,
        reference_stress_hrv_hr_proxy, sleep_reference_run_record, strain_reference_run_record,
        stress_reference_run_record,
    },
    report::write_json_report,
    store::{AlgorithmDefinitionRecord, AlgorithmRunRecord, BullStore},
    tool_args::{args, default_path, path_value, value},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use sha2::{Digest, Sha256};

const EXTERNAL_REFERENCE_OUTPUT_SCHEMA: &str = "bull.external-reference-output.v1";

#[derive(Debug, Serialize)]
struct ReferenceAlgoReport {
    schema: String,
    generated_by: String,
    family: String,
    provider: String,
    provider_kind: String,
    algorithm_id: String,
    algorithm_version: String,
    pass: bool,
    input_valid: bool,
    provider_valid: bool,
    output_ready: bool,
    errors_clear: bool,
    provenance_ready: bool,
    storage_ready: bool,
    reference_ready: bool,
    stored_run_id: Option<String>,
    output: Option<serde_json::Value>,
    quality_flags: Vec<String>,
    errors: Vec<String>,
    next_actions: Vec<ReferenceNextAction>,
    provenance: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct ReferenceNextAction {
    scope: String,
    reason: String,
    action: String,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

fn run() -> bull_core::BullResult<()> {
    let args = args();
    let family = value(&args, "--family")?.unwrap_or_else(|| "hrv".to_string());
    let provider_arg = value(&args, "--provider")?;
    let external_command = path_value(&args, "--external-command")?;
    if external_command.is_some() && provider_arg.as_deref().unwrap_or("").trim().is_empty() {
        return Err(BullError::message(
            "--external-command requires an explicit --provider such as external.neurokit2.hrv",
        ));
    }
    let provider = provider_arg.unwrap_or_else(|| default_provider_for_family(&family));
    let external_args = values(&args, "--external-arg")?;
    let input_path = reference_input_path(&args, &family)?;
    let output = path_value(&args, "--output")?;
    let db = path_value(&args, "--db")?;
    let storage_requested = db.is_some();
    let run = run_reference_algorithm(
        &family,
        &provider,
        &input_path,
        external_command.as_deref(),
        &external_args,
    )?;
    let mut stored_run_id = None;

    if let Some(db_path) = db {
        let run_id = value(&args, "--run-id")?
            .unwrap_or_else(|| format!("{}:{}:{}", run.algorithm_id, run.start_time, run.end_time));
        let store = BullStore::open(&db_path)?;
        if let Some(definition) = &run.definition {
            store.upsert_algorithm_definition(definition)?;
        } else {
            for definition in reference_algorithm_definitions() {
                store.upsert_algorithm_definition(&definition)?;
            }
        }
        let record = run.record(&run_id);
        store.insert_algorithm_run(&record)?;
        stored_run_id = Some(run_id);
    }

    let input_valid = true;
    let provider_valid = true;
    let output_ready = run.output.is_some();
    let errors_clear = run.errors.is_empty();
    let provenance_ready = non_empty_object(&run.provenance);
    let storage_ready = !storage_requested || stored_run_id.is_some();
    let reference_ready = input_valid
        && provider_valid
        && output_ready
        && errors_clear
        && provenance_ready
        && storage_ready;
    let next_actions =
        reference_next_actions(&run.errors, output_ready, provenance_ready, storage_ready);
    let report = ReferenceAlgoReport {
        schema: "bull.reference-algo-report.v1".to_string(),
        generated_by: "bull-reference-algo-runner".to_string(),
        family: run.family,
        provider,
        provider_kind: run.provider_kind,
        algorithm_id: run.algorithm_id,
        algorithm_version: run.algorithm_version,
        pass: reference_ready,
        input_valid,
        provider_valid,
        output_ready,
        errors_clear,
        provenance_ready,
        storage_ready,
        reference_ready,
        stored_run_id,
        output: run.output,
        quality_flags: run.quality_flags,
        errors: run.errors,
        next_actions,
        provenance: run.provenance,
    };

    write_json_report(&report, output.as_deref())?;
    if report.pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

#[derive(Debug)]
struct ReferenceRun {
    family: String,
    algorithm_id: String,
    algorithm_version: String,
    start_time: String,
    end_time: String,
    output: Option<serde_json::Value>,
    quality_flags: Vec<String>,
    errors: Vec<String>,
    provenance: serde_json::Value,
    output_json: String,
    quality_flags_json: String,
    provenance_json: String,
    provider_kind: String,
    definition: Option<AlgorithmDefinitionRecord>,
}

impl ReferenceRun {
    fn record(&self, run_id: &str) -> AlgorithmRunRecord {
        AlgorithmRunRecord {
            run_id: run_id.to_string(),
            algorithm_id: self.algorithm_id.clone(),
            version: self.algorithm_version.clone(),
            start_time: self.start_time.clone(),
            end_time: self.end_time.clone(),
            output_json: self.output_json.clone(),
            quality_flags_json: self.quality_flags_json.clone(),
            provenance_json: self.provenance_json.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ExternalReferenceOutput {
    schema: String,
    family: String,
    provider: String,
    provider_version: String,
    source: String,
    license: String,
    algorithm_id: String,
    algorithm_version: String,
    start_time: String,
    end_time: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    input_schema: Option<String>,
    #[serde(default)]
    output_schema: Option<String>,
    #[serde(default)]
    output: Option<serde_json::Value>,
    #[serde(default = "empty_object")]
    output_units: serde_json::Value,
    #[serde(default = "empty_object")]
    parameters: serde_json::Value,
    #[serde(default = "empty_object")]
    input_requirements: serde_json::Value,
    #[serde(default = "empty_array")]
    quality_gates: serde_json::Value,
    #[serde(default)]
    quality_flags: Vec<String>,
    #[serde(default)]
    errors: Vec<String>,
    #[serde(default = "empty_object")]
    provenance: serde_json::Value,
}

fn run_reference_algorithm(
    family: &str,
    provider: &str,
    input_path: &std::path::Path,
    external_command: Option<&Path>,
    external_args: &[String],
) -> bull_core::BullResult<ReferenceRun> {
    if let Some(executable) = external_command {
        return run_external_reference_algorithm(
            family,
            provider,
            input_path,
            executable,
            external_args,
        );
    }

    match (family, provider) {
        ("hrv", REFERENCE_HRV_PROVIDER) => run_typed(
            input_path,
            |input: HrvInput| reference_hrv_time_domain(&input),
            hrv_reference_run_record,
        ),
        ("sleep", REFERENCE_SLEEP_ACTIGRAPHY_PROVIDER) => run_typed(
            input_path,
            |input: SleepInput| reference_sleep_actigraphy_summary(&input),
            sleep_reference_run_record,
        ),
        ("strain", REFERENCE_STRAIN_EDWARDS_PROVIDER) => run_typed(
            input_path,
            |input: StrainInput| reference_strain_edwards_load(&input),
            strain_reference_run_record,
        ),
        ("stress", REFERENCE_STRESS_HRV_HR_PROVIDER) => run_typed(
            input_path,
            |input: StressInput| reference_stress_hrv_hr_proxy(&input),
            stress_reference_run_record,
        ),
        ("hrv", other) => Err(BullError::message(format!(
            "unsupported HRV provider {other}; current provider is {REFERENCE_HRV_PROVIDER}"
        ))),
        ("sleep", other) => Err(BullError::message(format!(
            "unsupported sleep provider {other}; current provider is {REFERENCE_SLEEP_ACTIGRAPHY_PROVIDER}"
        ))),
        ("strain", other) => Err(BullError::message(format!(
            "unsupported strain provider {other}; current provider is {REFERENCE_STRAIN_EDWARDS_PROVIDER}"
        ))),
        ("stress", other) => Err(BullError::message(format!(
            "unsupported stress provider {other}; current provider is {REFERENCE_STRESS_HRV_HR_PROVIDER}"
        ))),
        (other, _) => Err(BullError::message(format!(
            "unsupported family {other}; current reference runner supports hrv|sleep|strain|stress"
        ))),
    }
}

fn run_typed<I, O>(
    input_path: &std::path::Path,
    run: impl FnOnce(I) -> AlgorithmRunResult<O>,
    record: impl FnOnce(&str, &AlgorithmRunResult<O>) -> bull_core::BullResult<AlgorithmRunRecord>,
) -> bull_core::BullResult<ReferenceRun>
where
    I: DeserializeOwned,
    O: Serialize,
{
    let input_raw =
        fs::read_to_string(input_path).map_err(|source| BullError::io(input_path, source))?;
    let input: I =
        serde_json::from_str(&input_raw).map_err(|source| BullError::json(input_path, source))?;
    let result = run(input);
    let record = record("__reference_pending_run_id__", &result)?;
    Ok(ReferenceRun {
        family: result.family,
        algorithm_id: result.algorithm_id,
        algorithm_version: result.algorithm_version,
        start_time: result.start_time,
        end_time: result.end_time,
        output: result
            .output
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|error| BullError::message(format!("cannot serialize output: {error}")))?,
        quality_flags: result.quality_flags,
        errors: result.errors,
        provenance: result.provenance,
        output_json: record.output_json,
        quality_flags_json: record.quality_flags_json,
        provenance_json: record.provenance_json,
        provider_kind: "internal_reference".to_string(),
        definition: None,
    })
}

fn run_external_reference_algorithm(
    family: &str,
    provider: &str,
    input_path: &Path,
    executable: &Path,
    external_args: &[String],
) -> bull_core::BullResult<ReferenceRun> {
    let input_bytes = fs::read(input_path).map_err(|source| BullError::io(input_path, source))?;
    let input_sha256 = sha256_hex(&input_bytes);
    let mut command = Command::new(executable);
    command
        .args(external_args)
        .arg("--input")
        .arg(input_path)
        .arg("--family")
        .arg(family)
        .arg("--provider")
        .arg(provider)
        .arg("--output-format")
        .arg(EXTERNAL_REFERENCE_OUTPUT_SCHEMA)
        .env("BULL_REFERENCE_INPUT_PATH", input_path)
        .env("BULL_REFERENCE_INPUT_SHA256", &input_sha256)
        .env("BULL_REFERENCE_FAMILY", family)
        .env("BULL_REFERENCE_PROVIDER", provider)
        .env(
            "BULL_REFERENCE_OUTPUT_SCHEMA",
            EXTERNAL_REFERENCE_OUTPUT_SCHEMA,
        );

    let process_output = command.output().map_err(|error| {
        BullError::message(format!(
            "cannot run external reference provider {}: {error}",
            executable.display()
        ))
    })?;
    let stdout_sha256 = sha256_hex(&process_output.stdout);
    let stderr_sha256 = sha256_hex(&process_output.stderr);
    if !process_output.status.success() {
        return Err(BullError::message(format!(
            "external reference provider {} exited with status {:?}: {}",
            executable.display(),
            process_output.status.code(),
            truncate_for_error(&String::from_utf8_lossy(&process_output.stderr))
        )));
    }

    let external: ExternalReferenceOutput = serde_json::from_slice(&process_output.stdout)
        .map_err(|error| {
            BullError::message(format!(
                "external reference provider {} did not emit valid JSON: {error}",
                executable.display()
            ))
        })?;
    validate_external_reference_output(&external, family, provider)?;

    let command_args = external_args
        .iter()
        .cloned()
        .chain([
            "--input".to_string(),
            input_path.display().to_string(),
            "--family".to_string(),
            family.to_string(),
            "--provider".to_string(),
            provider.to_string(),
            "--output-format".to_string(),
            EXTERNAL_REFERENCE_OUTPUT_SCHEMA.to_string(),
        ])
        .collect::<Vec<_>>();
    let provenance = json!({
        "provider_kind": "external_reference",
        "external_provider": external.provider,
        "external_provider_version": external.provider_version,
        "external_source": external.source,
        "external_license": external.license,
        "external_report_schema": external.schema,
        "external_report_provenance": external.provenance,
        "external_command": {
            "program": executable.display().to_string(),
            "args": command_args,
            "input_path": input_path.display().to_string(),
            "input_sha256": input_sha256,
            "stdout_sha256": stdout_sha256,
            "stderr_sha256": stderr_sha256,
            "status_code": process_output.status.code()
        },
        "output_units": external.output_units,
        "parameters": external.parameters
    });
    let output_json = serde_json::to_string(&external.output).map_err(|error| {
        BullError::message(format!(
            "cannot serialize external reference output: {error}"
        ))
    })?;
    let quality_flags_json = serde_json::to_string(&external.quality_flags).map_err(|error| {
        BullError::message(format!(
            "cannot serialize external reference quality flags: {error}"
        ))
    })?;
    let provenance_json = serde_json::to_string(&json!({
        "provenance": provenance,
        "errors": external.errors
    }))
    .map_err(|error| {
        BullError::message(format!(
            "cannot serialize external reference provenance: {error}"
        ))
    })?;
    let definition = external_reference_definition(&external)?;

    Ok(ReferenceRun {
        family: external.family,
        algorithm_id: external.algorithm_id,
        algorithm_version: external.algorithm_version,
        start_time: external.start_time,
        end_time: external.end_time,
        output: external.output,
        quality_flags: external.quality_flags,
        errors: external.errors,
        provenance,
        output_json,
        quality_flags_json,
        provenance_json,
        provider_kind: "external_reference".to_string(),
        definition: Some(definition),
    })
}

fn validate_external_reference_output(
    report: &ExternalReferenceOutput,
    family: &str,
    provider: &str,
) -> bull_core::BullResult<()> {
    if report.schema != EXTERNAL_REFERENCE_OUTPUT_SCHEMA {
        return Err(BullError::message(format!(
            "unexpected external reference schema {}; expected {EXTERNAL_REFERENCE_OUTPUT_SCHEMA}",
            report.schema
        )));
    }
    if report.family != family {
        return Err(BullError::message(format!(
            "external reference family {} does not match requested family {family}",
            report.family
        )));
    }
    if report.provider != provider {
        return Err(BullError::message(format!(
            "external reference provider {} does not match requested provider {provider}",
            report.provider
        )));
    }
    require_non_empty("provider_version", &report.provider_version)?;
    require_non_empty("source", &report.source)?;
    require_non_empty("license", &report.license)?;
    require_non_empty("algorithm_id", &report.algorithm_id)?;
    require_non_empty("algorithm_version", &report.algorithm_version)?;
    require_non_empty("start_time", &report.start_time)?;
    require_non_empty("end_time", &report.end_time)?;
    require_object("output_units", &report.output_units)?;
    require_object("parameters", &report.parameters)?;
    require_object("input_requirements", &report.input_requirements)?;
    require_array("quality_gates", &report.quality_gates)?;
    require_object("provenance", &report.provenance)?;
    if report
        .provenance
        .as_object()
        .is_some_and(|object| object.is_empty())
    {
        return Err(BullError::message(
            "external reference provenance must be a non-empty object",
        ));
    }
    if let Some(output) = &report.output {
        require_object("output", output)?;
        if output.as_object().is_some_and(|object| !object.is_empty())
            && report
                .output_units
                .as_object()
                .is_some_and(|object| object.is_empty())
        {
            return Err(BullError::message(
                "external reference output_units must record units for benchmark outputs",
            ));
        }
    } else if report.errors.is_empty() {
        return Err(BullError::message(
            "external reference output is required when errors is empty",
        ));
    }
    Ok(())
}

fn reference_next_actions(
    errors: &[String],
    output_ready: bool,
    provenance_ready: bool,
    storage_ready: bool,
) -> Vec<ReferenceNextAction> {
    let mut actions = Vec::new();
    if !output_ready {
        actions.push(ReferenceNextAction {
            scope: "output".to_string(),
            reason: "reference_output_missing".to_string(),
            action: "Provide enough valid input data for the benchmark reference algorithm to emit an output.".to_string(),
        });
    }
    if !provenance_ready {
        actions.push(ReferenceNextAction {
            scope: "provenance".to_string(),
            reason: "reference_provenance_missing".to_string(),
            action:
                "Record the reference provider, version, input, parameters, and source evidence before using the output for comparison."
                    .to_string(),
        });
    }
    if !storage_ready {
        actions.push(ReferenceNextAction {
            scope: "storage".to_string(),
            reason: "reference_storage_missing".to_string(),
            action: "Rerun with a writable Bull SQLite database or omit --db for a report-only benchmark.".to_string(),
        });
    }
    for error in errors {
        actions.push(ReferenceNextAction {
            scope: "reference".to_string(),
            reason: reference_error_reason(error),
            action: reference_error_action(error),
        });
    }
    actions.sort();
    actions.dedup();
    actions
}

fn reference_error_reason(error: &str) -> String {
    if error.contains("not_enough") || error.contains("insufficient") {
        "insufficient_reference_input".to_string()
    } else if error.contains("must") || error.contains("invalid") {
        "invalid_reference_input".to_string()
    } else {
        "reference_algorithm_error".to_string()
    }
}

fn reference_error_action(error: &str) -> String {
    if error.contains("not_enough") || error.contains("insufficient") {
        "Collect or import a longer/high-quality owned-data window before using this reference output for benchmarking.".to_string()
    } else if error.contains("must") || error.contains("invalid") {
        "Fix the reference input shape, units, and time window before rerunning the benchmark reference algorithm.".to_string()
    } else {
        "Inspect the reference algorithm error and rerun after correcting the benchmark input or adapter.".to_string()
    }
}

fn non_empty_object(value: &serde_json::Value) -> bool {
    value.as_object().is_some_and(|object| !object.is_empty())
}

fn external_reference_definition(
    report: &ExternalReferenceOutput,
) -> bull_core::BullResult<AlgorithmDefinitionRecord> {
    Ok(AlgorithmDefinitionRecord {
        algorithm_id: report.algorithm_id.clone(),
        version: report.algorithm_version.clone(),
        metric_family: report.family.clone(),
        display_name: report
            .display_name
            .clone()
            .unwrap_or_else(|| format!("External Reference {}", report.provider)),
        implementation: "external-reference".to_string(),
        license: report.license.clone(),
        input_schema: report
            .input_schema
            .clone()
            .unwrap_or_else(|| format!("bull.{}-input.v1", report.family)),
        output_schema: report
            .output_schema
            .clone()
            .unwrap_or_else(|| EXTERNAL_REFERENCE_OUTPUT_SCHEMA.to_string()),
        input_requirements_json: serde_json::to_string(&report.input_requirements).map_err(
            |error| {
                BullError::message(format!(
                    "cannot serialize external reference input requirements: {error}"
                ))
            },
        )?,
        params_json: serde_json::to_string(&json!({
            "provider": report.provider,
            "provider_version": report.provider_version,
            "source": report.source,
            "parameters": report.parameters,
            "output_units": report.output_units,
            "external_report_schema": report.schema
        }))
        .map_err(|error| {
            BullError::message(format!(
                "cannot serialize external reference params: {error}"
            ))
        })?,
        quality_gates_json: serde_json::to_string(&report.quality_gates).map_err(|error| {
            BullError::message(format!(
                "cannot serialize external reference quality gates: {error}"
            ))
        })?,
        status: "benchmark-only".to_string(),
    })
}

fn default_provider_for_family(family: &str) -> String {
    match family {
        "hrv" => REFERENCE_HRV_PROVIDER.to_string(),
        "sleep" => REFERENCE_SLEEP_ACTIGRAPHY_PROVIDER.to_string(),
        "strain" => REFERENCE_STRAIN_EDWARDS_PROVIDER.to_string(),
        "stress" => REFERENCE_STRESS_HRV_HR_PROVIDER.to_string(),
        _ => String::new(),
    }
}

fn values(args: &[String], name: &str) -> bull_core::BullResult<Vec<String>> {
    let mut values = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == name {
            let Some(value) = iter.next() else {
                return Err(BullError::message(format!("missing value for {name}")));
            };
            values.push(value.clone());
        }
    }
    Ok(values)
}

fn empty_object() -> serde_json::Value {
    json!({})
}

fn empty_array() -> serde_json::Value {
    json!([])
}

fn require_non_empty(field: &str, value: &str) -> bull_core::BullResult<()> {
    if value.trim().is_empty() {
        return Err(BullError::message(format!(
            "external reference {field} must be non-empty"
        )));
    }
    Ok(())
}

fn require_object(field: &str, value: &serde_json::Value) -> bull_core::BullResult<()> {
    if !value.is_object() {
        return Err(BullError::message(format!(
            "external reference {field} must be a JSON object"
        )));
    }
    Ok(())
}

fn require_array(field: &str, value: &serde_json::Value) -> bull_core::BullResult<()> {
    if !value.is_array() {
        return Err(BullError::message(format!(
            "external reference {field} must be a JSON array"
        )));
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn truncate_for_error(value: &str) -> String {
    const LIMIT: usize = 500;
    let trimmed = value.trim();
    let mut chars = trimmed.chars();
    let shortened = chars.by_ref().take(LIMIT).collect::<String>();
    if chars.next().is_some() {
        format!("{shortened}...")
    } else {
        shortened
    }
}

fn reference_input_path(
    args: &[String],
    family: &str,
) -> bull_core::BullResult<std::path::PathBuf> {
    match family {
        "hrv" => default_path(
            args,
            "--input",
            "fixtures/synthetic/hrv_bull_v0_hand_derived.json",
        ),
        "sleep" => default_path(
            args,
            "--input",
            "fixtures/synthetic/sleep_bull_v0_hand_derived.json",
        ),
        "strain" => default_path(
            args,
            "--input",
            "fixtures/synthetic/strain_bull_v0_hand_derived.json",
        ),
        "stress" => default_path(
            args,
            "--input",
            "fixtures/synthetic/stress_bull_v0_hand_derived.json",
        ),
        other => Err(BullError::message(format!(
            "unsupported family {other}; current reference runner supports hrv|sleep|strain|stress"
        ))),
    }
}
