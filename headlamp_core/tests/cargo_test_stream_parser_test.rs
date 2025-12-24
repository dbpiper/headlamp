use std::path::Path;

use headlamp_core::format::cargo_test::{CargoTestStreamEvent, CargoTestStreamParser};

#[test]
fn cargo_test_stream_parser_emits_test_finished_and_finalizes_with_failure_details() {
    let repo_root = Path::new("/repo");
    let mut parser = CargoTestStreamParser::new(repo_root);

    let combined = [
        "Running unittests src/lib.rs (target/debug/deps/pkg-abc123)",
        "test pass_test ... ok",
        "test fail_test ... FAILED",
        "",
        "failures:",
        "",
        "---- fail_test stdout ----",
        "log line",
        "",
        "thread 'fail_test' panicked at src/lib.rs:1:1:",
        "boom",
        "",
        "failures:",
        "    fail_test",
        "",
        "test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s",
    ]
    .join("\n");

    let events = combined
        .lines()
        .flat_map(|line| parser.push_line(line))
        .collect::<Vec<_>>();

    assert!(events.iter().any(|e| matches!(
        e,
        CargoTestStreamEvent::TestFinished { test_name, status, .. }
            if test_name == "fail_test" && status == "failed"
    )));

    let model = parser.finalize().expect("model");
    assert_eq!(model.test_results.len(), 1);
    let suite = &model.test_results[0];
    assert!(suite.test_results.iter().any(|t| t.full_name == "fail_test"));
    let failed = suite
        .test_results
        .iter()
        .find(|t| t.full_name == "fail_test")
        .expect("failed test present");
    assert_eq!(failed.status, "failed");
    assert!(!failed.failure_messages.is_empty());
}


