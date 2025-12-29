use std::path::Path;

use headlamp::format::bridge::{BridgeAggregated, BridgeAssertion, BridgeFileResult, BridgeJson};
use headlamp::format::ctx::make_ctx;
use headlamp::format::vitest::render_vitest_from_test_model;

fn mk_bridge_console_event(
    level: &str,
    message: &str,
    test_path: &str,
    test_name: &str,
) -> serde_json::Value {
    serde_json::json!({
        "type": "console",
        "timestampMs": 1,
        "level": level,
        "message": message,
        "testPath": test_path,
        "currentTestName": test_name
    })
}

fn build_console_entries_for_two_tests(
    test_file_path: &str,
    pass_name: &str,
    fail_name: &str,
) -> Vec<headlamp::format::bridge::BridgeConsoleEntry> {
    vec![
        headlamp::format::bridge::BridgeConsoleEntry {
            type_name: Some("log".to_string()),
            message: Some(serde_json::Value::String(format!(
                "[JEST-BRIDGE-EVENT] {}",
                mk_bridge_console_event("log", "log-pass", test_file_path, pass_name)
            ))),
            origin: None,
        },
        headlamp::format::bridge::BridgeConsoleEntry {
            type_name: Some("error".to_string()),
            message: Some(serde_json::Value::String(format!(
                "[JEST-BRIDGE-EVENT] {}",
                mk_bridge_console_event("error", "err-fail", test_file_path, fail_name)
            ))),
            origin: None,
        },
    ]
}

fn build_file_result_for_two_tests(
    test_file_path: &str,
    pass_name: &str,
    fail_name: &str,
    console_entries: Vec<headlamp::format::bridge::BridgeConsoleEntry>,
) -> BridgeFileResult {
    BridgeFileResult {
        test_file_path: test_file_path.to_string(),
        status: "failed".to_string(),
        timed_out: None,
        failure_message: String::new(),
        failure_details: None,
        test_exec_error: None,
        console: Some(console_entries),
        test_results: vec![
            BridgeAssertion {
                title: "pass".to_string(),
                full_name: pass_name.to_string(),
                status: "passed".to_string(),
                timed_out: None,
                duration: 1,
                location: None,
                failure_messages: vec![],
                failure_details: None,
            },
            BridgeAssertion {
                title: "fail".to_string(),
                full_name: fail_name.to_string(),
                status: "failed".to_string(),
                timed_out: None,
                duration: 1,
                location: None,
                failure_messages: vec!["Error: boom".to_string()],
                failure_details: None,
            },
        ],
    }
}

fn aggregated_for_one_failed_suite_with_two_tests() -> BridgeAggregated {
    BridgeAggregated {
        num_total_test_suites: 1,
        num_passed_test_suites: 0,
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
        run_time_ms: Some(1),
    }
}

fn build_bridge_with_single_file(file: BridgeFileResult) -> BridgeJson {
    BridgeJson {
        start_time: 0,
        test_results: vec![file],
        aggregated: aggregated_for_one_failed_suite_with_two_tests(),
    }
}

#[test]
fn vitest_renderer_filters_logs_to_current_failed_test_when_possible() {
    let cwd = Path::new("/repo");
    let ctx = make_ctx(cwd, Some(100), true, true, None);

    let test_file_path = "/repo/tests/mixed.test.js";
    let pass_name = "pass";
    let fail_name = "fail";

    let console_entries = build_console_entries_for_two_tests(test_file_path, pass_name, fail_name);
    let file =
        build_file_result_for_two_tests(test_file_path, pass_name, fail_name, console_entries);
    let bridge = build_bridge_with_single_file(file);

    let rendered = render_vitest_from_test_model(&bridge, &ctx, true);
    assert!(rendered.contains("Logs:"));
    assert!(rendered.contains("err-fail"));
    assert!(!rendered.contains("log-pass"));
}
