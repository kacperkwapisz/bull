//! Behavior → recovery insight engine.
//!
//! The journal lets a user log daily behaviors (each a free-form tag they opted
//! into, e.g. `alcohol`, `late_caffeine`, `read_before_bed`). This module turns
//! those logs plus each day's recovery score into an honest, deterministic
//! "which behaviors help or hurt your recovery" summary.
//!
//! Design principles:
//! - **Honest under-sampling.** A behavior is only reported once it has enough
//!   days *both* with and without it; otherwise it is listed as insufficient,
//!   never as a confident claim.
//! - **Plain difference of means.** Impact is `mean recovery on days with` minus
//!   `mean recovery on days without` — no opaque model, fully reproducible.
//! - **No fabricated data.** Only days that actually carry a recovery score are
//!   analyzed; missing scores are skipped, not imputed.
//!
//! All inputs are user-provided journal entries and recovery scores already
//! derived from the band's own sensors; nothing here ingests third-party data.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// One day of journal + recovery data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyBehaviorRecord {
    /// ISO `YYYY-MM-DD` for the local day, carried through for traceability.
    pub date: String,
    /// Recovery score in `0..=100`. `None` days are excluded from analysis.
    pub recovery_score: Option<f64>,
    /// Behavior tags the user logged as active that day.
    pub behaviors: BTreeSet<String>,
}

/// Qualitative strength of a behavior's association with recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpactStrength {
    Strong,
    Moderate,
    Weak,
}

/// A single behavior's measured association with recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorImpact {
    pub behavior: String,
    pub days_with: usize,
    pub days_without: usize,
    pub mean_with: f64,
    pub mean_without: f64,
    /// `mean_with - mean_without`. Positive = recovery tends to be higher on
    /// days with this behavior; negative = lower.
    pub delta: f64,
    pub strength: ImpactStrength,
}

/// The full insight summary.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BehaviorInsights {
    /// Days that carried a recovery score and were analyzed.
    pub analyzed_days: usize,
    /// Reportable impacts, sorted by `delta` descending (helpful first).
    pub impacts: Vec<BehaviorImpact>,
    /// Behaviors seen but lacking enough with/without days to report, sorted.
    pub insufficient: Vec<String>,
}

impl BehaviorInsights {
    /// Behaviors associated with higher recovery, strongest first.
    pub fn helpful(&self) -> Vec<&BehaviorImpact> {
        self.impacts.iter().filter(|i| i.delta > 0.0).collect()
    }

    /// Behaviors associated with lower recovery, most harmful first.
    pub fn harmful(&self) -> Vec<&BehaviorImpact> {
        self.impacts
            .iter()
            .rev()
            .filter(|i| i.delta < 0.0)
            .collect()
    }
}

/// Bridge arguments for `behavior.insights`.
#[derive(Debug, Clone, Deserialize)]
pub struct BehaviorInsightsArgs {
    pub records: Vec<DailyBehaviorRecord>,
    #[serde(default)]
    pub config: Option<InsightConfig>,
}

/// Tuning for the insight engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsightConfig {
    /// Minimum days a behavior must appear (and not appear) to be reportable.
    pub min_days_per_side: usize,
    /// Minimum analyzed days before any insight is produced at all.
    pub min_total_days: usize,
    /// `|delta|` at/above this (recovery points) is a Strong association.
    pub strong_delta: f64,
    /// `|delta|` at/above this (recovery points) is a Moderate association.
    pub moderate_delta: f64,
}

impl Default for InsightConfig {
    fn default() -> Self {
        Self {
            min_days_per_side: 3,
            min_total_days: 7,
            strong_delta: 10.0,
            moderate_delta: 5.0,
        }
    }
}

