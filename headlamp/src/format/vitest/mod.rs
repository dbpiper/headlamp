use crate::format::bridge_console::parse_bridge_console;
use crate::format::ctx::Ctx;
use crate::format::fns::{build_file_badge_line, build_per_file_overview, render_run_line};
use crate::test_model::TestRunModel;
use once_cell::sync::Lazy;
use path_slash::PathExt;
use regex::Regex;

mod assertions;
mod console;
mod file_failure;
mod footer;

static CODE_FRAME_LINE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*(>?\s*\d+\s*\|)").unwrap());

pub fn render_vitest_from_test_model(
    data: &TestRunModel,
    ctx: &Ctx,
    only_failures: bool,
) -> String {
    let mut lines: Vec<String> = vec![];
    render_run_header(&mut lines, ctx, only_failures);
    let suites = sorted_suites(data)
        .into_iter()
        .filter(|suite| !suite.test_results.is_empty())
        .collect::<Vec<_>>();
    suites
        .iter()
        .copied()
        .for_each(|suite| render_suite(&mut lines, suite, ctx, only_failures));
    lines.extend(footer::render_footer(data, &suites, ctx, only_failures));
    lines.join("\n")
}

#[derive(Debug)]
struct SuiteRenderCtx<'a> {
    rel_path: String,
    badge_count: usize,
    has_suite_failure: bool,
    has_inline_failed_assertion: bool,
    assertions_sorted: Vec<&'a crate::test_model::TestCaseResult>,
    console_list: Vec<crate::format::console::ConsoleEntry>,
    assertion_events: Vec<crate::format::bridge_console::AssertionEvt>,
    http_sorted: Vec<crate::format::bridge_console::HttpEvent>,
}

fn render_run_header(lines: &mut Vec<String>, ctx: &Ctx, only_failures: bool) {
    if only_failures {
        return;
    }
    lines.push(render_run_line(&ctx.cwd));
    lines.push(String::new());
}

fn sorted_suites(data: &TestRunModel) -> Vec<&crate::test_model::TestSuiteResult> {
    let mut suites = data.test_results.iter().collect::<Vec<_>>();
    suites.sort_by_key(|suite| {
        std::path::Path::new(&suite.test_file_path)
            .to_slash_lossy()
            .to_string()
    });
    suites
}

fn render_suite(
    lines: &mut Vec<String>,
    suite: &crate::test_model::TestSuiteResult,
    ctx: &Ctx,
    only_failures: bool,
) {
    let suite_ctx = build_suite_render_ctx(suite, ctx, only_failures);
    maybe_render_per_file_overview(lines, suite, &suite_ctx.rel_path, only_failures);
    maybe_render_file_badge_and_console(lines, &suite_ctx, ctx, only_failures);
    maybe_render_inline_failed_assertion(
        lines,
        suite,
        &suite_ctx.assertions_sorted,
        ctx,
        only_failures,
    );
    if suite_ctx.has_suite_failure {
        lines.extend(file_failure::render_file_level_failure(
            suite,
            ctx,
            &suite_ctx.console_list,
        ));
    }
    suite_ctx
        .assertions_sorted
        .iter()
        .filter(|assertion| assertion.status == "failed")
        .for_each(|assertion| {
            lines.extend(assertions::render_failed_assertion(
                &suite_ctx.rel_path,
                suite,
                assertion,
                ctx,
                &suite_ctx.console_list,
                &suite_ctx.assertion_events,
                &suite_ctx.http_sorted,
            ));
        });
}

fn build_suite_render_ctx<'a>(
    suite: &'a crate::test_model::TestSuiteResult,
    ctx: &Ctx,
    only_failures: bool,
) -> SuiteRenderCtx<'a> {
    let rel_path = relativize_suite_path(&suite.test_file_path, ctx);
    let failed_count = suite
        .test_results
        .iter()
        .filter(|test| test.status == "failed")
        .count();
    let has_suite_failure = has_suite_failure(suite, failed_count);
    let badge_count = if has_suite_failure {
        failed_count.max(1)
    } else {
        failed_count
    };
    let (http, assertion_events, console_list) = parse_bridge_console(suite.console.as_ref());
    let console_list = match (only_failures, badge_count > 0) {
        (true, true) => console::filter_console_to_failed_tests(suite, console_list),
        _ => console_list,
    };
    let mut http_sorted = http;
    http_sorted.sort_by_key(|evt| evt.timestamp_ms);
    let mut assertions_sorted = suite.test_results.iter().collect::<Vec<_>>();
    assertions_sorted.sort_by(|a, b| a.full_name.cmp(&b.full_name));
    let has_inline_failed_assertion =
        !only_failures && assertions_sorted.iter().any(|a| a.status == "failed");
    SuiteRenderCtx {
        rel_path,
        badge_count,
        has_suite_failure,
        has_inline_failed_assertion,
        assertions_sorted,
        console_list,
        assertion_events,
        http_sorted,
    }
}

fn relativize_suite_path(abs_or_rel: &str, ctx: &Ctx) -> String {
    let file_path = std::path::Path::new(abs_or_rel)
        .to_slash_lossy()
        .to_string();
    file_path
        .strip_prefix(&format!("{}/", ctx.cwd))
        .unwrap_or(file_path.as_str())
        .to_string()
}

fn has_suite_failure(suite: &crate::test_model::TestSuiteResult, failed_assertions: usize) -> bool {
    (failed_assertions == 0)
        && (!suite.failure_message.trim().is_empty() || suite.status == "failed")
}

fn maybe_render_per_file_overview(
    lines: &mut Vec<String>,
    suite: &crate::test_model::TestSuiteResult,
    rel: &str,
    only_failures: bool,
) {
    if only_failures {
        return;
    }
    let assertions = suite
        .test_results
        .iter()
        .map(|a| (a.full_name.clone(), a.status.clone()))
        .collect::<Vec<_>>();
    lines.extend(build_per_file_overview(rel, &assertions));
}

fn maybe_render_file_badge_and_console(
    lines: &mut Vec<String>,
    suite_ctx: &SuiteRenderCtx<'_>,
    ctx: &Ctx,
    only_failures: bool,
) {
    if only_failures && suite_ctx.badge_count == 0 {
        return;
    }
    lines.push(build_file_badge_line(
        &suite_ctx.rel_path,
        suite_ctx.badge_count,
    ));
    if suite_ctx.badge_count > 0 && !suite_ctx.has_inline_failed_assertion {
        lines.push(String::new());
    }
    if only_failures && suite_ctx.badge_count > 0 {
        lines.extend(crate::format::console::build_console_section(
            &suite_ctx.console_list,
            ctx.show_logs,
        ));
    }
}

fn maybe_render_inline_failed_assertion(
    lines: &mut Vec<String>,
    suite: &crate::test_model::TestSuiteResult,
    assertions_sorted: &[&crate::test_model::TestCaseResult],
    ctx: &Ctx,
    only_failures: bool,
) {
    if only_failures {
        return;
    }
    if let Some(first_failed) = assertions_sorted.iter().find(|a| a.status == "failed") {
        lines.extend(assertions::render_inline_failed_assertion_block(
            suite,
            first_failed,
            ctx,
        ));
    }
}
