use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{BullError, BullResult};

pub const CAPTURE_SANITIZE_REPORT_SCHEMA: &str = "bull.capture-sanitize-report.v1";
pub const CAPTURE_SANITIZE_MANIFEST_SCHEMA: &str = "bull.capture-sanitize-manifest.v1";

#[derive(Debug, Clone)]
pub struct CaptureSanitizeOptions<'a> {
    pub input_path: &'a Path,
    pub output_path: &'a Path,
    pub salt: &'a str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureSanitizeReport {
    pub schema: String,
    pub generated_by: String,
    pub input_path: String,
    pub output_path: String,
    pub pass: bool,
    #[serde(default)]
    pub input_valid: bool,
    #[serde(default)]
    pub output_ready: bool,
    #[serde(default)]
    pub supported_files_written: bool,
    #[serde(default)]
    pub unsupported_files_omitted: bool,
    #[serde(default)]
    pub redaction_scan_clear: bool,
    #[serde(default)]
    pub warnings_clear: bool,
    #[serde(default)]
    pub evidence_complete: bool,
    #[serde(default)]
    pub sanitize_ready: bool,
    pub totals: CaptureSanitizeTotals,
    pub files: Vec<SanitizedFileReport>,
    pub warnings: Vec<String>,
    pub issues: Vec<String>,
    #[serde(default)]
    pub next_actions: Vec<CaptureSanitizeNextAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CaptureSanitizeTotals {
    pub files_seen: usize,
    pub files_written: usize,
    pub files_omitted: usize,
    pub secret_redactions: u64,
    pub identifier_pseudonyms: u64,
    pub email_redactions: u64,
    pub authorization_redactions: u64,
    pub mac_pseudonyms: u64,
    pub jwt_redactions: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizedFileReport {
    pub path: String,
    pub format: String,
    pub input_sha256: String,
    pub output_sha256: Option<String>,
    pub input_bytes: u64,
    pub output_bytes: Option<u64>,
    pub written: bool,
    pub omitted: bool,
    pub redactions: CaptureSanitizeRedactions,
    pub warnings: Vec<String>,
    pub issues: Vec<String>,
    #[serde(default)]
    pub next_actions: Vec<CaptureSanitizeNextAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct CaptureSanitizeNextAction {
    pub scope: String,
    pub reason: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CaptureSanitizeRedactions {
    pub secret_redactions: u64,
    pub identifier_pseudonyms: u64,
    pub email_redactions: u64,
    pub authorization_redactions: u64,
    pub mac_pseudonyms: u64,
    pub jwt_redactions: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureSanitizeManifest {
    pub schema: String,
    pub generated_by: String,
    pub files: Vec<SanitizedFileManifest>,
    pub totals: CaptureSanitizeTotals,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizedFileManifest {
    pub path: String,
    pub format: String,
    pub sha256: String,
    pub byte_len: u64,
    pub redactions: CaptureSanitizeRedactions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureFileFormat {
    Json,
    Jsonl,
    Text,
    Binary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyTreatment {
    Preserve,
    Secret,
    Identifier,
}

pub fn sanitize_capture_path(
    options: CaptureSanitizeOptions<'_>,
) -> BullResult<CaptureSanitizeReport> {
    if options.salt.trim().is_empty() {
        return Err(BullError::message("sanitize salt must not be empty"));
    }

    let input_path = options.input_path;
    let output_path = options.output_path;

    if !input_path.exists() {
        return Err(BullError::message(format!(
            "input path does not exist: {}",
            input_path.display()
        )));
    }

    let mut files = Vec::new();
    let mut warnings = Vec::new();
    let mut issues = Vec::new();
    let mut totals = CaptureSanitizeTotals::default();

    if input_path.is_dir() {
        reject_output_inside_input(input_path, output_path)?;
        fs::create_dir_all(output_path).map_err(|source| BullError::io(output_path, source))?;
        let mut paths = Vec::new();
        collect_files(input_path, &mut paths)?;
        paths.sort();
        for path in paths {
            let relative = relative_path(input_path, &path);
            let result = sanitize_one_file(&path, &relative, output_path, options.salt)?;
            merge_file_result(&mut totals, &result);
            if !result.warnings.is_empty() {
                warnings.extend(
                    result
                        .warnings
                        .iter()
                        .map(|warning| format!("{}: {warning}", result.path)),
                );
            }
            if !result.issues.is_empty() {
                issues.extend(
                    result
                        .issues
                        .iter()
                        .map(|issue| format!("{}: {issue}", result.path)),
                );
            }
            files.push(result);
        }
        write_sanitize_manifest(output_path, &files, &totals, &warnings)?;
    } else {
        let relative = input_path
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("capture"));
        if let Some(parent) = output_path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).map_err(|source| BullError::io(parent, source))?;
        }
        let output_root = output_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let temp_result = sanitize_one_file(input_path, &relative, output_root, options.salt)?;
        let target_path = output_root.join(&relative);
        if temp_result.written && target_path != output_path {
            fs::rename(&target_path, output_path)
                .map_err(|source| BullError::io(output_path, source))?;
        }
        merge_file_result(&mut totals, &temp_result);
        warnings.extend(
            temp_result
                .warnings
                .iter()
                .map(|warning| format!("{}: {warning}", temp_result.path)),
        );
        issues.extend(
            temp_result
                .issues
                .iter()
                .map(|issue| format!("{}: {issue}", temp_result.path)),
        );
        files.push(SanitizedFileReport {
            path: output_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("capture")
                .to_string(),
            ..temp_result
        });
    }
    let next_actions = capture_sanitize_report_next_actions(&files);
    let input_valid = true;
    let output_ready = totals.files_seen == files.len()
        && totals.files_seen == totals.files_written + totals.files_omitted
        && files.iter().all(|file| {
            if file.written {
                !file.omitted && file.output_sha256.is_some() && file.output_bytes.is_some()
            } else {
                file.omitted && file.output_sha256.is_none() && file.output_bytes.is_none()
            }
        });
    let supported_files_written = files.iter().filter(|file| !file.omitted).all(|file| {
        file.written
            && file.output_sha256.is_some()
            && file.output_bytes.is_some()
            && file.issues.is_empty()
    });
    let unsupported_files_omitted = files
        .iter()
        .filter(|file| file.omitted)
        .all(|file| file.format == "binary" && !file.written);
    let redaction_scan_clear = issues.is_empty() && files.iter().all(|file| file.issues.is_empty());
    let warnings_clear = warnings.is_empty() && files.iter().all(|file| file.warnings.is_empty());
    let evidence_complete = totals.files_omitted == 0 && warnings_clear && redaction_scan_clear;
    let sanitize_ready = input_valid
        && output_ready
        && supported_files_written
        && unsupported_files_omitted
        && redaction_scan_clear;

    Ok(CaptureSanitizeReport {
        schema: CAPTURE_SANITIZE_REPORT_SCHEMA.to_string(),
        generated_by: "bull-capture-sanitize".to_string(),
        input_path: input_path.display().to_string(),
        output_path: output_path.display().to_string(),
        pass: sanitize_ready,
        input_valid,
        output_ready,
        supported_files_written,
        unsupported_files_omitted,
        redaction_scan_clear,
        warnings_clear,
        evidence_complete,
        sanitize_ready,
        totals,
        files,
        warnings,
        issues,
        next_actions,
    })
}

pub fn sanitize_json_value(value: &mut serde_json::Value, salt: &str) -> CaptureSanitizeRedactions {
    let mut redactions = CaptureSanitizeRedactions::default();
    sanitize_json_value_inner(value, salt, None, &mut redactions);
    redactions
}

pub fn sanitize_text(text: &str, salt: &str) -> (String, CaptureSanitizeRedactions) {
    let mut redactions = CaptureSanitizeRedactions::default();
    let mut sanitized_lines = Vec::new();

    for line in text.lines() {
        let mut sanitized = redact_text_key_value_line(line, salt, &mut redactions);
        sanitized = redact_authorization_tokens(&sanitized, &mut redactions);
        sanitized = redact_jwt_tokens(&sanitized, &mut redactions);
        sanitized = redact_email_tokens(&sanitized, &mut redactions);
        sanitized = redact_mac_addresses(&sanitized, salt, &mut redactions);
        sanitized_lines.push(sanitized);
    }

    let mut sanitized = sanitized_lines.join("\n");
    if text.ends_with('\n') {
        sanitized.push('\n');
    }
    (sanitized, redactions)
}

fn sanitize_one_file(
    input_path: &Path,
    relative_path: &Path,
    output_root: &Path,
    salt: &str,
) -> BullResult<SanitizedFileReport> {
    let bytes = fs::read(input_path).map_err(|source| BullError::io(input_path, source))?;
    let input_sha256 = sha256_hex(&bytes);
    let format = classify_file(input_path, &bytes);
    let mut warnings = Vec::new();
    let mut issues = Vec::new();
    let mut redactions = CaptureSanitizeRedactions::default();

    if matches!(format, CaptureFileFormat::Binary) {
        return Ok(SanitizedFileReport {
            path: slash_path(relative_path),
            format: "binary".to_string(),
            input_sha256,
            output_sha256: None,
            input_bytes: bytes.len() as u64,
            output_bytes: None,
            written: false,
            omitted: true,
            redactions,
            warnings: vec![
                "binary file omitted; v1 sanitizer only preserves text/JSON protocol evidence"
                    .to_string(),
            ],
            issues,
            next_actions: capture_sanitize_file_next_actions(
                &slash_path(relative_path),
                &[
                    "binary file omitted; v1 sanitizer only preserves text/JSON protocol evidence"
                        .to_string(),
                ],
                &[],
            ),
        });
    }

    let text = String::from_utf8(bytes.clone()).map_err(|source| {
        BullError::message(format!("{} is not UTF-8: {source}", input_path.display()))
    })?;

    let sanitized = match format {
        CaptureFileFormat::Json => match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(mut value) => {
                redactions += sanitize_json_value(&mut value, salt);
                serde_json::to_string_pretty(&value).map_err(|source| {
                    BullError::message(format!("cannot serialize sanitized JSON: {source}"))
                })? + "\n"
            }
            Err(source) => {
                warnings.push(format!(
                    "JSON parse failed; sanitized as text instead: {source}"
                ));
                let (sanitized, text_redactions) = sanitize_text(&text, salt);
                redactions += text_redactions;
                sanitized
            }
        },
        CaptureFileFormat::Jsonl => {
            sanitize_jsonl_text(&text, salt, &mut warnings, &mut redactions)?
        }
        CaptureFileFormat::Text => {
            let (sanitized, text_redactions) = sanitize_text(&text, salt);
            redactions += text_redactions;
            sanitized
        }
        CaptureFileFormat::Binary => unreachable!("binary files returned before UTF-8 decode"),
    };

    issues.extend(scan_sanitized_text_for_leaks(&sanitized));

    let output_path = output_root.join(relative_path);
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|source| BullError::io(parent, source))?;
    }
    fs::write(&output_path, sanitized.as_bytes())
        .map_err(|source| BullError::io(&output_path, source))?;
    let output_sha256 = sha256_hex(sanitized.as_bytes());

    Ok(SanitizedFileReport {
        path: slash_path(relative_path),
        format: format.as_str().to_string(),
        input_sha256,
        output_sha256: Some(output_sha256),
        input_bytes: bytes.len() as u64,
        output_bytes: Some(sanitized.len() as u64),
        written: true,
        omitted: false,
        redactions,
        next_actions: capture_sanitize_file_next_actions(
            &slash_path(relative_path),
            &warnings,
            &issues,
        ),
        warnings,
        issues,
    })
}

fn capture_sanitize_report_next_actions(
    files: &[SanitizedFileReport],
) -> Vec<CaptureSanitizeNextAction> {
    dedupe_capture_sanitize_next_actions(
        files
            .iter()
            .flat_map(|file| file.next_actions.iter().cloned())
            .collect(),
    )
}

fn capture_sanitize_file_next_actions(
    path: &str,
    warnings: &[String],
    issues: &[String],
) -> Vec<CaptureSanitizeNextAction> {
    let mut actions = Vec::new();
    for warning in warnings {
        let (reason, action) = if warning.contains("binary file omitted") {
            (
                "binary_file_omitted",
                "Convert the capture to supported text/JSON/JSONL evidence or document that this binary artifact is not needed before using the sanitized bundle as parser evidence.",
            )
        } else if warning.contains("JSON parse failed") || warning.contains("not valid JSON") {
            (
                "json_parse_fallback",
                "Fix the source capture exporter to emit valid JSON/JSONL, or keep this text fallback as debug-only evidence with a parser regression.",
            )
        } else {
            (
                "sanitize_warning",
                "Review the sanitizer warning and decide whether the affected capture remains usable as trusted parser evidence.",
            )
        };
        actions.push(CaptureSanitizeNextAction {
            scope: path.to_string(),
            reason: reason.to_string(),
            action: action.to_string(),
        });
    }
    for issue in issues {
        let (reason, action) = capture_sanitize_issue_action(issue);
        actions.push(CaptureSanitizeNextAction {
            scope: path.to_string(),
            reason: reason.to_string(),
            action: action.to_string(),
        });
    }
    dedupe_capture_sanitize_next_actions(actions)
}

fn capture_sanitize_issue_action(issue: &str) -> (&'static str, &'static str) {
    if issue.contains("Bearer token") || issue.contains("Authorization header") {
        (
            "secret_redaction_failed",
            "Update the sanitizer redaction rule, regenerate the sanitized capture, then run privacy lint before importing this evidence.",
        )
    } else if issue.contains("email-like") {
        (
            "email_redaction_failed",
            "Update email redaction or remove the affected line, regenerate the sanitized capture, then run privacy lint before importing this evidence.",
        )
    } else if issue.contains("JWT-like") {
        (
            "jwt_redaction_failed",
            "Update JWT redaction, regenerate the sanitized capture, then run privacy lint before importing this evidence.",
        )
    } else if issue.contains("MAC-address-like") {
        (
            "mac_pseudonym_failed",
            "Update MAC pseudonymization, regenerate the sanitized capture, then run privacy lint before importing this evidence.",
        )
    } else {
        (
            "sanitize_leak_check_failed",
            "Fix the sanitizer rule for this leak class, regenerate the capture, and add a regression fixture before trusting the sanitized bundle.",
        )
    }
}

fn dedupe_capture_sanitize_next_actions(
    actions: Vec<CaptureSanitizeNextAction>,
) -> Vec<CaptureSanitizeNextAction> {
    let mut seen = BTreeSet::new();
    let mut deduped = Vec::new();
    for action in actions {
        let key = format!("{}:{}:{}", action.scope, action.reason, action.action);
        if seen.insert(key) {
            deduped.push(action);
        }
    }
    deduped
}

fn sanitize_jsonl_text(
    text: &str,
    salt: &str,
    warnings: &mut Vec<String>,
    redactions: &mut CaptureSanitizeRedactions,
) -> BullResult<String> {
    let mut output = Vec::new();

    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            output.push(String::new());
            continue;
        }

        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(mut value) => {
                *redactions += sanitize_json_value(&mut value, salt);
                output.push(serde_json::to_string(&value).map_err(|source| {
                    BullError::message(format!("cannot serialize sanitized JSONL: {source}"))
                })?);
            }
            Err(source) => {
                warnings.push(format!(
                    "line {} is not valid JSON; sanitized as text: {source}",
                    index + 1
                ));
                let (sanitized, line_redactions) = sanitize_text(line, salt);
                *redactions += line_redactions;
                output.push(sanitized);
            }
        }
    }

    let mut sanitized = output.join("\n");
    if text.ends_with('\n') {
        sanitized.push('\n');
    }
    Ok(sanitized)
}

