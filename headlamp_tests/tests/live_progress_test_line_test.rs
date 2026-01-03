use std::time::Duration;

use headlamp::live_progress::{TestOutcome, render_finished_test_line};

#[test]
fn live_progress_finished_test_line_formats_subsecond_durations() {
    let line = render_finished_test_line(
        TestOutcome::Pass,
        Some(Duration::from_micros(400)),
        "suite/path",
        "test_name",
    );
    assert!(
        line.contains("[400µs]"),
        "expected smart subsecond format, got: {line}"
    );
}

#[test]
fn live_progress_finished_test_line_formats_multi_unit_durations() {
    let line = render_finished_test_line(
        TestOutcome::Pass,
        Some(Duration::from_millis(1_200)),
        "suite/path",
        "test_name",
    );
    assert!(
        line.contains("[1s 200ms]"),
        "expected multi-unit duration, got: {line}"
    );
}

#[test]
fn live_progress_finished_test_line_keeps_missing_duration_placeholder() {
    let line = render_finished_test_line(TestOutcome::Pass, None, "suite/path", "test_name");
    assert!(
        line.contains("[--]"),
        "expected missing duration placeholder, got: {line}"
    );
}
