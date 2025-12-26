use headlamp::format::bridge::{
    BridgeAggregated, BridgeAssertion, BridgeConsoleEntry, BridgeFileResult, BridgeJson,
};
use headlamp::format::ctx::make_ctx;
use headlamp::format::vitest::render_vitest_from_test_model;

fn sample_bridge() -> BridgeJson {
    BridgeJson {
        start_time: 0,
        test_results: vec![
            BridgeFileResult {
                test_file_path: "/repo/tests/pass.test.js".to_string(),
                status: "passed".to_string(),
                timed_out: None,
                failure_message: "".to_string(),
                failure_details: None,
                test_exec_error: None,
                console: None,
                test_results: vec![BridgeAssertion {
                    title: "ok".to_string(),
                    full_name: "ok".to_string(),
                    status: "passed".to_string(),
                    timed_out: None,
                    duration: 1,
                    location: None,
                    failure_messages: vec![],
                    failure_details: None,
                }],
            },
            BridgeFileResult {
                test_file_path: "/repo/tests/fail.test.js".to_string(),
                status: "failed".to_string(),
                timed_out: None,
                failure_message: "suite boom".to_string(),
                failure_details: None,
                test_exec_error: None,
                console: Some(vec![BridgeConsoleEntry {
                    message: Some(serde_json::Value::String("console error".to_string())),
                    type_name: Some("error".to_string()),
                    origin: Some("origin".to_string()),
                }]),
                test_results: vec![BridgeAssertion {
                    title: "bad".to_string(),
                    full_name: "bad".to_string(),
                    status: "failed".to_string(),
                    timed_out: None,
                    duration: 2,
                    location: None,
                    failure_messages: vec!["Expected: 1\nReceived: 2".to_string()],
                    failure_details: None,
                }],
            },
        ],
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
            run_time_ms: Some(3),
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