fn sanitize_json_value_inner(
    value: &mut serde_json::Value,
    salt: &str,
    parent_key: Option<&str>,
    redactions: &mut CaptureSanitizeRedactions,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                match key_treatment(key) {
                    KeyTreatment::Preserve => {
                        sanitize_json_value_inner(child, salt, Some(key), redactions);
                    }
                    KeyTreatment::Secret => {
                        *child = serde_json::Value::String(format!(
                            "<redacted:{}>",
                            normalized_key(key)
                        ));
                        redactions.secret_redactions += 1;
                    }
                    KeyTreatment::Identifier => {
                        let original = stable_json_scalar(child);
                        *child = serde_json::Value::String(pseudonym_marker(key, &original, salt));
                        redactions.identifier_pseudonyms += 1;
                    }
                }
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                sanitize_json_value_inner(child, salt, parent_key, redactions);
            }
        }
        serde_json::Value::String(text) => {
            if parent_key
                .map(key_treatment)
                .is_some_and(|treatment| treatment == KeyTreatment::Preserve)
            {
                return;
            }
            let (sanitized, text_redactions) = sanitize_text(text, salt);
            if sanitized != *text {
                *text = sanitized;
                *redactions += text_redactions;
            }
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
}

fn redact_text_key_value_line(
    line: &str,
    salt: &str,
    redactions: &mut CaptureSanitizeRedactions,
) -> String {
    let Some((separator_index, separator)) = find_key_value_separator(line) else {
        return line.to_string();
    };

    let key = line[..separator_index]
        .trim()
        .trim_matches('"')
        .trim_matches('\'');
    if key.len() > 80 || key.is_empty() {
        return line.to_string();
    }

    match key_treatment(key) {
        KeyTreatment::Preserve => line.to_string(),
        KeyTreatment::Secret => {
            redactions.secret_redactions += 1;
            format!(
                "{}{} <redacted:{}>",
                &line[..separator_index].trim_end(),
                separator,
                normalized_key(key)
            )
        }
        KeyTreatment::Identifier => {
            redactions.identifier_pseudonyms += 1;
            let value = line[separator_index + separator.len()..].trim();
            format!(
                "{}{} {}",
                &line[..separator_index].trim_end(),
                separator,
                pseudonym_marker(key, value, salt)
            )
        }
    }
}

