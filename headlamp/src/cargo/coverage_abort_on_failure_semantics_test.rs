use headlamp_core::args::ParsedArgs;

fn base_args() -> ParsedArgs {
    ParsedArgs {
        runner_args: vec![],
        selection_paths: vec![],
        selection_specified: false,
        keep_artifacts: false,
        watch: false,
        ci: false,
        verbose: false,
        quiet: false,
        no_cache: false,
        collect_coverage: true,
        coverage_ui: headlamp_core::config::CoverageUi::Both,
        coverage_abort_on_failure: true,
        coverage_detail: None,
        coverage_show_code: false,
        coverage_mode: headlamp_core::config::CoverageMode::Auto,
        coverage_max_files: None,
        coverage_max_hotspots: None,
        coverage_page_fit: false,
        coverage_thresholds: None,
        include_globs: vec![],
        exclude_globs: vec![],
        editor_cmd: None,
        workspace_root: None,
        only_failures: false,
        show_logs: false,
        sequential: false,
        bootstrap_command: None,
        changed: None,
        changed_depth: None,
        dependency_language: None,
    }
}

fn model_with_failed_tests(
    num_failed_tests: u64,
    num_failed_suites: u64,
) -> headlamp_core::test_model::TestRunModel {
    headlamp_core::test_model::TestRunModel {
        start_time: 0,
        test_results: vec![],
        aggregated: headlamp_core::test_model::TestRunAggregated {
            num_total_test_suites: 0,
            num_passed_test_suites: 0,
            num_failed_test_suites: num_failed_suites,
            num_total_tests: 0,
            num_passed_tests: 0,
            num_failed_tests,
            num_pending_tests: 0,
            num_todo_tests: 0,
            num_timed_out_tests: None,
            num_timed_out_test_suites: None,
            start_time: 0,
            success: num_failed_tests == 0 && num_failed_suites == 0,
            run_time_ms: None,
        },
    }
}

#[test]
fn coverage_abort_on_failure_only_aborts_when_model_has_failures() {
    let args = base_args();

    let model_no_failures = model_with_failed_tests(0, 0);
    assert!(!super::should_abort_coverage_after_run(
        &args,
        &model_no_failures
    ));

    let model_failed_tests = model_with_failed_tests(1, 0);
    assert!(super::should_abort_coverage_after_run(
        &args,
        &model_failed_tests
    ));

    let model_failed_suites = model_with_failed_tests(0, 1);
    assert!(super::should_abort_coverage_after_run(
        &args,
        &model_failed_suites
    ));
}
