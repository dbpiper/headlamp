use std::path::Path;

use headlamp_core::format::ctx::make_ctx;
use headlamp_core::format::vitest::render_vitest_from_test_model;
use headlamp_core::test_model::{TestRunAggregated, TestRunModel};

fn empty_success_model() -> TestRunModel {
    TestRunModel {
        start_time: 0,
        test_results: vec![],
        aggregated: TestRunAggregated {
            num_total_test_suites: 0,
            num_passed_test_suites: 0,
            num_failed_test_suites: 0,
            num_total_tests: 0,
            num_passed_tests: 0,
            num_failed_tests: 0,
            num_pending_tests: 0,
            num_todo_tests: 0,
            num_timed_out_tests: None,
            num_timed_out_test_suites: None,
            start_time: 0,
            success: true,
            run_time_ms: Some(0),
        },
    }
}

#[test]
fn only_failures_still_prints_footer_for_empty_successful_run() {
    let cwd = Path::new("/repo");
    let ctx = make_ctx(cwd, Some(80), true, false, None);
    let rendered = render_vitest_from_test_model(&empty_success_model(), &ctx, true);
    assert!(rendered.contains("Failed Tests 0"));
    assert!(rendered.contains("Test Files"));
    assert!(rendered.contains("Tests"));
}