fn redact_authorization_tokens(text: &str, redactions: &mut CaptureSanitizeRedactions) -> String {
    let lower = text.to_ascii_lowercase();
    if lower.contains("authorization:") {
        redactions.authorization_redactions += 1;
        let Some(index) = lower.find("authorization:") else {
            return text.to_string();
        };
        return format!("{}Authorization: <redacted:authorization>", &text[..index]);
    }

    let Some(index) = lower.find("bearer ") else {
        return text.to_string();
    };
    redactions.authorization_redactions += 1;
    format!("{}Bearer <redacted:bearer-token>", &text[..index])
}

fn redact_jwt_tokens(text: &str, redactions: &mut CaptureSanitizeRedactions) -> String {
    replace_tokens(text, |token| {
        if looks_like_jwt(token) {
            redactions.jwt_redactions += 1;
            Some("<redacted:jwt>".to_string())
        } else {
            None
        }
    })
}

fn redact_email_tokens(text: &str, redactions: &mut CaptureSanitizeRedactions) -> String {
    replace_tokens(text, |token| {
        if looks_like_email(token) {
            redactions.email_redactions += 1;
            Some("<redacted:email>".to_string())
        } else {
            None
        }
    })
}

fn redact_mac_addresses(
    text: &str,
    salt: &str,
    redactions: &mut CaptureSanitizeRedactions,
) -> String {
    let mut output = String::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        let remaining = &text[index..];
        if remaining.len() >= 17 && looks_like_mac_prefix(remaining) {
            let mac = &remaining[..17];
            output.push_str(&pseudonym_marker("mac_address", mac, salt));
            redactions.mac_pseudonyms += 1;
            index += 17;
        } else {
            let Some(ch) = remaining.chars().next() else {
                break;
            };
            output.push(ch);
            index += ch.len_utf8();
        }
    }
    output
}

