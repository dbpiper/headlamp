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

    if !only_failures {
        lines.push(render_run_line(&ctx.cwd));
        lines.push(String::new());
    }

    let mut suites = data.test_results.iter().collect::<Vec<_>>();
    suites.sort_by(|a, b| {
        let ap = std::path::Path::new(&a.test_file_path)
            .to_slash_lossy()
            .to_string();
        let bp = std::path::Path::new(&b.test_file_path)
            .to_slash_lossy()
            .to_string();
        ap.cmp(&bp)
    });

    for file in suites {
        let file_path = std::path::Path::new(&file.test_file_path)
            .to_slash_lossy()
            .to_string();
        let rel = file_path
            .strip_prefix(&format!("{}/", ctx.cwd))
            .unwrap_or(file_path.as_str())
            .to_string();
        let failed = file
            .test_results
            .iter()
            .filter(|a| a.status == "failed")
            .count();
        let has_assertion_failures = failed > 0;
        let has_suite_failure = !has_assertion_failures
            && (!file.failure_message.trim().is_empty() || file.status == "failed");
        let badge_count = if has_suite_failure {
            failed.max(1)
        } else {
            failed
        };
        let (http, assertion_events, console_list) = parse_bridge_console(file.console.as_ref());
        let console_list = if only_failures && badge_count > 0 {
            console::filter_console_to_failed_tests(file, console_list)
        } else {
            console_list
        };
        let mut http_sorted = http;
        http_sorted.sort_by_key(|evt| evt.timestamp_ms);

        let mut assertions_sorted = file.test_results.iter().collect::<Vec<_>>();
        assertions_sorted.sort_by(|a, b| a.full_name.cmp(&b.full_name));

        if !only_failures {
            let assertions = assertions_sorted
                .iter()
                .map(|a| (a.full_name.clone(), a.status.clone()))
                .collect::<Vec<_>>();
            lines.extend(build_per_file_overview(&rel, &assertions));
        }
        let has_inline_failed_assertion = !only_failures
            && assertions_sorted
                .iter()
                .any(|assertion| assertion.status == "failed");
        if !(only_failures && badge_count == 0) {
            lines.push(build_file_badge_line(&rel, badge_count));
            if badge_count > 0 && !has_inline_failed_assertion {
                lines.push(String::new());
            }
            if only_failures && badge_count > 0 {
                lines.extend(crate::format::console::build_console_section(
                    &console_list,
                    ctx.show_logs,
                ));
            }
        }

        if !only_failures {
            let failed_assertions = assertions_sorted.iter().filter(|a| a.status == "failed");
            if let Some(first_failed) = failed_assertions.clone().next() {
                lines.extend(assertions::render_inline_failed_assertion_block(
                    file,
                    first_failed,
                    ctx,
                ));
            }
        }

        if has_suite_failure {
            lines.extend(file_failure::render_file_level_failure(
                file,
                ctx,
                &console_list,
            ));
        }

        for assertion in assertions_sorted.iter().filter(|a| a.status == "failed") {
            lines.extend(assertions::render_failed_assertion(
                &rel,
                file,
                assertion,
                ctx,
                &console_list,
                &assertion_events,
                &http_sorted,
            ));
        }
    }

    lines.extend(footer::render_footer(data, ctx, only_failures));
    lines.join("\n")
}
