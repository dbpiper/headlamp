use headlamp::live_progress::{
    LiveProgressMode, RenderRunFrameArgs, frame_physical_line_count,
    live_progress_mode_with_env_ci, render_run_frame, render_run_frame_with_columns,
};

#[test]
fn live_progress_render_run_frame_includes_clear_and_run_prefix() {
    let frame = render_run_frame(
        "jest.config.js",
        2,
        5,
        3,
        12,
        7,
        "stdout: Compiling headlamp v0.1.0",
    );
    assert!(frame.starts_with("RUN "));
    assert!(frame.contains("(2/5)"));
    assert!(frame.contains("jest.config.js"));
    assert!(frame.contains("+12s"));
    assert!(frame.contains("Compiling"));
    // We render activity on a second line (or wrapped line) rather than squeezing into one.
    assert!(frame.contains('\n'));
}

#[test]
fn live_progress_long_details_wraps_to_multiple_physical_lines() {
    let long_label = "/Users/david/src/headlamp/headlamp_parity_tests/tests/parity_suite_test.rs";
    let long_recent =
        "stderr: Finished `test` profile [unoptimized + debuginfo] target(s) in 123.45s";

    let frame_a = render_run_frame_with_columns(RenderRunFrameArgs {
        current_label: long_label,
        done_units: 0,
        total_units: 1,
        spinner_index: 0,
        elapsed_seconds: 199,
        idle_seconds: 0,
        recent: long_recent,
        columns: 64,
    });
    assert!(frame_a.starts_with("RUN ["));
    assert!(frame_a.contains("stderr:"));
    assert!(frame_a.contains('\n'));
}

#[test]
fn live_progress_frame_physical_line_count_counts_wrapped_lines_even_without_newlines() {
    let columns = 20;
    let long_single_line = "x".repeat(60);
    assert!(!long_single_line.contains('\n'));
    assert_eq!(frame_physical_line_count(&long_single_line, columns), 3);
}

#[test]
fn live_progress_enable_gate_follows_tty_flag() {
    assert_eq!(
        live_progress_mode_with_env_ci(false, false, false, false),
        LiveProgressMode::Plain
    );
    assert_eq!(
        live_progress_mode_with_env_ci(true, false, false, false),
        LiveProgressMode::Interactive
    );
}

#[test]
fn live_progress_disabled_in_ci_mode() {
    assert_eq!(
        live_progress_mode_with_env_ci(true, true, false, false),
        LiveProgressMode::Plain
    );
}

#[test]
fn live_progress_classifies_nextest_suite_json_lines() {
    let line = r#"{"type":"suite","event":"ok","passed":1,"failed":0,"ignored":0,"measured":0,"filtered_out":0,"exec_time":0.11518151,"nextest":{"crate":"headlamp_tests","test_binary":"vitest_render_snapshot_test","kind":"test"}}"#;
    let hint = headlamp::live_progress::classify_runner_line_for_progress(line).expect("hint");
    assert!(hint.contains("suite ok:"));
    assert!(hint.contains("headlamp_tests::vitest_render_snapshot_test"));
}