fn scan_sanitized_text_for_leaks(text: &str) -> Vec<String> {
    let mut issues = BTreeSet::new();
    let lower = text.to_ascii_lowercase();
    if lower.contains("bearer ") {
        issues.insert("sanitized output still contains Bearer token marker".to_string());
    }
    if lower.contains("authorization:")
        && !lower.contains("authorization: <redacted:authorization>")
    {
        issues
            .insert("sanitized output still contains unredacted Authorization header".to_string());
    }
    for token in tokens(text) {
        if looks_like_email(&token) {
            issues.insert("sanitized output still contains email-like token".to_string());
        }
        if looks_like_jwt(&token) {
            issues.insert("sanitized output still contains JWT-like token".to_string());
        }
    }
    if contains_mac_address(text) {
        issues.insert("sanitized output still contains MAC-address-like token".to_string());
    }
    issues.into_iter().collect()
}

fn write_sanitize_manifest(
    output_path: &Path,
    files: &[SanitizedFileReport],
    totals: &CaptureSanitizeTotals,
    warnings: &[String],
) -> BullResult<()> {
    let manifest_files = files
        .iter()
        .filter(|file| file.written)
        .filter_map(|file| {
            Some(SanitizedFileManifest {
                path: file.path.clone(),
                format: file.format.clone(),
                sha256: file.output_sha256.clone()?,
                byte_len: file.output_bytes?,
                redactions: file.redactions.clone(),
            })
        })
        .collect();
    let manifest = CaptureSanitizeManifest {
        schema: CAPTURE_SANITIZE_MANIFEST_SCHEMA.to_string(),
        generated_by: "bull-capture-sanitize".to_string(),
        files: manifest_files,
        totals: totals.clone(),
        warnings: warnings.to_vec(),
    };
    let manifest_json = serde_json::to_vec_pretty(&manifest).map_err(|source| {
        BullError::message(format!("cannot serialize sanitize manifest: {source}"))
    })?;
    fs::write(output_path.join("sanitize-manifest.json"), manifest_json)
        .map_err(|source| BullError::io(output_path.join("sanitize-manifest.json"), source))?;
    Ok(())
}

