use crate::test_model::{TestRunAggregated, TestRunModel};

use crate::pytest::apply_run_timing_to_model;

#[test]
fn pytest_timing_applies_elapsed_ms_not_unix_timestamp() {
    let mut model = TestRunModel {
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
            run_time_ms: None,
        },
    };

    apply_run_timing_to_model(&mut model, 1_700_000_000_000, 1_500);

    assert_eq!(model.start_time, 1_700_000_000_000);
    assert_eq!(model.aggregated.start_time, 1_700_000_000_000);
    assert_eq!(model.aggregated.run_time_ms, Some(1_500));
}
