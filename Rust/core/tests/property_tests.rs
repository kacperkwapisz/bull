use bull_core::property_tests::{
    PROPERTY_TEST_REPORT_SCHEMA, PropertyFailure, PropertyGroupReport, PropertySuiteOptions,
    property_group_next_actions, property_suite_next_actions, property_suite_report_from_groups,
    run_property_suite,
};
use serde_json::json;

#[test]
fn property_suite_passes_with_deterministic_seed() {
    let report = run_property_suite(PropertySuiteOptions {
        seed: 42,
        cases_per_group: 32,
    })
    .unwrap();

    assert_eq!(report.schema, PROPERTY_TEST_REPORT_SCHEMA);
    assert_eq!(report.generated_by, "bull-property-test-suite");
    assert_eq!(report.seed, 42);
    assert!(report.pass, "{:#?}", report.issues);
    assert!(report.input_valid);
    assert!(report.parser_properties_valid);
    assert!(report.deframer_properties_valid);
    assert!(report.algorithm_bounds_valid);
    assert!(report.algorithm_metamorphic_valid);
    assert!(report.all_groups_valid);
    assert!(report.property_suite_ready);
    assert!(report.issues.is_empty());

    let group_names = report
        .groups
        .iter()
        .map(|group| group.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        group_names,
        vec![
            "parser_frame_invariants",
            "deframer_stream_invariants",
            "algorithm_bounds_and_quality_invariants",
            "algorithm_metamorphic_invariants",
        ]
    );
    assert!(report.groups.iter().all(|group| group.pass));
    assert!(report.groups.iter().all(|group| group.checks > 0));
    assert!(report.groups.iter().all(|group| group.failures.is_empty()));
    assert!(
        report
            .groups
            .iter()
            .all(|group| group.next_actions.is_empty())
    );
    assert!(report.next_actions.is_empty());
}

#[test]
fn property_suite_report_fields_name_the_blocked_invariant_family() {
    let failures = vec![PropertyFailure {
        case_index: 7,
        property: "recovery_hrv_improvement_non_decreasing".to_string(),
        message: "raising HRV lowered recovery".to_string(),
        context: json!({
            "low_hrv_score": 72.0,
            "high_hrv_score": 70.0
        }),
    }];
    let groups = vec![
        passing_group("parser_frame_invariants"),
        passing_group("deframer_stream_invariants"),
        passing_group("algorithm_bounds_and_quality_invariants"),
        PropertyGroupReport {
            name: "algorithm_metamorphic_invariants".to_string(),
            pass: false,
            cases: 1,
            checks: 1,
            failures: failures.clone(),
            next_actions: property_group_next_actions(
                "algorithm_metamorphic_invariants",
                &failures,
            ),
        },
    ];

    let report = property_suite_report_from_groups(42, 1, groups);

    assert!(!report.pass);
    assert!(report.input_valid);
    assert!(report.parser_properties_valid);
    assert!(report.deframer_properties_valid);
    assert!(report.algorithm_bounds_valid);
    assert!(!report.algorithm_metamorphic_valid);
    assert!(!report.all_groups_valid);
    assert!(!report.property_suite_ready);
    assert!(
        report
            .issues
            .contains(&"algorithm_metamorphic_invariants failed 1 checks".to_string())
    );
    assert!(report.next_actions.iter().any(|action| {
        action.scope
            == "algorithm_metamorphic_invariants:recovery_hrv_improvement_non_decreasing:case_7"
            && action.reason == "algorithm_metamorphic_failure"
    }));
}

#[test]
fn property_suite_next_actions_pin_failing_group_property_and_case() {
    let failures = vec![PropertyFailure {
        case_index: 7,
        property: "recovery_hrv_improvement_non_decreasing".to_string(),
        message: "raising HRV lowered recovery".to_string(),
        context: json!({
            "low_hrv_score": 72.0,
            "high_hrv_score": 70.0
        }),
    }];
    let group_actions = property_group_next_actions("algorithm_metamorphic_invariants", &failures);
    let groups = vec![PropertyGroupReport {
        name: "algorithm_metamorphic_invariants".to_string(),
        pass: false,
        cases: 1,
        checks: 1,
        failures,
        next_actions: group_actions,
    }];

    let actions = property_suite_next_actions(&groups);

    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].scope,
        "algorithm_metamorphic_invariants:recovery_hrv_improvement_non_decreasing:case_7"
    );
    assert_eq!(actions[0].reason, "algorithm_metamorphic_failure");
    assert!(actions[0].action.contains("hand-derived regression"));
}

fn passing_group(name: &str) -> PropertyGroupReport {
    PropertyGroupReport {
        name: name.to_string(),
        pass: true,
        cases: 1,
        checks: 1,
        failures: Vec::new(),
        next_actions: Vec::new(),
    }
}

#[test]
fn property_suite_requires_non_zero_cases() {
    let error = run_property_suite(PropertySuiteOptions {
        seed: 42,
        cases_per_group: 0,
    })
    .unwrap_err();

    assert!(error.to_string().contains("cases_per_group"));
}
