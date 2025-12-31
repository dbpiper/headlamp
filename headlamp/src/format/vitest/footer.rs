use crate::format::ansi;
use crate::format::colors;
use crate::format::ctx::Ctx;
use crate::format::fns::draw_rule;
use crate::format::stacks;
use crate::format::time::format_duration;
use crate::test_model::{TestRunAggregated, TestRunModel};

pub(super) fn render_footer(
    data: &TestRunModel,
    suites: &[&crate::test_model::TestSuiteResult],
    ctx: &Ctx,
    only_failures: bool,
) -> Vec<String> {
    let filtered_agg = aggregated_from_suites(suites, data.aggregated.run_time_ms, data.start_time);
    let failed_count =
        if filtered_agg.num_total_tests == 0 && filtered_agg.num_failed_test_suites > 0 {
            filtered_agg.num_failed_test_suites
        } else {
            filtered_agg.num_failed_tests
        };
    let timed_out_count = filtered_agg.num_timed_out_tests.unwrap_or(0);
    let footer = vitest_footer(&filtered_agg, only_failures);

    let mut out: Vec<String> = vec![
        draw_rule(
            ctx.width,
            Some(&colors::bg_failure(&ansi::white(&format!(
                " Failed Tests {failed_count} "
            )))),
        ),
        String::new(),
        footer,
    ];

    if timed_out_count > 0 {
        out.push(String::new());
        out.push(draw_rule(
            ctx.width,
            Some(&colors::bg_failure(&ansi::white(&format!(
                " Timed Out {timed_out_count} "
            )))),
        ));
    }
    out
}

fn aggregated_from_suites(
    suites: &[&crate::test_model::TestSuiteResult],
    run_time_ms: Option<u64>,
    start_time: u64,
) -> TestRunAggregated {
    let (failed_tests, passed_tests, pending_tests, todo_tests, timed_out_tests) = suites
        .iter()
        .flat_map(|suite| suite.test_results.iter())
        .fold((0u64, 0u64, 0u64, 0u64, 0u64), |acc, test| {
            let (failed, passed, pending, todo, timed_out) = acc;
            let next_failed = failed.saturating_add((test.status == "failed") as u64);
            let next_passed = passed.saturating_add((test.status == "passed") as u64);
            let next_pending = pending.saturating_add((test.status == "pending") as u64);
            let next_todo = todo.saturating_add((test.status == "todo") as u64);
            let next_timed_out = timed_out.saturating_add((test.timed_out.unwrap_or(false)) as u64);
            (
                next_failed,
                next_passed,
                next_pending,
                next_todo,
                next_timed_out,
            )
        });
    let total_tests = suites
        .iter()
        .map(|suite| suite.test_results.len() as u64)
        .sum::<u64>();
    let failed_suites = suites
        .iter()
        .filter(|suite| {
            suite.status == "failed" || suite.test_results.iter().any(|t| t.status == "failed")
        })
        .count() as u64;
    let total_suites = suites.len() as u64;
    let passed_suites = total_suites.saturating_sub(failed_suites);
    let timed_out_suites = suites
        .iter()
        .filter(|suite| suite.timed_out.unwrap_or(false))
        .count() as u64;

    TestRunAggregated {
        num_total_test_suites: total_suites,
        num_passed_test_suites: passed_suites,
        num_failed_test_suites: failed_suites,
        num_total_tests: total_tests,
        num_passed_tests: passed_tests,
        num_failed_tests: failed_tests,
        num_pending_tests: pending_tests,
        num_todo_tests: todo_tests,
        num_timed_out_tests: Some(timed_out_tests),
        num_timed_out_test_suites: Some(timed_out_suites),
        start_time,
        success: failed_tests == 0 && failed_suites == 0,
        run_time_ms,
    }
}

fn vitest_footer(agg: &TestRunAggregated, only_failures: bool) -> String {
    let _ = only_failures;

    let files = vec![
        (agg.num_failed_test_suites > 0)
            .then(|| colors::failure(&format!("{} failed", agg.num_failed_test_suites))),
        (agg.num_passed_test_suites > 0)
            .then(|| colors::success(&format!("{} passed", agg.num_passed_test_suites))),
        (agg.num_pending_tests > 0)
            .then(|| colors::skip(&format!("{} skipped", agg.num_pending_tests))),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(&ansi::dim(" | "));

    let tests = vec![
        (agg.num_failed_tests > 0)
            .then(|| colors::failure(&format!("{} failed", agg.num_failed_tests))),
        (agg.num_passed_tests > 0)
            .then(|| colors::success(&format!("{} passed", agg.num_passed_tests))),
        (agg.num_pending_tests > 0)
            .then(|| colors::skip(&format!("{} skipped", agg.num_pending_tests))),
        (agg.num_todo_tests > 0).then(|| colors::todo(&format!("{} todo", agg.num_todo_tests))),
        agg.num_timed_out_tests
            .filter(|n| *n > 0)
            .map(|n| colors::failure(&format!("{n} timed out"))),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(&ansi::dim(" | "));

    let time_ms = agg.run_time_ms.unwrap_or_else(|| {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(agg.start_time);
        now_ms.saturating_sub(agg.start_time)
    });
    let time = format_duration(std::time::Duration::from_millis(time_ms));

    let footer = [
        format!(
            "{} {} {}",
            ansi::bold("Test Files"),
            files,
            ansi::dim(&format!("({})", agg.num_total_test_suites))
        ),
        format!(
            "{}     {} {}",
            ansi::bold("Tests"),
            tests,
            ansi::dim(&format!("({})", agg.num_total_tests))
        ),
        format!("{}      {}", ansi::bold("Time"), time),
    ]
    .join("\n");

    drop_blank_line_before_time_line(&footer)
}

fn drop_blank_line_before_time_line(text: &str) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    let kept = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            if !line.trim().is_empty() {
                return Some(*line);
            }
            let Some(next_line) = lines.get(index + 1) else {
                return Some(*line);
            };
            let next_is_time_line = stacks::strip_ansi_simple(next_line)
                .trim_start()
                .starts_with("Time ");
            (!next_is_time_line).then_some(*line)
        })
        .collect::<Vec<_>>();
    kept.join("\n")
}
