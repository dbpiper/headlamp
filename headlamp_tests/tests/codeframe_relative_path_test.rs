use headlamp::format::ctx::make_ctx;
use headlamp::format::stacks::strip_ansi_simple;
use headlamp::format::vitest::render_vitest_from_test_model;
use headlamp::test_model::{TestCaseResult, TestRunAggregated, TestRunModel, TestSuiteResult};

fn mk_temp_repo_dir(name: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir()
        .join("headlamp-tests")
        .join("codeframe-relative-path")
        .join(name);
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    base
}

fn write_numbered_lines(path: &std::path::Path, line_count: usize, marker_line: usize) {
    let content = (1..=line_count)
        .map(|line_number| {
            if line_number == marker_line {
                format!("const MARKER_LINE_{marker_line} = true;")
            } else {
                format!("const LINE_{line_number} = true;")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

#[test]
fn renders_code_frame_when_stack_location_is_relative_to_repo_root() {
    let repo_root = mk_temp_repo_dir("renders_code_frame_when_stack_location_is_relative");
    let failing_file = repo_root
        .join("headlamp_tests")
        .join("tests")
        .join("args_parse_test.rs");
    write_numbered_lines(&failing_file, 220, 189);
    let wrong_suite_path = repo_root.join("tests").join("args_parse_test.rs");

    let failure_message = [
        "thread 'derive_args_does_not_consume_selection_path_as_boolean_value' panicked at tests/args_parse_test.rs:189:5:",
        "assertion failed: parsed.selection_specified",
        "stack backtrace:",
        "   0: rust_begin_unwind",
        "             at ./tests/args_parse_test.rs:189:5",
    ]
    .join("\n");

    let model = TestRunModel {
        start_time: 0,
        test_results: vec![TestSuiteResult {
            test_file_path: wrong_suite_path.to_string_lossy().to_string(),
            status: "failed".to_string(),
            timed_out: None,
            failure_message: String::new(),
            failure_details: None,
            test_exec_error: None,
            console: None,
            test_results: vec![TestCaseResult {
                title: "derive_args_does_not_consume_selection_path_as_boolean_value".to_string(),
                full_name: "derive_args_does_not_consume_selection_path_as_boolean_value"
                    .to_string(),
                status: "failed".to_string(),
                timed_out: None,
                duration: 1,
                location: None,
                failure_messages: vec![failure_message],
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
            run_time_ms: Some(1),
        },
    };

    let ctx = make_ctx(repo_root.as_path(), Some(120), true, false, None);
    let rendered = render_vitest_from_test_model(&model, &ctx, false);
    let plain = strip_ansi_simple(&rendered);

    assert!(
        plain.contains("MARKER_LINE_189"),
        "rendered output did not include code frame line 189.\n\nRendered:\n{plain}"
    );
}
