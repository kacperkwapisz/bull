//! Behavior → daily-metric insight engine.
//!
//! The journal lets a user log daily behaviors (each a tag they opted into, e.g.
//! `alcohol`, `late_caffeine`, `read_before_bed`). This module turns those logs
//! plus a daily metric (recovery or sleep score) into an honest, deterministic
//! "which behaviors are associated with higher or lower <metric>" summary.
//!
//! Design principles:
//! - **Association, never proven cause.** Output is explicitly flagged
//!   `correlation_only`. A behavior that co-occurs with a confounder (e.g.
//!   "meditation" logged mostly on stressful days) can read as harmful; the
//!   summary is a starting point for reflection, not a causal claim. (A future
//!   revision may decouple co-occurring behaviors with a regression model.)
//! - **Honest under-sampling.** A behavior is only reported once it has enough
//!   days *both* with and without it; otherwise it is listed as insufficient,
//!   never as a confident claim.
//! - **Plain difference of means.** Impact is `mean metric on days with` minus
//!   `mean metric on days without` — no opaque model, fully reproducible.
//! - **No fabricated data.** Only days that actually carry a score are analyzed;
//!   missing scores are skipped, not imputed.
//!
//! All inputs are user-provided journal entries and scores already derived from
//! the band's own sensors; nothing here ingests third-party health data.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// One day of journal + daily-metric data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyBehaviorRecord {
    /// ISO `YYYY-MM-DD` for the local day, carried through for traceability.
    pub date: String,
    /// The daily metric being analyzed (e.g. recovery or sleep score), `0..=100`.
    /// `None` days are excluded from analysis.
    pub score: Option<f64>,
    /// Behavior tags the user logged as active that day.
    pub behaviors: BTreeSet<String>,
}

/// Qualitative strength of a behavior's association with the metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpactStrength {
    Strong,
    Moderate,
    Weak,
}

/// A single behavior's measured association with the metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorImpact {
    pub behavior: String,
    pub days_with: usize,
    pub days_without: usize,
    pub mean_with: f64,
    pub mean_without: f64,
    /// `mean_with - mean_without`. Positive = the metric tends to be higher on
    /// days with this behavior; negative = lower.
    pub delta: f64,
    pub strength: ImpactStrength,
}

/// The full insight summary.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BehaviorInsights {
    /// Which daily metric these impacts explain (e.g. `recovery`, `sleep`).
    pub metric: String,
    /// Days that carried a score and were analyzed.
    pub analyzed_days: usize,
    /// Reportable impacts, sorted by `delta` descending (helpful first).
    pub impacts: Vec<BehaviorImpact>,
    /// Behaviors seen but lacking enough with/without days to report, sorted.
    pub insufficient: Vec<String>,
    /// Always `true`: impacts are associations, not proven causes.
    pub correlation_only: bool,
}

impl BehaviorInsights {
    /// Behaviors associated with a higher metric, strongest first.
    pub fn helpful(&self) -> Vec<&BehaviorImpact> {
        self.impacts.iter().filter(|i| i.delta > 0.0).collect()
    }

    /// Behaviors associated with a lower metric, most harmful first.
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
    /// Which daily metric the scores represent. Defaults to `recovery`.
    #[serde(default = "default_metric")]
    pub metric: String,
    #[serde(default)]
    pub config: Option<InsightConfig>,
}

fn default_metric() -> String {
    "recovery".to_string()
}

/// Tuning for the insight engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsightConfig {
    /// Minimum days a behavior must appear (and not appear) to be reportable.
    pub min_days_per_side: usize,
    /// Minimum analyzed days before any insight is produced at all.
    pub min_total_days: usize,
    /// `|delta|` at/above this (metric points) is a Strong association.
    pub strong_delta: f64,
    /// `|delta|` at/above this (metric points) is a Moderate association.
    pub moderate_delta: f64,
}

