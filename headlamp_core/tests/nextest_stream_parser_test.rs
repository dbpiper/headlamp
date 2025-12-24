use std::path::Path;

use headlamp_core::format::nextest::NextestStreamParser;

#[test]
fn nextest_stream_parser_emits_updates_and_finalizes() {
    let repo_root = Path::new("/repo");
    let mut parser = NextestStreamParser::new(repo_root);

    let lines = [
        r#"{"type":"suite","event":"started","test_count":2,"nextest":{"crate":"parity_sum","test_binary":"sum_test","kind":"test"}}"#,
        r#"{"type":"test","event":"started","name":"parity_sum::sum_test$sum_passes"}"#,
        r#"{"type":"test","event":"started","name":"parity_sum::sum_test$sum_fails"}"#,
        r#"{"type":"test","event":"failed","name":"parity_sum::sum_test$sum_fails","exec_time":0.01,"stdout":"fail"}"#,
        r#"{"type":"suite","event":"failed","passed":1,"failed":1,"ignored":0,"measured":0,"filtered_out":0,"exec_time":0.02,"nextest":{"crate":"parity_sum","test_binary":"sum_test","kind":"test"}}"#,
        r#"{"type":"test","event":"ok","name":"parity_sum::sum_test$sum_passes","exec_time":0.01}"#,
    ];

    let updates = lines
        .iter()
        .filter_map(|line| parser.push_line(line))
        .collect::<Vec<_>>();

    assert!(updates.iter().any(|u| u.status == "failed"));
    assert!(updates.iter().any(|u| u.status == "passed"));

    let model = parser.finalize().expect("model");
    assert_eq!(model.test_results.len(), 1);
    assert_eq!(model.test_results[0].test_results.len(), 2);
}


