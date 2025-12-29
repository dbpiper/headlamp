use crate::test_model::{TestCaseResult, TestRunAggregated, TestRunModel, TestSuiteResult};

pub fn build_infra_failure_test_run_model(
    suite_path: &str,
    test_name: &str,
    failure_message: &str,
) -> TestRunModel {
    TestRunModel {
        start_time: 0,
        test_results: vec![TestSuiteResult {
            test_file_path: suite_path.to_string(),
            status: "failed".to_string(),
            timed_out: None,
            failure_message: String::new(),
            failure_details: None,
            test_exec_error: None,
            console: None,
            test_results: vec![TestCaseResult {
                title: test_name.to_string(),
                full_name: test_name.to_string(),
                status: "failed".to_string(),
                timed_out: None,
                duration: 0,
                location: None,
                failure_messages: vec![failure_message.to_string()],
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
            run_time_ms: Some(0),
        },
    }
}