impl InsightConfig {
    fn strength_for(&self, delta: f64) -> ImpactStrength {
        let magnitude = delta.abs();
        if magnitude >= self.strong_delta {
            ImpactStrength::Strong
        } else if magnitude >= self.moderate_delta {
            ImpactStrength::Moderate
        } else {
            ImpactStrength::Weak
        }
    }
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

/// Compute behavior → recovery insights from journal + recovery data.
///
/// Returns an empty (but well-formed) summary when there is not enough data,
/// rather than guessing.
pub fn compute_behavior_insights(
    records: &[DailyBehaviorRecord],
    config: &InsightConfig,
) -> BehaviorInsights {
    // Only days with a real recovery score participate.
    let scored: Vec<(&BTreeSet<String>, f64)> = records
        .iter()
        .filter_map(|r| {
            r.recovery_score
                .filter(|s| s.is_finite())
                .map(|s| (&r.behaviors, s))
        })
        .collect();

    let analyzed_days = scored.len();
    if analyzed_days < config.min_total_days {
        return BehaviorInsights {
            analyzed_days,
            impacts: Vec::new(),
            insufficient: Vec::new(),
        };
    }

    // Collect the universe of behaviors actually logged on a scored day.
    let mut all_behaviors: BTreeSet<&String> = BTreeSet::new();
    for (behaviors, _) in &scored {
        for b in behaviors.iter() {
            all_behaviors.insert(b);
        }
    }

    // Deterministic ordering by behavior name keeps output stable for equal deltas.
    let mut with_scores: BTreeMap<&String, Vec<f64>> = BTreeMap::new();
    let mut without_scores: BTreeMap<&String, Vec<f64>> = BTreeMap::new();
    for behavior in &all_behaviors {
        for (behaviors, score) in &scored {
            if behaviors.contains(*behavior) {
                with_scores.entry(behavior).or_default().push(*score);
            } else {
                without_scores.entry(behavior).or_default().push(*score);
            }
        }
    }

    let mut impacts: Vec<BehaviorImpact> = Vec::new();
    let mut insufficient: Vec<String> = Vec::new();
    for behavior in &all_behaviors {
        let with = with_scores.get(behavior).map(Vec::as_slice).unwrap_or(&[]);
        let without = without_scores
            .get(behavior)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if with.len() < config.min_days_per_side || without.len() < config.min_days_per_side {
            insufficient.push((*behavior).clone());
            continue;
        }
        let mean_with = mean(with);
        let mean_without = mean(without);
        let delta = mean_with - mean_without;
        impacts.push(BehaviorImpact {
            behavior: (*behavior).clone(),
            days_with: with.len(),
            days_without: without.len(),
            mean_with,
            mean_without,
            delta,
            strength: config.strength_for(delta),
        });
    }

    // Sort helpful (largest positive delta) first; break ties by name for stability.
    impacts.sort_by(|a, b| {
        b.delta
            .partial_cmp(&a.delta)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.behavior.cmp(&b.behavior))
    });