impl Default for InsightConfig {
    fn default() -> Self {
        Self {
            // A behavior needs enough yes-days and no-days before its impact is
            // trustworthy enough to surface.
            min_days_per_side: 5,
            min_total_days: 10,
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

/// Compute behavior → metric insights from journal + score data.
///
/// `metric` is a label describing what `score` measures (e.g. `recovery`,
/// `sleep`). Returns an empty (but well-formed) summary when there is not enough
/// data, rather than guessing.
pub fn compute_behavior_insights(
    records: &[DailyBehaviorRecord],
    metric: &str,
    config: &InsightConfig,
) -> BehaviorInsights {
    // Only days with a real score participate.
    let scored: Vec<(&BTreeSet<String>, f64)> = records
        .iter()
        .filter_map(|r| {
            r.score
                .filter(|s| s.is_finite())
                .map(|s| (&r.behaviors, s))
        })
        .collect();

    let analyzed_days = scored.len();
    let empty = BehaviorInsights {
        metric: metric.to_string(),
        analyzed_days,
        impacts: Vec::new(),
        insufficient: Vec::new(),
        correlation_only: true,
    };
    if analyzed_days < config.min_total_days {
        return empty;
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
        metric: metric.to_string(),
        analyzed_days,
        impacts,
        insufficient,
        correlation_only: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(date: &str, score: Option<f64>, behaviors: &[&str]) -> DailyBehaviorRecord {
        DailyBehaviorRecord {
            date: date.to_string(),
            score,
            behaviors: behaviors.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Build `n` days without the behavior at `base` score, then `m` days with
    /// it at `base + delta`, so tests read intent clearly.
    fn split_days(without: &[f64], with: &[f64], behavior: &str) -> Vec<DailyBehaviorRecord> {
        let mut out = Vec::new();
        let mut d = 1;
        for s in without {
            out.push(day(&format!("2026-01-{:02}", d), Some(*s), &[]));
            d += 1;
        }
        for s in with {
            out.push(day(&format!("2026-01-{:02}", d), Some(*s), &[behavior]));
            d += 1;
        }
        out
    }

    #[test]
    fn too_few_days_yields_empty() {
        let records = split_days(&[60.0, 62.0], &[40.0, 42.0], "alcohol");
        let out = compute_behavior_insights(&records, "recovery", &InsightConfig::default());
        assert_eq!(out.metric, "recovery");
        assert_eq!(out.analyzed_days, 4);
        assert!(out.impacts.is_empty());
        assert!(out.correlation_only);
    }

    #[test]
    fn unscored_days_are_excluded() {
        let mut records = Vec::new();
        for i in 0..12 {
            records.push(day(&format!("2026-01-{:02}", i + 1), None, &["alcohol"]));
        }
        let out = compute_behavior_insights(&records, "recovery", &InsightConfig::default());
        assert_eq!(out.analyzed_days, 0);
        assert!(out.impacts.is_empty());
    }

    #[test]
    fn detects_a_harmful_behavior() {
        // 5 dry days ~60, 5 alcohol days ~40 → ~20 point drop.
        let records = split_days(
            &[60.0, 62.0, 58.0, 61.0, 59.0],
            &[40.0, 42.0, 38.0, 41.0, 39.0],
            "alcohol",
        );
        let out = compute_behavior_insights(&records, "recovery", &InsightConfig::default());
        assert_eq!(out.analyzed_days, 10);
        assert_eq!(out.impacts.len(), 1);
        let impact = &out.impacts[0];
        assert_eq!(impact.behavior, "alcohol");
        assert_eq!(impact.days_with, 5);
        assert_eq!(impact.days_without, 5);
        assert!(impact.delta < 0.0, "alcohol should lower the metric");
        assert_eq!(impact.strength, ImpactStrength::Strong);
        assert_eq!(out.harmful().len(), 1);
        assert!(out.helpful().is_empty());
    }

    #[test]
    fn under_sampled_behavior_is_marked_insufficient() {
        // `meditation` appears on only 2 days → not reportable; `alcohol` is fine.
        let mut records = split_days(
            &[60.0, 62.0, 58.0, 61.0, 59.0],
            &[40.0, 42.0, 38.0, 41.0, 39.0],
            "alcohol",
        );
        records[0].behaviors.insert("meditation".to_string());
        records[1].behaviors.insert("meditation".to_string());
        let out = compute_behavior_insights(&records, "recovery", &InsightConfig::default());
        assert!(out.impacts.iter().all(|i| i.behavior != "meditation"));
        assert!(out.insufficient.contains(&"meditation".to_string()));
    }

    #[test]
    fn helpful_and_harmful_are_ordered() {
        // hydration helps (+), late_screen hurts (-), 5 each + 5 neutral days.
        let mut records = Vec::new();
        for (i, s) in [70.0, 72.0, 74.0, 71.0, 73.0].iter().enumerate() {
            records.push(day(&format!("2026-01-{:02}", i + 1), Some(*s), &["hydration"]));
        }
        for (i, s) in [50.0, 48.0, 52.0, 49.0, 51.0].iter().enumerate() {
            records.push(day(&format!("2026-01-{:02}", i + 6), Some(*s), &["late_screen"]));
        }
        for (i, s) in [60.0, 61.0, 59.0, 60.0, 62.0].iter().enumerate() {
            records.push(day(&format!("2026-01-{:02}", i + 11), Some(*s), &[]));
        }
        let out = compute_behavior_insights(&records, "recovery", &InsightConfig::default());
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
    fn metric_label_flows_through() {
        let records = split_days(
            &[80.0, 82.0, 78.0, 81.0, 79.0],
            &[60.0, 62.0, 58.0, 61.0, 59.0],
            "late_meal",
        );
        let out = compute_behavior_insights(&records, "sleep", &InsightConfig::default());
        assert_eq!(out.metric, "sleep");
        assert_eq!(out.impacts[0].behavior, "late_meal");
    }

    #[test]
    fn bridge_round_trip() {
        // Exercise the dispatcher end-to-end so the registered method, args
        // shape, and serialized result stay in sync.
        let records: Vec<_> = [60.0, 62.0, 58.0, 61.0, 59.0]
            .iter()
            .enumerate()
            .map(|(i, s)| {
                serde_json::json!({ "date": format!("2026-01-{:02}", i + 1), "score": s, "behaviors": [] })
            })
            .chain([40.0, 42.0, 38.0, 41.0, 39.0].iter().enumerate().map(|(i, s)| {
                serde_json::json!({ "date": format!("2026-01-{:02}", i + 6), "score": s, "behaviors": ["alcohol"] })
            }))
            .collect();
        let request = serde_json::json!({
            "schema": crate::bridge::BRIDGE_REQUEST_SCHEMA,
            "request_id": "test-behavior-insights",
            "method": "behavior.insights",
            "args": { "records": records, "metric": "recovery" },
        });
        let response_json = crate::bridge::handle_bridge_request_json(&request.to_string());
        let response: serde_json::Value = serde_json::from_str(&response_json).unwrap();
        assert_eq!(response["ok"], serde_json::json!(true), "{response_json}");
        let result = &response["result"];
        assert_eq!(result["metric"], serde_json::json!("recovery"));
        assert_eq!(result["analyzed_days"], serde_json::json!(10));
        assert_eq!(result["correlation_only"], serde_json::json!(true));
        assert_eq!(result["impacts"][0]["behavior"], serde_json::json!("alcohol"));
        assert!(result["impacts"][0]["delta"].as_f64().unwrap() < 0.0);
    }

    #[test]
    fn output_is_deterministic() {
        let records = split_days(
            &[60.0, 62.0, 58.0, 61.0, 59.0],
            &[40.0, 42.0, 38.0, 41.0, 39.0],
            "alcohol",
        );
        let cfg = InsightConfig::default();
        let a = compute_behavior_insights(&records, "recovery", &cfg);
        let b = compute_behavior_insights(&records, "recovery", &cfg);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }
}
