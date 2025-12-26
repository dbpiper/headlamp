use std::path::Path;

use headlamp::format::nextest::parse_nextest_libtest_json_output;

#[test]
fn parses_ok_test_after_suite_failed_event() {
    let repo_root = Path::new("/repo");
    let combined = [
        r#"{"type":"suite","event":"started","test_count":2,"nextest":{"crate":"parity_sum","test_binary":"sum_test","kind":"test"}}"#,
        r#"{"type":"test","event":"started","name":"parity_sum::sum_test$sum_passes"}"#,
        r#"{"type":"test","event":"started","name":"parity_sum::sum_test$sum_fails"}"#,
        r#"{"type":"test","event":"failed","name":"parity_sum::sum_test$sum_fails","exec_time":0.01,"stdout":"fail"}"#,
        r#"{"type":"suite","event":"failed","passed":1,"failed":1,"ignored":0,"measured":0,"filtered_out":0,"exec_time":0.02,"nextest":{"crate":"parity_sum","test_binary":"sum_test","kind":"test"}}"#,
        r#"{"type":"test","event":"ok","name":"parity_sum::sum_test$sum_passes","exec_time":0.01}"#,
    ]
    .join("\n");

    let model = parse_nextest_libtest_json_output(repo_root, &combined).expect("model");
    assert_eq!(model.test_results.len(), 1);
    assert_eq!(model.test_results[0].test_results.len(), 2);
    assert!(
        model.test_results[0]
            .test_results
            .iter()
            .any(|t| t.full_name == "sum_passes" && t.status == "passed")
    );
    assert!(
        model.test_results[0]
            .test_results
            .iter()
            .any(|t| t.full_name == "sum_fails" && t.status == "failed")
    );
}