    BehaviorInsights {
        analyzed_days,
        impacts,
        insufficient,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(date: &str, score: Option<f64>, behaviors: &[&str]) -> DailyBehaviorRecord {
        DailyBehaviorRecord {
            date: date.to_string(),
            recovery_score: score,
            behaviors: behaviors.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn too_few_days_yields_empty() {
        let records = vec![
            day("2026-01-01", Some(50.0), &["alcohol"]),
            day("2026-01-02", Some(60.0), &[]),
        ];
        let out = compute_behavior_insights(&records, &InsightConfig::default());
        assert_eq!(out.analyzed_days, 2);
        assert!(out.impacts.is_empty());
        assert!(out.insufficient.is_empty());
    }

    #[test]
    fn unscored_days_are_excluded() {
        let mut records = Vec::new();
        for i in 0..10 {
            records.push(day(&format!("2026-01-{:02}", i + 1), None, &["alcohol"]));
        }
        let out = compute_behavior_insights(&records, &InsightConfig::default());
        assert_eq!(out.analyzed_days, 0);
        assert!(out.impacts.is_empty());
    }

    #[test]
    fn detects_a_harmful_behavior() {
        // Alcohol days run ~20 points lower than dry days.
        let records = vec![
            day("2026-01-01", Some(40.0), &["alcohol"]),
            day("2026-01-02", Some(42.0), &["alcohol"]),
            day("2026-01-03", Some(38.0), &["alcohol"]),
            day("2026-01-04", Some(60.0), &[]),
            day("2026-01-05", Some(62.0), &[]),
            day("2026-01-06", Some(58.0), &[]),
            day("2026-01-07", Some(61.0), &[]),
        ];
        let out = compute_behavior_insights(&records, &InsightConfig::default());
        assert_eq!(out.analyzed_days, 7);
        assert_eq!(out.impacts.len(), 1);
        let impact = &out.impacts[0];
        assert_eq!(impact.behavior, "alcohol");
        assert_eq!(impact.days_with, 3);
        assert_eq!(impact.days_without, 4);
        assert!(impact.delta < 0.0, "alcohol should lower recovery");
        assert_eq!(impact.strength, ImpactStrength::Strong);
        assert_eq!(out.harmful().len(), 1);
        assert!(out.helpful().is_empty());
    }

    #[test]
    fn under_sampled_behavior_is_marked_insufficient() {
        // `meditation` only appears once → not reportable; `alcohol` is fine.
        let records = vec![
            day("2026-01-01", Some(40.0), &["alcohol"]),
            day("2026-01-02", Some(42.0), &["alcohol"]),
            day("2026-01-03", Some(38.0), &["alcohol", "meditation"]),
            day("2026-01-04", Some(60.0), &[]),
            day("2026-01-05", Some(62.0), &[]),
            day("2026-01-06", Some(58.0), &[]),
            day("2026-01-07", Some(61.0), &[]),
        ];
        let out = compute_behavior_insights(&records, &InsightConfig::default());
        assert!(out.impacts.iter().all(|i| i.behavior != "meditation"));
        assert!(out.insufficient.contains(&"meditation".to_string()));
    }

    #[test]
    fn helpful_and_harmful_are_ordered() {
        let records = vec![
            // hydration helps (+), late_screen hurts (-)
            day("2026-01-01", Some(70.0), &["hydration"]),
            day("2026-01-02", Some(72.0), &["hydration"]),
            day("2026-01-03", Some(74.0), &["hydration"]),
            day("2026-01-04", Some(50.0), &["late_screen"]),
            day("2026-01-05", Some(48.0), &["late_screen"]),
            day("2026-01-06", Some(52.0), &["late_screen"]),
            day("2026-01-07", Some(60.0), &[]),
            day("2026-01-08", Some(61.0), &[]),
            day("2026-01-09", Some(59.0), &[]),
        ];
        let out = compute_behavior_insights(&records, &InsightConfig::default());
        // Helpful first (hydration), harmful last (late_screen).
        assert_eq!(out.impacts.first().unwrap().behavior, "hydration");
        assert_eq!(out.impacts.last().unwrap().behavior, "late_screen");
        assert!(out.impacts.first().unwrap().delta > 0.0);
        assert!(out.impacts.last().unwrap().delta < 0.0);
    }

    #[test]
    fn strength_thresholds() {
        let cfg = InsightConfig::default();
        assert_eq!(cfg.strength_for(12.0), ImpactStrength::Strong);
        assert_eq!(cfg.strength_for(-12.0), ImpactStrength::Strong);
        assert_eq!(cfg.strength_for(6.0), ImpactStrength::Moderate);
        assert_eq!(cfg.strength_for(2.0), ImpactStrength::Weak);
    }

    #[test]
    fn bridge_round_trip() {
        // Exercise the dispatcher end-to-end so the registered method, args
        // shape, and serialized result stay in sync.
        let args = serde_json::json!({
            "records": [
                { "date": "2026-01-01", "recovery_score": 40.0, "behaviors": ["alcohol"] },
                { "date": "2026-01-02", "recovery_score": 42.0, "behaviors": ["alcohol"] },
                { "date": "2026-01-03", "recovery_score": 38.0, "behaviors": ["alcohol"] },
                { "date": "2026-01-04", "recovery_score": 60.0, "behaviors": [] },
                { "date": "2026-01-05", "recovery_score": 62.0, "behaviors": [] },
                { "date": "2026-01-06", "recovery_score": 58.0, "behaviors": [] },
                { "date": "2026-01-07", "recovery_score": 61.0, "behaviors": [] }
            ]
        });
        let request = serde_json::json!({
            "schema": crate::bridge::BRIDGE_REQUEST_SCHEMA,
            "request_id": "test-behavior-insights",
            "method": "behavior.insights",
            "args": args,
        });
        let response_json =
            crate::bridge::handle_bridge_request_json(&request.to_string());
        let response: serde_json::Value = serde_json::from_str(&response_json).unwrap();
        assert_eq!(response["ok"], serde_json::json!(true), "{response_json}");
        let result = &response["result"];
        assert_eq!(result["analyzed_days"], serde_json::json!(7));
        assert_eq!(result["impacts"][0]["behavior"], serde_json::json!("alcohol"));
        assert!(result["impacts"][0]["delta"].as_f64().unwrap() < 0.0);
    }

    #[test]
    fn output_is_deterministic() {
        let records = vec![
            day("2026-01-01", Some(40.0), &["alcohol"]),
            day("2026-01-02", Some(42.0), &["alcohol"]),
            day("2026-01-03", Some(38.0), &["alcohol"]),
            day("2026-01-04", Some(60.0), &[]),
            day("2026-01-05", Some(62.0), &[]),
            day("2026-01-06", Some(58.0), &[]),
            day("2026-01-07", Some(61.0), &[]),
        ];
        let cfg = InsightConfig::default();
        let a = compute_behavior_insights(&records, &cfg);
        let b = compute_behavior_insights(&records, &cfg);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }
}
