use std::path::Path;

use headlamp::format::cargo_test::{CargoTestStreamEvent, CargoTestStreamParser};

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
    assert!(
        suite
            .test_results
            .iter()
            .any(|t| t.full_name == "fail_test")
    );
    let failed = suite
        .test_results
        .iter()
        .find(|t| t.full_name == "fail_test")
        .expect("failed test present");
    assert_eq!(failed.status, "failed");
    assert!(!failed.failure_messages.is_empty());
}

#[test]
fn cargo_test_stream_parser_captures_multiline_panic_block_with_backtrace_and_left_right() {
    let repo_root = Path::new("/repo");
    let mut parser = CargoTestStreamParser::new(repo_root);

    let combined = [
        "Running tests/sum_test.rs (target/debug/deps/sum_test-0000000000000000)",
        "test pass_test ... ok",
        "test fail_test ... FAILED",
        "",
        "failures:",
        "",
        "---- fail_test stdout ----",
        "",
        "thread 'fail_test' panicked at src/lib.rs:1:1:",
        "assertion `left == right` failed",
        "  left: 1",
        " right: 2",
        "stack backtrace:",
        "   0: rust_begin_unwind",
        "             at /repo/src/lib.rs:1:1",
        "",
        "failures:",
        "    fail_test",
        "",
        "test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s",
    ]
    .join("\n");

    combined.lines().for_each(|line| {
        let _ = parser.push_line(line);
    });
    let model = parser.finalize().expect("model");
    let suite = &model.test_results[0];
    let failed = suite
        .test_results
        .iter()
        .find(|t| t.full_name == "fail_test")
        .expect("failed test present");
    let message = failed.failure_messages.join("\n");
    assert!(message.contains("assertion `left == right` failed"));
    assert!(message.contains("left: 1"));
    assert!(message.contains("right: 2"));
    assert!(message.contains("stack backtrace:"));
    assert!(message.contains("at /repo/src/lib.rs:1:1"));
}

#[test]
fn cargo_test_stream_parser_drops_empty_suites() {
    let repo_root = Path::new("/repo");
    let mut parser = CargoTestStreamParser::new(repo_root);

    let combined = [
        "Running tests/empty_suite.rs (target/debug/deps/empty_suite-0000000000000000)",
        "running 0 tests",
        "",
        "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s",
        "",
        "Running tests/real_suite.rs (target/debug/deps/real_suite-0000000000000000)",
        "running 1 test",
        "test pass_test ... ok",
        "",
        "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s",
    ]
    .join("\n");

    combined.lines().for_each(|line| {
        let _ = parser.push_line(line);
    });

    let model = parser.finalize().expect("model");
    assert_eq!(model.test_results.len(), 1);
    assert_eq!(model.aggregated.num_total_test_suites, 1);
    assert_eq!(model.aggregated.num_passed_test_suites, 1);
    assert_eq!(model.aggregated.num_total_tests, 1);
    assert_eq!(model.aggregated.num_passed_tests, 1);
    assert_eq!(model.aggregated.num_failed_tests, 0);
}
