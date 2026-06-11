use std::path::Path;

const REQUIRED_CLI_BINS: &[&str] = &[
    "bull-fixture-index",
    "bull-capture-sanitize",
    "bull-capture-sqlite-import",
    "bull-parser-fixture-runner",
    "bull-capture-correlation",
    "bull-metric-input-readiness",
    "bull-capture-arrival-plan",
    "bull-command-capture-plan",
    "bull-metric-feature-report",
    "bull-local-health-validation-suite",
    "bull-command-validator",
    "bull-export-validator",
    "bull-reference-algo-runner",
    "bull-algo-benchmark",
    "bull-calibration-evaluator",
    "bull-health-sync-dry-run",
    "bull-debug-ws-contract",
    "bull-debug-ws-serve",
    "bull-ui-coverage-audit",
    "bull-storage-check",
    "bull-property-test-suite",
    "bull-perf-budget",
    "bull-privacy-lint",
];

const REQUIRED_DOC_ENTRIES: &[&str] = &[
    "`bull-metric-input-readiness` / `metrics.input_readiness`",
    "`bull-capture-arrival-plan` / `capture.arrival_plan`",
    "`bull-command-capture-plan` / `commands.capture_plan`",
    "`bull-capture-sqlite-import`",
    "`bull-metric-feature-report motion` / `metrics.motion_features`",
    "`bull-metric-feature-report heart-rate` / `metrics.heart_rate_features`",
    "`bull-metric-feature-report vital-event` / `metrics.vital_event_features`",
    "`bull-metric-feature-report step-discovery` / `metrics.step_packet_discovery`",
    "`bull-metric-feature-report step-validation` / `metrics.step_capture_validation`",
    "`bull-metric-feature-report raw-motion-steps` / `metrics.raw_motion_step_estimate`",
    "`bull-metric-feature-report step-counter-ingest` / `metrics.step_counter_ingest`",
    "`bull-metric-feature-report step-rollup` / `metrics.step_counter_daily_rollup`",
    "`bull-metric-feature-report steps-unavailable-status` / `metrics.activity_unavailable_daily_status`",
    "`bull-metric-feature-report calories-unavailable-status` / `metrics.energy_unavailable_daily_status`",
    "`bull-metric-feature-report hrv` / `metrics.hrv_features`",
    "`bull-metric-feature-report hrv-validation` / `metrics.hrv_capture_validation`",
    "`bull-metric-feature-report respiratory-rate-validation` / `metrics.respiratory_rate_capture_validation`",
    "`bull-metric-feature-report recovery-sensors` / `metrics.recovery_sensor_discovery`",
    "`bull-metric-feature-report recovery-unavailable-status` / `metrics.recovery_unavailable_daily_status`",
    "`bull-metric-feature-report window` / `metrics.window_features`",
    "`bull-metric-feature-report resting-hr` / `metrics.resting_hr_features`",
    "`bull-metric-feature-report rhr-rollup` / `metrics.resting_hr_daily_rollup`",
    "`bull-metric-feature-report rhr-validation` / `metrics.resting_hr_capture_validation`",
    "`bull-metric-feature-report sleep-score` / `metrics.sleep_score_from_features`",
    "`bull-metric-feature-report recovery-score` / `metrics.recovery_score_from_features`",
    "`bull-metric-feature-report strain-score` / `metrics.strain_score_from_features`",
    "`bull-metric-feature-report stress-score` / `metrics.stress_score_from_features`",
    "`bull-local-health-validation-suite`",
];

#[test]
fn required_machine_readable_tools_are_registered_as_cargo_bins() {
    let manifest = read_workspace_file("Cargo.toml");
    for bin in REQUIRED_CLI_BINS {
        assert!(
            manifest.contains(&format!("name = \"{bin}\"")),
            "Cargo.toml missing required Bull tool bin {bin}"
        );
        assert!(
            manifest.contains(&format!("path = \"src/bin/{bin}.rs\"")),
            "Cargo.toml missing expected path for Bull tool bin {bin}"
        );
    }
}

#[test]
fn testing_strategy_names_scriptable_tools_for_bridge_gates() {
    let strategy = read_bull_file("docs/testing-and-tooling-strategy.md");
    for entry in REQUIRED_DOC_ENTRIES {
        assert!(
            strategy.contains(entry),
            "testing strategy missing scriptable tooling entry {entry}"
        );
    }
    assert!(
        strategy.contains("6. `bull-capture-arrival-plan` / `capture.arrival_plan`"),
        "Immediate Tool Order should name the standalone capture arrival plan CLI"
    );
    assert!(
        strategy.contains("25. `bull-debug-ws-serve`"),
        "Immediate Tool Order should include the debug WebSocket serve tool"
    );
}

fn read_workspace_file(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("cannot read {path:?}: {error}"))
}

fn read_bull_file(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("cannot read {path:?}: {error}"))
}
