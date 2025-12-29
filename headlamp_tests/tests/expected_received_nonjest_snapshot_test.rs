use headlamp::format::ctx::make_ctx;
use headlamp::format::vitest::render_vitest_from_test_model;
use headlamp::test_model::{TestCaseResult, TestRunAggregated, TestRunModel, TestSuiteResult};

#[test]
fn render_expected_received_from_rust_left_right_snapshot() {
    let model = TestRunModel {
        start_time: 0,
        test_results: vec![TestSuiteResult {
            test_file_path: "/repo/tests/sum_test.rs".to_string(),
            status: "failed".to_string(),
            timed_out: None,
            failure_message: String::new(),
            failure_details: None,
            test_exec_error: None,
            console: None,
            test_results: vec![TestCaseResult {
                title: "test_sum_fails".to_string(),
                full_name: "test_sum_fails".to_string(),
                status: "failed".to_string(),
                timed_out: None,
                duration: 1,
                location: None,
                failure_messages: vec![String::from(
                    "assertion `left == right` failed\n  left: 1\n right: 2\n",
                )],
                failure_details: None,
            }],
        }],
        aggregated: TestRunAggregated {
            num_total_test_suites: 1,
            num_passed_test_suites: 0,
            num_failed_test_suites: 1,
            num_total_tests: 1,
            num_passed_tests: 0,
            num_failed_tests: 1,
            num_pending_tests: 0,
            num_todo_tests: 0,
            num_timed_out_tests: None,
            num_timed_out_test_suites: None,
            start_time: 0,
            success: false,
            run_time_ms: Some(1),
        },
    };
    let ctx = make_ctx(std::path::Path::new("/repo"), Some(80), true, false, None);
    let out = render_vitest_from_test_model(&model, &ctx, false);
    insta::assert_snapshot!(
        "render_expected_received_from_rust_left_right_snapshot",
        out
    );
}
