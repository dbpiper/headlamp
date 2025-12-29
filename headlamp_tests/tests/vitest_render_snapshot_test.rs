use headlamp::format::bridge::{
    BridgeAggregated, BridgeAssertion, BridgeConsoleEntry, BridgeFileResult, BridgeJson,
};
use headlamp::format::ctx::make_ctx;
use headlamp::format::vitest::render_vitest_from_test_model;

fn mk_assertion(
    title: &str,
    full_name: &str,
    status: &str,
    duration: u64,
    failure_messages: Vec<String>,
) -> BridgeAssertion {
    BridgeAssertion {
        title: title.to_string(),
        full_name: full_name.to_string(),
        status: status.to_string(),
        timed_out: None,
        duration,
        location: None,
        failure_messages,
        failure_details: None,
    }
}

fn mk_file_result_pass() -> BridgeFileResult {
    BridgeFileResult {
        test_file_path: "/repo/tests/pass.test.js".to_string(),
        status: "passed".to_string(),
        timed_out: None,
        failure_message: "".to_string(),
        failure_details: None,
        test_exec_error: None,
        console: None,
        test_results: vec![mk_assertion("ok", "ok", "passed", 1, vec![])],
    }
}

fn mk_http_abort_console_entry() -> BridgeConsoleEntry {
    BridgeConsoleEntry {
        message: Some(serde_json::Value::String(
            "[JEST-BRIDGE-EVENT] {type:'httpAbort', timestampMs: 10, method:'GET', url:'https://api.example.test/foo', durationMs: 1500, testPath:'/repo/tests/fail.test.js', currentTestName:'bad'}"
                .to_string(),
        )),
        type_name: Some("log".to_string()),
        origin: Some("origin".to_string()),
    }
}

fn mk_file_result_fail() -> BridgeFileResult {
    BridgeFileResult {
        test_file_path: "/repo/tests/fail.test.js".to_string(),
        status: "failed".to_string(),
        timed_out: None,
        failure_message: "suite boom".to_string(),
        failure_details: None,
        test_exec_error: None,
        console: Some(vec![
            BridgeConsoleEntry {
                message: Some(serde_json::Value::String("console error".to_string())),
                type_name: Some("error".to_string()),
                origin: Some("origin".to_string()),
            },
            mk_http_abort_console_entry(),
        ]),
        test_results: vec![mk_assertion(
            "bad",
            "bad",
            "failed",
            2,
            vec!["Expected: 1\nReceived: 2".to_string()],
        )],
    }
}

fn sample_bridge() -> BridgeJson {
    BridgeJson {
        start_time: 0,
        test_results: vec![mk_file_result_pass(), mk_file_result_fail()],
        aggregated: BridgeAggregated {
            num_total_test_suites: 2,
            num_passed_test_suites: 1,
            num_failed_test_suites: 1,
            num_total_tests: 2,
            num_passed_tests: 1,
            num_failed_tests: 1,
            num_pending_tests: 0,
            num_todo_tests: 0,
            num_timed_out_tests: None,
            num_timed_out_test_suites: None,
            start_time: 0,
            success: false,
            run_time_ms: Some(1500),
        },
    }
}

#[test]
fn render_vitest_from_bridge_snapshot() {
    let repo = std::path::PathBuf::from("/repo");
    let ctx = make_ctx(&repo, Some(80), true, false, Some("vscode".to_string()));
    let out = render_vitest_from_test_model(&sample_bridge(), &ctx, false);
    insta::assert_snapshot!("render_vitest_from_bridge_snapshot", out);
}

#[test]
fn render_vitest_ignores_empty_test_suites() {
    let repo = std::path::PathBuf::from("/repo");
    let ctx = make_ctx(&repo, Some(80), true, false, Some("vscode".to_string()));

    let mut bridge = sample_bridge();
    bridge.test_results.push(BridgeFileResult {
        test_file_path: "/repo/tests/empty.test.js".to_string(),
        status: "passed".to_string(),
        timed_out: None,
        failure_message: "".to_string(),
        failure_details: None,
        test_exec_error: None,
        console: None,
        test_results: vec![],
    });
    bridge.aggregated.num_total_test_suites = 3;

    let out = render_vitest_from_test_model(&bridge, &ctx, false);
    let simple = headlamp::format::stacks::strip_ansi_simple(&out);
    assert!(!simple.contains("empty.test.js"));
    let test_files_line = simple
        .lines()
        .find(|line| line.trim_start().starts_with("Test Files"))
        .expect("missing Test Files footer line");
    assert!(test_files_line.contains("(2)"));
}
