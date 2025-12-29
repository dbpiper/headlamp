use crate::cargo::empty_test_run_model_for_exit_code;

#[test]
fn empty_test_run_model_for_exit_code_does_not_claim_phantom_failures() {
    let model = empty_test_run_model_for_exit_code(1);
    assert!(!model.aggregated.success);
    assert_eq!(model.aggregated.num_total_test_suites, 0);
    assert_eq!(model.aggregated.num_failed_test_suites, 0);
    assert_eq!(model.aggregated.num_total_tests, 0);
    assert_eq!(model.aggregated.num_failed_tests, 0);
}