fn merge_file_result(totals: &mut CaptureSanitizeTotals, result: &SanitizedFileReport) {
    totals.files_seen += 1;
    if result.written {
        totals.files_written += 1;
    }
    if result.omitted {
        totals.files_omitted += 1;
    }
    totals.secret_redactions += result.redactions.secret_redactions;
    totals.identifier_pseudonyms += result.redactions.identifier_pseudonyms;
    totals.email_redactions += result.redactions.email_redactions;
    totals.authorization_redactions += result.redactions.authorization_redactions;
    totals.mac_pseudonyms += result.redactions.mac_pseudonyms;
    totals.jwt_redactions += result.redactions.jwt_redactions;
}

fn collect_files(root: &Path, files: &mut Vec<PathBuf>) -> BullResult<()> {
    for entry in fs::read_dir(root).map_err(|source| BullError::io(root, source))? {
        let entry = entry.map_err(|source| BullError::io(root, source))?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, files)?;
        } else if path.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn reject_output_inside_input(input_path: &Path, output_path: &Path) -> BullResult<()> {
    let input = input_path
        .canonicalize()
        .map_err(|source| BullError::io(input_path, source))?;
    let output_parent = output_path.parent().unwrap_or_else(|| Path::new("."));
    let output_parent = output_parent
        .canonicalize()
        .unwrap_or_else(|_| output_parent.to_path_buf());
    if output_parent.starts_with(&input) {
        return Err(BullError::message(
            "output path must not be inside the input capture directory",
        ));
    }
    Ok(())
}

