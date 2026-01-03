use std::time::Duration;

use crate::format::time::format_duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestOutcome {
    Pass,
    Fail,
    Skip,
    Unknown,
}

pub fn render_finished_test_line(
    outcome: TestOutcome,
    duration: Option<Duration>,
    suite_path: &str,
    test_name: &str,
) -> String {
    let mut out = String::with_capacity(64 + suite_path.len() + test_name.len());
    out.push_str(outcome_label(outcome));
    out.push(' ');
    out.push('[');
    if let Some(duration) = duration {
        out.push_str(format_duration(duration).as_str());
    } else {
        out.push_str("--");
    }
    out.push(']');
    out.push(' ');
    out.push_str(suite_path);
    if !test_name.trim().is_empty() {
        out.push_str("::");
        out.push_str(test_name);
    }
    out
}

pub fn outcome_from_status(status: &str) -> TestOutcome {
    match status.trim().to_ascii_lowercase().as_str() {
        "passed" | "ok" | "pass" => TestOutcome::Pass,
        "failed" | "fail" => TestOutcome::Fail,
        "ignored" | "skipped" => TestOutcome::Skip,
        "pending" => TestOutcome::Skip,
        _ => TestOutcome::Unknown,
    }
}

fn outcome_label(outcome: TestOutcome) -> &'static str {
    match outcome {
        TestOutcome::Pass => "PASS",
        TestOutcome::Fail => "FAIL",
        TestOutcome::Skip => "SKIP",
        TestOutcome::Unknown => "DONE",
    }
}
