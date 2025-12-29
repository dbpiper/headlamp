use crate::format::ansi;
use crate::format::colors;
use crate::format::ctx::Ctx;
use crate::format::fns::draw_rule;
use crate::test_model::{TestRunAggregated, TestRunModel};

pub(super) fn render_footer(data: &TestRunModel, ctx: &Ctx, only_failures: bool) -> Vec<String> {
    let failed_count =
        if data.aggregated.num_total_tests == 0 && data.aggregated.num_failed_test_suites > 0 {
            data.aggregated.num_failed_test_suites
        } else {
            data.aggregated.num_failed_tests
        };
    let timed_out_count = data.aggregated.num_timed_out_tests.unwrap_or(0);
    let footer = if data.test_results.len() as u64 != data.aggregated.num_total_test_suites {
        vitest_footer_from_files(&data.test_results, &data.aggregated)
    } else {
        vitest_footer(&data.aggregated, only_failures)
    };

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

fn vitest_footer_from_files(
    files: &[crate::test_model::TestSuiteResult],
    agg: &TestRunAggregated,
) -> String {
    let file_failed = |file: &crate::test_model::TestSuiteResult| -> bool {
        file.status == "failed" || file.test_results.iter().any(|a| a.status == "failed")
    };
    let failed_files = files.iter().filter(|f| file_failed(f)).count() as u64;
    let total_files = files.len() as u64;

    let total_tests = files
        .iter()
        .map(|f| f.test_results.len() as u64)
        .sum::<u64>();
    let failed_tests = if total_tests == 0 {
        failed_files
    } else {
        files
            .iter()
            .flat_map(|f| f.test_results.iter())
            .filter(|a| a.status == "failed")
            .count() as u64
    };

    let time = agg
        .run_time_ms
        .map(|ms| format!("{ms}ms"))
        .unwrap_or_default();

    [
        format!(
            "{} {} {}",
            ansi::bold("Test Files"),
            colors::failure(&format!("{failed_files} failed")),
            ansi::dim(&format!("({total_files})"))
        ),
        format!(
            "{}     {} {}",
            ansi::bold("Tests"),
            colors::failure(&format!("{failed_tests} failed")),
            ansi::dim(&format!("({total_tests})"))
        ),
        format!("{}      {}", ansi::bold("Time"), time),
    ]
    .join("\n")
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

    let time = agg
        .run_time_ms
        .map(|ms| format!("{ms}ms"))
        .unwrap_or_default();

    [
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
    .join("\n")
}