fn relative_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

fn slash_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => part.to_str().map(ToOwned::to_owned),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn classify_file(path: &Path, bytes: &[u8]) -> CaptureFileFormat {
    if bytes.contains(&0) || std::str::from_utf8(bytes).is_err() {
        return CaptureFileFormat::Binary;
    }
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("json") => CaptureFileFormat::Json,
        Some("jsonl") | Some("ndjson") => CaptureFileFormat::Jsonl,
        _ => CaptureFileFormat::Text,
    }
}

impl CaptureFileFormat {
    fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Jsonl => "jsonl",
            Self::Text => "text",
            Self::Binary => "binary",
        }
    }
}

impl std::ops::AddAssign for CaptureSanitizeRedactions {
    fn add_assign(&mut self, rhs: Self) {
        self.secret_redactions += rhs.secret_redactions;
        self.identifier_pseudonyms += rhs.identifier_pseudonyms;
        self.email_redactions += rhs.email_redactions;
        self.authorization_redactions += rhs.authorization_redactions;
        self.mac_pseudonyms += rhs.mac_pseudonyms;
        self.jwt_redactions += rhs.jwt_redactions;
    }
}

fn key_treatment(key: &str) -> KeyTreatment {
    let normalized = normalized_key(key);
    if is_preserved_key(&normalized) {
        KeyTreatment::Preserve
    } else if is_secret_key(&normalized) {
        KeyTreatment::Secret
    } else if is_identifier_key(&normalized) {
        KeyTreatment::Identifier
    } else {
        KeyTreatment::Preserve
    }
}

