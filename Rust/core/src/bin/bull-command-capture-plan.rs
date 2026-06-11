use std::fs;

use bull_core::{
    commands::{
        CommandEmulatorLogEvidenceOptions, command_capture_plan_from_results,
        command_evidence_from_emulator_log, command_evidence_with_local_frame_matches,
        load_command_evidence, load_command_local_frame_candidates, validate_commands,
    },
    report::write_json_report,
    tool_args::{args, flag, path_value, value},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

fn run() -> bull_core::BullResult<()> {
    let args = args();
    let output = path_value(&args, "--output")?;
    let mut evidence = match path_value(&args, "--evidence")? {
        Some(path) => load_command_evidence(&path)?,
        None => Vec::new(),
    };

    if let Some(path) = path_value(&args, "--emulator-log")? {
        let emulator_report = command_evidence_from_emulator_log(
            &path,
            &CommandEmulatorLogEvidenceOptions {
                write_type: value(&args, "--write-type")?
                    .unwrap_or_else(|| "with_response".to_string()),
                visible_user_intent: flag(&args, "--visible-user-intent"),
                triggering_ui_action: value(&args, "--triggering-ui-action")?,
                visible_confirmation: flag(&args, "--visible-confirmation"),
                rollback_plan: flag(&args, "--rollback-plan"),
                explicit_approval: flag(&args, "--explicit-approval"),
                mirror_local_frame: flag(&args, "--emulator-mirror-local-frame"),
                capture_app: value(&args, "--capture-app")?
                    .unwrap_or_else(|| "whoop_official".to_string()),
                capture_kind: value(&args, "--capture-kind")?
                    .unwrap_or_else(|| "official_app_to_macos_emulator".to_string()),
                owner: value(&args, "--owner")?.unwrap_or_else(|| "user".to_string()),
            },
        )?;
        if let Some(evidence_output) = path_value(&args, "--emulator-evidence-output")? {
            if let Some(parent) = evidence_output.parent() {
                fs::create_dir_all(parent)
                    .map_err(|source| bull_core::BullError::io(parent, source))?;
            }
            let json = serde_json::to_string_pretty(&emulator_report).map_err(|source| {
                bull_core::BullError::message(format!(
                    "cannot serialize emulator evidence: {source}"
                ))
            })?;
            fs::write(&evidence_output, json.as_bytes())
                .map_err(|source| bull_core::BullError::io(&evidence_output, source))?;
        }
        evidence.extend(emulator_report.evidence);
    }

    if let Some(path) = path_value(&args, "--local-frame-candidates")? {
        let candidates = load_command_local_frame_candidates(&path)?;
        let match_report = command_evidence_with_local_frame_matches(&evidence, &candidates);
        if let Some(match_output) = path_value(&args, "--local-frame-match-output")? {
            if let Some(parent) = match_output.parent() {
                fs::create_dir_all(parent)
                    .map_err(|source| bull_core::BullError::io(parent, source))?;
            }
            let json = serde_json::to_string_pretty(&match_report).map_err(|source| {
                bull_core::BullError::message(format!(
                    "cannot serialize local frame match report: {source}"
                ))
            })?;
            fs::write(&match_output, json.as_bytes())
                .map_err(|source| bull_core::BullError::io(&match_output, source))?;
        }
        evidence = match_report.evidence;
    }

    let validation = validate_commands(&evidence);
    let requested_commands = value(&args, "--commands")?
        .map(|raw| {
            raw.split(',')
                .map(|command| command.trim().to_string())
                .filter(|command| !command.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let plan = command_capture_plan_from_results(&validation.commands, &requested_commands);
    let pass = validation.evidence_valid && plan.pass;

    write_json_report(&plan, output.as_deref())?;
    if pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
}
