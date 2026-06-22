/// Readiness engine — "should you push today?" synthesis from daily metrics.
///
/// Ported from Noop's `ReadinessEngine.swift`. Pure, deterministic function of
/// daily metric rows — no networking, no strap commands, no state.
///
/// Signals and references:
/// - HRV readiness: z-score vs trailing baseline (Plews 2013; Buchheit 2014)
/// - Resting HR drift: elevated vs baseline (Lamberts 2004)
/// - Respiratory rate drift: illness early signal
/// - Training Stress Balance (ACWR): acute (7d) vs chronic (28d) strain (Gabbett 2016)
/// - Training monotony: mean/SD of daily strain (Foster 1998)
use serde::Serialize;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessLevel {
    Primed,
    Balanced,
    Strained,
    Rundown,
    Insufficient,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalFlag {
    Good,
    Neutral,
    Watch,
    Bad,
}

#[derive(Debug, Clone, Serialize)]
pub struct Signal {
    pub key: String,
    pub label: String,
    pub evidence: Option<String>,
    pub detail: String,
    pub flag: SignalFlag,
}

#[derive(Debug, Clone, Serialize)]
pub struct Readiness {
    pub level: ReadinessLevel,
    pub headline: String,
    pub summary: String,
    pub signals: Vec<Signal>,
    pub acwr: Option<f64>,
    pub monotony: Option<f64>,
}

/// One day of metrics for readiness evaluation.
#[derive(Debug, Clone)]
pub struct DailyMetricRow {
    pub day: String,
    pub hrv: Option<f64>,
    pub resting_hr: Option<f64>,
    pub resp_rate: Option<f64>,
    pub strain: Option<f64>,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const BASELINE_WINDOW: usize = 30;
const MIN_BASELINE: usize = 7;
const ACUTE_WINDOW: usize = 7;
const CHRONIC_WINDOW: usize = 28;
const MIN_CHRONIC: usize = 14;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Evaluate readiness from daily metrics. `today` is "YYYY-MM-DD"; when
/// provided, only that day is used as "latest" (prevents stale reads).
pub fn evaluate(days: &[DailyMetricRow], today: Option<&str>) -> Readiness {
    let mut sorted: Vec<&DailyMetricRow> = days.iter().collect();
    sorted.sort_by(|a, b| a.day.cmp(&b.day));

    let latest = if let Some(today) = today {
        sorted.iter().find(|d| d.day == today).copied()
    } else {
        sorted.last().copied()
    };

    let Some(latest) = latest else {
        return insufficient("Wear the strap for a few nights and your readiness read will appear here.");
    };

    let history: Vec<&&DailyMetricRow> = sorted.iter().filter(|d| d.day < latest.day).collect();

    let mut signals = Vec::new();

    // HRV readiness
    if let Some(s) = z_signal(
        latest.hrv,
        &history.iter().rev().take(BASELINE_WINDOW).filter_map(|d| d.hrv).collect::<Vec<_>>(),
        "hrv", "HRV", "ms", 0, true,
        "above your baseline — well recovered",
        "in your normal range",
        "a touch below baseline",
        "suppressed — a sign of autonomic fatigue",
    ) {
        signals.push(s);
    }

    // Resting HR drift
    if let Some(s) = z_signal(
        latest.resting_hr,
        &history.iter().rev().take(BASELINE_WINDOW).filter_map(|d| d.resting_hr).collect::<Vec<_>>(),
        "rhr", "Resting HR", "bpm", 0, false,
        "at or below baseline",
        "in your normal range",
        "running a little high",
        "elevated — overtraining or illness can do this",
    ) {
        signals.push(s);
    }

    // Respiratory rate drift
    if let Some(rr) = latest.resp_rate {
        if (8.0..=25.0).contains(&rr) {
            let base: Vec<f64> = history.iter().rev().take(BASELINE_WINDOW)
                .filter_map(|d| d.resp_rate)
                .filter(|r| (8.0..=25.0).contains(r))
                .collect();
            if base.len() >= MIN_BASELINE {
                if let (Some(m), Some(sd)) = (mean(&base), sample_sd(&base)) {
                    if sd > 0.0 && (8.0..=25.0).contains(&m) {
                        let z = (rr - m) / sd;
                        if z >= 2.0 {
                            signals.push(Signal {
                                key: "respRate".into(), label: "Respiratory rate".into(),
                                evidence: Some(format!("{:.1} vs {:.1} rpm", rr, m)),
                                detail: "up vs baseline — sometimes an early sign of getting sick".into(),
                                flag: SignalFlag::Bad,
                            });
                        } else if z >= 1.5 {
                            signals.push(Signal {
                                key: "respRate".into(), label: "Respiratory rate".into(),
                                evidence: Some(format!("{:.1} vs {:.1} rpm", rr, m)),
                                detail: "slightly raised vs baseline".into(),
                                flag: SignalFlag::Watch,
                            });
                        }
                    }
                }
            }
        }
    }

    // ACWR + monotony
    let strain_series: Vec<f64> = sorted.iter().filter_map(|d| d.strain).collect();
    let mut acwr = None;
    let mut monotony = None;

    if strain_series.len() >= MIN_CHRONIC {
        let acute = mean(&strain_series[strain_series.len().saturating_sub(ACUTE_WINDOW)..]);
        let chronic = mean(&strain_series[strain_series.len().saturating_sub(CHRONIC_WINDOW)..]);
        if let (Some(a), Some(c)) = (acute, chronic) {
            if c > 0.0 {
                let ratio = a / c;
                acwr = Some(ratio);
                signals.push(acwr_signal(ratio, a, c));
            }
        }
        let week = &strain_series[strain_series.len().saturating_sub(ACUTE_WINDOW)..];
        if week.len() >= 4 {
            if let (Some(m), Some(sd)) = (mean(week), sample_sd(week)) {
                if sd > 0.0 {
                    let mono = m / sd;
                    monotony = Some(mono);
                    if mono >= 2.0 {
                        signals.push(Signal {
                            key: "monotony".into(), label: "Training variety".into(),
                            evidence: Some(format!("monotony {:.1}", mono)),
                            detail: "low — similar strain every day raises strain/illness risk".into(),
                            flag: SignalFlag::Watch,
                        });
                    }
                }
            }
        }
    }

    let (level, headline, summary) = synthesize(&signals, !history.is_empty() || acwr.is_some());
    Readiness { level, headline, summary, signals, acwr, monotony }
}

// ---------------------------------------------------------------------------
// Signal builders
// ---------------------------------------------------------------------------

fn z_signal(
    value: Option<f64>, baseline: &[f64],
    key: &str, label: &str, unit: &str, decimals: usize, higher_is_better: bool,
    good: &str, neutral: &str, watch: &str, bad: &str,
) -> Option<Signal> {
    let v = value?;
    if baseline.len() < MIN_BASELINE { return None; }
    let m = mean(baseline)?;
    let sd = sample_sd(baseline)?;
    if sd <= 0.0 { return None; }
    // Orient z so positive = "better"
    let z = if higher_is_better { (v - m) / sd } else { (m - v) / sd };
    let (flag, text) = if z >= 0.5 {
        (SignalFlag::Good, good)
    } else if z >= -0.5 {
        (SignalFlag::Neutral, neutral)
    } else if z >= -1.0 {
        (SignalFlag::Watch, watch)
    } else {
        (SignalFlag::Bad, bad)
    };
    let ev = if decimals == 0 {
        format!("{:.0} vs {:.0} {}", v, m, unit)
    } else {
        format!("{:.1} vs {:.1} {}", v, m, unit)
    };
    Some(Signal { key: key.into(), label: label.into(), evidence: Some(ev), detail: text.into(), flag })
}

fn acwr_signal(ratio: f64, acute: f64, chronic: f64) -> Signal {
    let ev = format!("7d {:.1} / 28d {:.1}", acute, chronic);
    let pct = format!("{:.2}", ratio);
    let (detail, flag) = if ratio < 0.8 {
        (format!("ramping down (acute:chronic {}) — room to build", pct), SignalFlag::Watch)
    } else if ratio < 1.3 {
        (format!("in the sweet spot (acute:chronic {})", pct), SignalFlag::Good)
    } else if ratio < 1.5 {
        (format!("building fast (acute:chronic {}) — watch fatigue", pct), SignalFlag::Watch)
    } else {
        (format!("spiking (acute:chronic {}) — higher injury risk", pct), SignalFlag::Bad)
    };
    Signal { key: "acwr".into(), label: "Training load".into(), evidence: Some(ev), detail, flag }
}

// ---------------------------------------------------------------------------
// Synthesis
// ---------------------------------------------------------------------------

fn synthesize(signals: &[Signal], has_history: bool) -> (ReadinessLevel, String, String) {
    if !has_history || signals.is_empty() {
        return (ReadinessLevel::Insufficient, "Readiness".into(),
            "A few more nights of data and your readiness read will sharpen.".into());
    }
    let bad_count = signals.iter().filter(|s| s.flag == SignalFlag::Bad).count();
    let watch_count = signals.iter().filter(|s| s.flag == SignalFlag::Watch).count();
    let good_count = signals.iter().filter(|s| s.flag == SignalFlag::Good).count();
    let recovery_down = signals.iter().any(|s|
        ["hrv", "rhr", "respRate"].contains(&s.key.as_str()) && s.flag == SignalFlag::Bad);
    let load_high = signals.iter().any(|s| s.key == "acwr" && s.flag == SignalFlag::Bad);

    if bad_count >= 2 || (recovery_down && load_high) {
        (ReadinessLevel::Rundown, "Run down".into(),
         "Several signals are down at once. Treat today as recovery — easy movement, real sleep tonight.".into())
    } else if recovery_down || load_high || bad_count >= 1 {
        (ReadinessLevel::Strained, "Strained".into(),
         "One of your signals is flagging. You can train, but keep it controlled and bank the recovery.".into())
    } else if good_count >= 2 && watch_count == 0 {
        (ReadinessLevel::Primed, "Primed".into(),
         "Your signals are aligned and your load is supported. A harder session is well backed today.".into())
    } else {
        (ReadinessLevel::Balanced, "Balanced".into(),
         "Nothing's flagging. Train to feel — your body's holding steady.".into())
    }
}

fn insufficient(msg: &str) -> Readiness {
    Readiness {
        level: ReadinessLevel::Insufficient,
        headline: "Readiness".into(),
        summary: msg.into(),
        signals: vec![],
        acwr: None,
        monotony: None,
    }
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

fn mean(xs: &[f64]) -> Option<f64> {
    if xs.is_empty() { return None; }
    Some(xs.iter().sum::<f64>() / xs.len() as f64)
}

fn sample_sd(xs: &[f64]) -> Option<f64> {
    if xs.len() < 2 { return None; }
    let m = mean(xs)?;
    let ss: f64 = xs.iter().map(|x| (x - m).powi(2)).sum();
    Some((ss / (xs.len() - 1) as f64).sqrt())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn day(d: &str, hrv: Option<f64>, rhr: Option<f64>, strain: Option<f64>) -> DailyMetricRow {
        DailyMetricRow { day: d.into(), hrv, resting_hr: rhr, resp_rate: None, strain }
    }

    #[test]
    fn test_insufficient_with_no_data() {
        let r = evaluate(&[], None);
        assert_eq!(r.level, ReadinessLevel::Insufficient);
    }

    #[test]
    fn test_balanced_with_stable_metrics() {
        let mut days = Vec::new();
        for i in 1..=20 {
            days.push(day(&format!("2024-01-{:02}", i), Some(60.0), Some(55.0), Some(10.0)));
        }
        let r = evaluate(&days, None);
        assert!(
            matches!(r.level, ReadinessLevel::Balanced | ReadinessLevel::Primed),
            "stable metrics should be balanced or primed, got {:?}", r.level
        );
    }

    #[test]
    fn test_strained_with_low_hrv() {
        let mut days = Vec::new();
        for i in 1..=19 {
            // Natural variation so SD > 0
            let hrv = 60.0 + (i as f64 % 5.0) - 2.0;
            let rhr = 55.0 + (i as f64 % 3.0) - 1.0;
            days.push(day(&format!("2024-01-{:02}", i), Some(hrv), Some(rhr), Some(10.0)));
        }
        // Today: HRV crashed well below baseline
        days.push(day("2024-01-20", Some(30.0), Some(55.0), Some(10.0)));
        let r = evaluate(&days, None);
        assert!(
            matches!(r.level, ReadinessLevel::Strained | ReadinessLevel::Rundown),
            "crashed HRV should be strained/rundown, got {:?}", r.level
        );
    }

    #[test]
    fn test_rundown_with_multiple_bad_signals() {
        let mut days = Vec::new();
        for i in 1..=19 {
            let hrv = 60.0 + (i as f64 % 5.0) - 2.0;
            let rhr = 55.0 + (i as f64 % 3.0) - 1.0;
            days.push(day(&format!("2024-01-{:02}", i), Some(hrv), Some(rhr), Some(10.0)));
        }
        // Today: HRV crashed + RHR elevated
        days.push(day("2024-01-20", Some(30.0), Some(75.0), Some(10.0)));
        let r = evaluate(&days, None);
        assert_eq!(r.level, ReadinessLevel::Rundown, "multiple bad signals = rundown");
    }

    #[test]
    fn test_acwr_sweet_spot() {
        let mut days = Vec::new();
        for i in 1..=28 {
            days.push(day(&format!("2024-01-{:02}", i.min(28)), Some(60.0), Some(55.0), Some(10.0)));
        }
        let r = evaluate(&days, None);
        if let Some(acwr) = r.acwr {
            assert!((acwr - 1.0).abs() < 0.1, "steady load should be ~1.0 ACWR, got {}", acwr);
        }
    }
}
