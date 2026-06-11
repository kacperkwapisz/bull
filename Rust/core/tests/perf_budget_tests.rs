use bull_core::perf_budget::{
    PERF_BUDGET_REPORT_SCHEMA, PerfBudgetOptions, PerfBudgets, run_perf_budget,
};

#[test]
fn perf_budget_report_covers_parser_deframer_algorithms_and_export() {
    let report = run_perf_budget(PerfBudgetOptions {
        scale: 24,
        budgets: PerfBudgets::default(),
    })
    .unwrap();

    assert_eq!(report.schema, PERF_BUDGET_REPORT_SCHEMA);
    assert_eq!(report.generated_by, "bull-perf-budget");
    assert!(report.pass, "{:#?}", report.issues);
    assert!(report.input_valid);
    assert!(report.parser_workload_ready);
    assert!(report.deframer_workload_ready);
    assert!(report.score_workload_ready);
    assert!(report.export_workload_ready);
    assert!(report.duration_budget_ready);
    assert!(report.memory_budget_ready);
    assert!(report.correctness_ready);
    assert!(report.all_workloads_ready);
    assert!(report.perf_budget_ready);
    assert!(report.issues.is_empty());
    assert!(report.next_actions.is_empty());

    let workload_names = report
        .workloads
        .iter()
        .map(|workload| workload.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        workload_names,
        vec![
            "parser_frame_batch",
            "deframer_split_stream",
            "bull_score_batch",
            "raw_export_bundle",
        ]
    );
    assert!(report.workloads.iter().all(|workload| workload.pass));
    assert!(report.workloads.iter().all(|workload| workload.checks > 0));
    assert!(
        report
            .workloads
            .iter()
            .all(|workload| workload.next_actions.is_empty())
    );

    let export = report
        .workloads
        .iter()
        .find(|workload| workload.name == "raw_export_bundle")
        .unwrap();
    assert_eq!(export.details["raw_rows"], 24);
    assert_eq!(export.details["decoded_frame_rows"], 24);
    assert_eq!(export.details["packet_timeline_rows"], 24);
}

#[test]
fn perf_budget_reports_failed_budget_without_hiding_workload_context() {
    let report = run_perf_budget(PerfBudgetOptions {
        scale: 4,
        budgets: PerfBudgets {
            parser_max_estimated_peak_bytes: 1,
            ..PerfBudgets::default()
        },
    })
    .unwrap();

    assert!(!report.pass);
    assert!(report.input_valid);
    assert!(!report.parser_workload_ready);
    assert!(report.deframer_workload_ready);
    assert!(report.score_workload_ready);
    assert!(report.export_workload_ready);
    assert!(report.duration_budget_ready);
    assert!(!report.memory_budget_ready);
    assert!(report.correctness_ready);
    assert!(!report.all_workloads_ready);
    assert!(!report.perf_budget_ready);
    let parser = report
        .workloads
        .iter()
        .find(|workload| workload.name == "parser_frame_batch")
        .unwrap();
    assert!(!parser.pass);
    assert!(
        parser
            .issues
            .iter()
            .any(|issue| issue.contains("estimated peak"))
    );
    assert!(
        parser
            .next_actions
            .iter()
            .any(|action| action.reason == "memory_budget_exceeded"
                && action.action.contains("mobile memory budget")),
        "{:?}",
        parser.next_actions
    );
    assert!(
        report.next_actions.iter().any(|action| {
            action.scope == "parser_frame_batch" && action.reason == "memory_budget_exceeded"
        }),
        "{:?}",
        report.next_actions
    );
    assert_eq!(parser.details["parse_failures"], 0);
}

#[test]
fn perf_budget_requires_non_zero_scale() {
    let error = run_perf_budget(PerfBudgetOptions {
        scale: 0,
        budgets: PerfBudgets::default(),
    })
    .unwrap_err();

    assert!(error.to_string().contains("scale"));
}
