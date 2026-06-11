use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const ACTIVITY_IDENTITY_KEY_VERSION: &str = "v1";
pub const ACTIVITY_IDENTITY_KEY_PREFIX: &str = "bull:activity-session:v1:";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActivityIdentityInput {
    pub source: String,
    pub provenance: Value,
    pub start_time: String,
    pub end_time: String,
    pub activity_type: String,
    #[serde(default)]
    pub raw_identifiers: Vec<String>,
    #[serde(default)]
    pub labels: Vec<String>,
}

pub fn activity_idempotency_key(input: &ActivityIdentityInput) -> String {
    let canonical_payload = canonical_activity_identity_payload(input);
    let digest = Sha256::digest(canonical_payload.as_bytes());
    format!("{ACTIVITY_IDENTITY_KEY_PREFIX}{}", hex::encode(digest))
}

fn canonical_activity_identity_payload(input: &ActivityIdentityInput) -> String {
    let payload = Value::Object(
        [
            (
                "activity_type".to_string(),
                Value::String(normalized_marker(&input.activity_type)),
            ),
            (
                "end_time".to_string(),
                Value::String(canonical_time(&input.end_time)),
            ),
            (
                "labels".to_string(),
                Value::Array(
                    canonical_labels(&input.labels)
                        .into_iter()
                        .map(Value::String)
                        .collect(),
                ),
            ),
            ("provenance".to_string(), input.provenance.clone()),
            (
                "raw_identifiers".to_string(),
                Value::Array(
                    canonical_raw_identifiers(&input.raw_identifiers)
                        .into_iter()
                        .map(Value::String)
                        .collect(),
                ),
            ),
            (
                "source".to_string(),
                Value::String(input.source.trim().to_string()),
            ),
            (
                "start_time".to_string(),
                Value::String(canonical_time(&input.start_time)),
            ),
            (
                "version".to_string(),
                Value::String(ACTIVITY_IDENTITY_KEY_VERSION.to_string()),
            ),
        ]
        .into_iter()
        .collect(),
    );
    canonical_json(&payload)
}

fn canonical_raw_identifiers(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn canonical_labels(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| normalized_marker(value))
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => {
            serde_json::to_string(value).expect("JSON strings are always serializable")
        }
        Value::Array(values) => {
            let mut output = String::from("[");
            for (index, item) in values.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push_str(&canonical_json(item));
            }
            output.push(']');
            output
        }
        Value::Object(map) => {
            let mut keys = map.keys().collect::<Vec<_>>();
            keys.sort();

            let mut output = String::from("{");
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push_str(
                    &serde_json::to_string(key).expect("JSON object keys are always serializable"),
                );
                output.push(':');
                output.push_str(&canonical_json(
                    map.get(*key).expect("sorted key must exist in map"),
                ));
            }
            output.push('}');
            output
        }
    }
}

fn canonical_time(value: &str) -> String {
    parse_utc_instant(value)
        .map(|instant| instant.0.to_string())
        .unwrap_or_else(|| value.trim().to_string())
}

fn normalized_marker(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|char| {
            if char.is_ascii_alphanumeric() {
                char
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct UtcInstant(i128);

fn parse_utc_instant(value: &str) -> Option<UtcInstant> {
    let value = value.trim();
    let (date, time_and_offset) = value.split_once('T')?;
    let (year, month, day) = parse_date(date)?;
    let (time, offset_seconds) = split_time_and_offset(time_and_offset)?;
    let (hour, minute, second, nanos) = parse_time(time)?;
    let days = days_from_civil(year, month, day)?;
    let seconds = i128::from(days) * 86_400
        + i128::from(hour) * 3_600
        + i128::from(minute) * 60
        + i128::from(second)
        - i128::from(offset_seconds);
    Some(UtcInstant(seconds * 1_000_000_000 + i128::from(nanos)))
}

fn parse_date(value: &str) -> Option<(i32, u32, u32)> {
    let mut parts = value.split('-');
    let year = parts.next()?.parse::<i32>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() || value.len() != 10 {
        return None;
    }
    if !(1..=12).contains(&month) {
        return None;
    }
    if !(1..=days_in_month(year, month)).contains(&day) {
        return None;
    }
    Some((year, month, day))
}

fn split_time_and_offset(value: &str) -> Option<(&str, i32)> {
    if let Some(time) = value.strip_suffix('Z') {
        return Some((time, 0));
    }
    let split_index = value.rfind(['+', '-'])?;
    if split_index == 0 {
        return None;
    }
    let (time, offset) = value.split_at(split_index);
    let sign = if offset.starts_with('+') { 1 } else { -1 };
    let offset = &offset[1..];
    let (hour, minute) = offset.split_once(':')?;
    if hour.len() != 2 || minute.len() != 2 {
        return None;
    }
    let hour = hour.parse::<i32>().ok()?;
    let minute = minute.parse::<i32>().ok()?;
    if !(0..=23).contains(&hour) || !(0..=59).contains(&minute) {
        return None;
    }
    Some((time, sign * (hour * 3_600 + minute * 60)))
}

fn parse_time(value: &str) -> Option<(u32, u32, u32, u32)> {
    let mut parts = value.split(':');
    let hour = parts.next()?.parse::<u32>().ok()?;
    let minute = parts.next()?.parse::<u32>().ok()?;
    let seconds_part = parts.next()?;
    if parts.next().is_some() || hour > 23 || minute > 59 {
        return None;
    }
    let (second_text, fraction_text) = seconds_part
        .split_once('.')
        .map_or((seconds_part, None), |(second, fraction)| {
            (second, Some(fraction))
        });
    let second = second_text.parse::<u32>().ok()?;
    if second > 59 {
        return None;
    }
    let nanos = match fraction_text {
        Some(fraction) if fraction.is_empty() || fraction.len() > 9 => return None,
        Some(fraction) => {
            if !fraction.chars().all(|char| char.is_ascii_digit()) {
                return None;
            }
            let mut padded = fraction.to_string();
            while padded.len() < 9 {
                padded.push('0');
            }
            padded.parse::<u32>().ok()?
        }
        None => 0,
    };
    Some((hour, minute, second, nanos))
}

fn days_from_civil(year: i32, month: u32, day: u32) -> Option<i64> {
    let month_i32 = i32::try_from(month).ok()?;
    let day_i32 = i32::try_from(day).ok()?;
    let year = year - i32::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month_prime = month_i32 + if month_i32 > 2 { -3 } else { 9 };
    let doy = (153 * month_prime + 2) / 5 + day_i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(i64::from(era) * 146_097 + i64::from(doe) - 719_468)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}