fn normalized_key(key: &str) -> String {
    key.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn is_preserved_key(key: &str) -> bool {
    matches!(
        key,
        "appversion"
            | "bodyhex"
            | "byteshex"
            | "capturedat"
            | "characteristicuuid"
            | "commandhex"
            | "devicefirmware"
            | "devicemodel"
            | "direction"
            | "eventhex"
            | "firmware"
            | "firmwareversion"
            | "framehex"
            | "operation"
            | "packethex"
            | "payloadhex"
            | "rawbyteshex"
            | "rawpayloadhex"
            | "requesthex"
            | "responsehex"
            | "rssi"
            | "serviceuuid"
            | "timestamp"
            | "uuid"
            | "valuehex"
    )
}

fn is_secret_key(key: &str) -> bool {
    key.contains("authorization")
        || key.contains("accesstoken")
        || key.contains("refreshtoken")
        || key.contains("idtoken")
        || key.contains("authtoken")
        || key.contains("apikey")
        || key.contains("clientsecret")
        || key.contains("password")
        || key.contains("secret")
        || key.contains("cookie")
        || key == "token"
}

fn is_identifier_key(key: &str) -> bool {
    key.contains("email")
        || key.contains("phone")
        || key.contains("memberid")
        || key.contains("userid")
        || key.contains("accountid")
        || key.contains("profileid")
        || key.contains("deviceid")
        || key.contains("serial")
        || key == "mac"
        || key.contains("macaddress")
        || key.contains("bluetoothaddress")
        || key.contains("androidid")
        || key.contains("iosid")
        || key.contains("advertisingid")
        || key.contains("installationid")
        || key.contains("sessionid")
        || key == "name"
}

fn stable_json_scalar(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

fn pseudonym_marker(key: &str, value: &str, salt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(b"\0");
    hasher.update(normalized_key(key).as_bytes());
    hasher.update(b"\0");
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    format!(
        "<pseudonym:{}:{}>",
        normalized_key(key),
        hex::encode(&digest[..8])
    )
}

fn find_key_value_separator(line: &str) -> Option<(usize, &'static str)> {
    let equals = line.find('=');
    let colon = line.find(':');
    match (equals, colon) {
        (Some(equals), Some(colon)) if equals < colon => Some((equals, "=")),
        (Some(_), Some(colon)) => Some((colon, ":")),
        (Some(equals), None) => Some((equals, "=")),
        (None, Some(colon)) => Some((colon, ":")),
        (None, None) => None,
    }
}

fn replace_tokens(text: &str, mut replacer: impl FnMut(&str) -> Option<String>) -> String {
    let mut output = String::with_capacity(text.len());
    let mut token = String::new();
    for ch in text.chars() {
        if is_token_char(ch) {
            token.push(ch);
        } else {
            flush_token(&mut output, &mut token, &mut replacer);
            output.push(ch);
        }
    }
    flush_token(&mut output, &mut token, &mut replacer);
    output
}

fn flush_token(
    output: &mut String,
    token: &mut String,
    replacer: &mut impl FnMut(&str) -> Option<String>,
) {
    if token.is_empty() {
        return;
    }
    if let Some(replacement) = replacer(token) {
        output.push_str(&replacement);
    } else {
        output.push_str(token);
    }
    token.clear();
}

fn tokens(text: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut token = String::new();
    for ch in text.chars() {
        if is_token_char(ch) {
            token.push(ch);
        } else if !token.is_empty() {
            values.push(std::mem::take(&mut token));
        }
    }
    if !token.is_empty() {
        values.push(token);
    }
    values
}

fn is_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '%' | '+' | '@' | '/' | '=')
}

fn looks_like_email(token: &str) -> bool {
    let Some(at_index) = token.find('@') else {
        return false;
    };
    at_index > 0
        && token[at_index + 1..].contains('.')
        && !token.ends_with('@')
        && !token.starts_with('@')
}

fn looks_like_jwt(token: &str) -> bool {
    token.len() > 60 && token.starts_with("eyJ") && token.matches('.').count() == 2
}

fn looks_like_mac_prefix(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() < 17 {
        return false;
    }
    for index in 0..17 {
        if matches!(index, 2 | 5 | 8 | 11 | 14) {
            if bytes[index] != b':' {
                return false;
            }
        } else if !bytes[index].is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

fn contains_mac_address(text: &str) -> bool {
    let mut index = 0;
    while index < text.len() {
        let remaining = &text[index..];
        if remaining.len() >= 17 && looks_like_mac_prefix(remaining) {
            return true;
        }
        let Some(ch) = remaining.chars().next() else {
            break;
        };
        index += ch.len_utf8();
    }
    false
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}
