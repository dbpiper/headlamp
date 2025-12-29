use crate::format::ansi;
use crate::format::bridge_console::parse_bridge_console;
use crate::format::bridge_http::render_http_card;
use crate::format::codeframe::{Loc, build_code_frame_section, find_code_frame_start};
use crate::format::colors;
use crate::format::console::build_console_section;
use crate::format::ctx::Ctx;
use crate::format::details::{lines_from_details, merge_msg_lines};
use crate::format::fns::{
    build_file_badge_line, build_per_file_overview, deepest_project_loc, draw_fail_line, draw_rule,
    render_run_line,
};
use crate::format::paths::preferred_editor_href;
use crate::test_model::{TestRunAggregated, TestRunModel};
use once_cell::sync::Lazy;
use path_slash::PathExt;
use regex::Regex;

static CODE_FRAME_LINE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*(>?\s*\d+\s*\|)").unwrap());

fn filter_console_to_failed_tests(
    file: &crate::test_model::TestSuiteResult,
    console_entries: Vec<crate::format::console::ConsoleEntry>,
) -> Vec<crate::format::console::ConsoleEntry> {
    let failed_names = file
        .test_results
        .iter()
        .filter(|a| a.status == "failed")
        .map(|a| a.full_name.as_str())
        .collect::<Vec<_>>();
    if failed_names.is_empty() {
        return console_entries;
    }
    let matches_failed = |e: &crate::format::console::ConsoleEntry| -> bool {
        e.current_test_name
            .as_deref()
            .is_some_and(|n| failed_names.contains(&n))
    };
    if !console_entries.iter().any(matches_failed) {
        return console_entries;
    }
    console_entries
        .into_iter()
        .filter(matches_failed)
        .collect::<Vec<_>>()
}

fn extract_expected_received_values(messages_array: &[String]) -> (Option<String>, Option<String>) {
    let stripped = messages_array
        .iter()
        .map(|line| crate::format::stacks::strip_ansi_simple(line))
        .collect::<Vec<_>>();
    let expected = stripped.iter().find_map(|line| {
        line.strip_prefix("Expected: ")
            .map(|v| v.trim().to_string())
    });
    let received = stripped.iter().find_map(|line| {
        line.strip_prefix("Received: ")
            .map(|v| v.trim().to_string())
    });
    if expected.is_some() || received.is_some() {
        return (expected, received);
    }

    let left = stripped.iter().find_map(|line| {
        line.trim_start()
            .strip_prefix("left: ")
            .map(|v| v.trim().to_string())
    });
    let right = stripped.iter().find_map(|line| {
        line.trim_start()
            .strip_prefix("right: ")
            .map(|v| v.trim().to_string())
    });
    (right, left)
}

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
            filter_console_to_failed_tests(file, console_list)
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
                lines.extend(build_console_section(&console_list, ctx.show_logs));
            }
        }

        if !only_failures {
            let failed_assertions = assertions_sorted.iter().filter(|a| a.status == "failed");
            if let Some(first_failed) = failed_assertions.clone().next() {
                lines.extend(render_inline_failed_assertion_block(
                    file,
                    first_failed,
                    ctx,
                ));
            }
        }

        if has_suite_failure {
            lines.extend(render_file_level_failure(file, ctx, &console_list));
        }

        for assertion in assertions_sorted.iter().filter(|a| a.status == "failed") {
            lines.extend(render_failed_assertion(
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

    lines.extend(render_footer(data, ctx, only_failures));
    lines.join("\n")
}

fn render_inline_failed_assertion_block(
    file: &crate::test_model::TestSuiteResult,
    assertion: &crate::test_model::TestCaseResult,
    ctx: &Ctx,
) -> Vec<String> {
    let primary_block = if !assertion.failure_messages.is_empty() {
        assertion.failure_messages.join("\n")
    } else {
        file.failure_message.clone()
    };
    let (stacks, detail_msgs) = lines_from_details(
        assertion
            .failure_details
            .as_ref()
            .or(file.failure_details.as_ref()),
    );
    let file_stacks = file
        .failure_message
        .lines()
        .filter(|line| {
            crate::format::stacks::is_stack_line(&crate::format::stacks::strip_ansi_simple(line))
        })
        .map(|line| crate::format::fns::color_stack_line(line, &ctx.project_hint))
        .collect::<Vec<_>>();
    let stacks_for_render = if stacks.is_empty() {
        file_stacks
    } else {
        stacks.clone()
    };
    let messages_array = merge_msg_lines(&primary_block, &detail_msgs);

    let mut messages_for_code_frame = messages_array.clone();
    let file_failure_lines = file
        .failure_message
        .lines()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    if let Some(start) = find_code_frame_start(&file_failure_lines) {
        let frame_block = file_failure_lines
            .iter()
            .skip(start)
            .take_while(|line| !line.trim().is_empty())
            .cloned()
            .collect::<Vec<_>>();
        if !frame_block.is_empty() {
            messages_for_code_frame.extend(frame_block);
        }
    }
    let stack_loc = messages_array
        .iter()
        .chain(stacks.iter())
        .rev()
        .find_map(|line| crate::format::fns::parse_stack_location(line));
    let (line, column) = match assertion.location.as_ref() {
        Some(loc) if loc.line > 0 && loc.column > 0 => (loc.line, Some(loc.column)),
        Some(loc) if loc.line > 0 => (loc.line, stack_loc.as_ref().map(|(_, _, c)| *c)),
        _ => (
            stack_loc.as_ref().map(|(_, l, _)| *l).unwrap_or(0),
            stack_loc.as_ref().map(|(_, _, c)| *c),
        ),
    };
    let synth_loc = (line > 0).then(|| Loc {
        file: file.test_file_path.clone(),
        line,
        column,
    });
    render_ts_style_assertion_failure(
        &messages_for_code_frame,
        &stacks_for_render,
        ctx,
        synth_loc.as_ref(),
    )
}

fn render_ts_style_assertion_failure(
    messages_array: &[String],
    stacks: &[String],
    ctx: &Ctx,
    synth_loc: Option<&Loc>,
) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    out.extend(build_code_frame_section(
        &messages_array
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        ctx.show_stacks,
        synth_loc,
    ));
    out.extend(render_expected_received_sections(messages_array));
    out.extend(render_assertion_block(messages_array));
    if ctx.show_stacks && !stacks.is_empty() {
        out.push(ansi::dim("    Stack:"));
        stacks
            .iter()
            .take(6)
            .for_each(|ln| out.push(format!("            {}", ln.trim_start())));
        out.push(String::new());
    }
    out
}

fn render_expected_received_sections(messages_array: &[String]) -> Vec<String> {
    let (expected, received) = extract_expected_received_values(messages_array);
    if expected.is_none() && received.is_none() {
        return vec![];
    }
    let mut out: Vec<String> = vec![format!("    {}", ansi::bold("Expected"))];
    if let Some(v) = expected {
        out.push(format!("      {}", colors::success(&v)));
    }
    out.push(format!("    {}", ansi::bold("Received")));
    if let Some(v) = received {
        out.push(format!("      {}", colors::failure(&v)));
    }
    out.push(String::new());
    out
}

fn render_expected_received_sections_compact(messages_array: &[String]) -> Vec<String> {
    let (expected, received) = extract_expected_received_values(messages_array);
    if expected.is_none() && received.is_none() {
        return vec![];
    }
    let mut out: Vec<String> = vec![format!("    {}", ansi::bold("Expected"))];
    if let Some(v) = expected {
        out.push(format!("      {}", colors::success(&v)));
    }
    out.push(format!("    {}", ansi::bold("Received")));
    if let Some(v) = received {
        out.push(format!("      {}", colors::failure(&v)));
    }
    out
}

fn render_assertion_block(messages_array: &[String]) -> Vec<String> {
    let stripped = messages_array
        .iter()
        .map(|ln| crate::format::stacks::strip_ansi_simple(ln))
        .collect::<Vec<_>>();
    let start = stripped
        .iter()
        .position(|ln| ln.trim_start().starts_with("expect("))
        .unwrap_or(usize::MAX);
    if start == usize::MAX {
        return vec![];
    }
    let mut out: Vec<String> = vec![format!("    {}", ansi::bold("Assertion:"))];
    let (expected, received) = extract_expected_received_values(messages_array);
    stripped
        .iter()
        .skip(start)
        .take_while(|ln| !ln.trim_start().starts_with("at "))
        .map(|line| line.to_string())
        .filter(|line| {
            let simple = crate::format::stacks::strip_ansi_simple(line);
            let trimmed = simple.trim();
            !(trimmed.is_empty()
                || CODE_FRAME_LINE_RE.is_match(trimmed)
                || (trimmed.starts_with('|') && trimmed.contains('^')))
        })
        .for_each(|line| out.push(format!("    {}", ansi::yellow(&format!("    {line}")))));
    if let Some(v) = expected {
        out.push(format!(
            "    {}",
            ansi::yellow(&format!("    Expected: {v}"))
        ));
    }
    if let Some(v) = received {
        out.push(format!(
            "    {}",
            ansi::yellow(&format!("    Received: {v}"))
        ));
        out.push(format!(
            "    {}",
            ansi::yellow(&format!("    Received: {v}"))
        ));
    }
    out.push(String::new());
    out
}

fn render_file_level_failure(
    file: &crate::test_model::TestSuiteResult,
    ctx: &Ctx,
    console_list: &[crate::format::console::ConsoleEntry],
) -> Vec<String> {
    if file.failure_message.trim().is_empty() && file.test_exec_error.is_none() {
        return vec![];
    }

    let (stacks, messages) = lines_from_details(file.failure_details.as_ref());
    let msg_lines = merge_msg_lines(&file.failure_message, &messages);

    let mut out: Vec<String> = vec![];
    if ctx.show_stacks {
        if !msg_lines.is_empty() {
            out.push(ansi::dim("    Message:"));
            msg_lines
                .iter()
                .for_each(|ln| out.push(format!("      {ln}")));
            out.push(String::new());
        }
        if !stacks.is_empty() {
            out.push(ansi::dim("    Stack:"));
            stacks
                .iter()
                .take(6)
                .for_each(|ln| out.push(format!("      {ln}")));
            out.push(String::new());
        }
    }
    out.extend(build_console_section(console_list, ctx.show_logs));
    out
}

fn render_failed_assertion(
    rel: &str,
    file: &crate::test_model::TestSuiteResult,
    assertion: &crate::test_model::TestCaseResult,
    ctx: &Ctx,
    console_list: &[crate::format::console::ConsoleEntry],
    assertion_events: &[crate::format::bridge_console::AssertionEvt],
    http_sorted: &[crate::format::bridge_console::HttpEvent],
) -> Vec<String> {
    let header = format!("{rel} > {}", assertion.full_name);
    let bullet = |text: &str| format!("{} {}", colors::failure("×"), ansi::white(text));

    let primary_block = if !assertion.failure_messages.is_empty() {
        assertion.failure_messages.join("\n")
    } else {
        file.failure_message.clone()
    };
    let (stacks, detail_msgs) = lines_from_details(
        assertion
            .failure_details
            .as_ref()
            .or(file.failure_details.as_ref()),
    );
    let messages_array = merge_msg_lines(&primary_block, &detail_msgs);
    let merged_for_stack = crate::format::stacks::collapse_stacks(
        &messages_array
            .iter()
            .chain(stacks.iter())
            .cloned()
            .collect::<Vec<_>>(),
    );
    let deepest = deepest_project_loc(&merged_for_stack, &ctx.project_hint);
    let loc_link = deepest.as_ref().map(|(file, line, _)| {
        let href = preferred_editor_href(file, Some(*line), ctx.editor_cmd.as_deref());
        let base = format!(
            "{}:{}",
            std::path::Path::new(file)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(""),
            line
        );
        crate::format::ansi::osc8(&base, &href)
    });

    let mut out: Vec<String> = vec![
        String::new(),
        draw_fail_line(ctx.width),
        {
            let hdr = if let Some(link) = loc_link.as_ref() {
                format!(
                    "{}  {}",
                    ansi::white(&header),
                    ansi::dim(&format!("({link})"))
                )
            } else {
                ansi::white(&header)
            };
            bullet(&hdr)
        },
        String::new(),
    ];
    if ctx.show_stacks {
        // Keep the per-test section aligned with headlamp-original’s vitest-style layout:
        // code frame -> blank lines -> Expected/Received -> stack snippet -> Message -> Error -> rule.
        let synth = assertion
            .location
            .as_ref()
            .filter(|loc| loc.line > 0)
            .map(|loc| Loc {
                file: file.test_file_path.replace('\\', "/"),
                line: loc.line,
                column: None,
            })
            .or_else(|| {
                deepest.as_ref().map(|(file, line, _)| Loc {
                    file: file.to_string(),
                    line: *line,
                    column: None,
                })
            });
        out.extend(build_code_frame_section(
            &messages_array
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            ctx.show_stacks,
            synth.as_ref(),
        ));

        out.extend(render_per_test_failure_details(
            &messages_array,
            &merged_for_stack,
            ctx,
        ));
    }
    {
        let primary_block = if !assertion.failure_messages.is_empty() {
            assertion.failure_messages.join("\n")
        } else {
            file.failure_message.clone()
        };
        let http_card = render_http_card(
            rel,
            &assertion.full_name,
            &assertion.title,
            &primary_block,
            &file.test_file_path.replace('\\', "/"),
            assertion_events,
            http_sorted,
        );
        if !http_card.is_empty() {
            out.extend(http_card);
        }
    }
    out.extend(build_console_section(console_list, ctx.show_logs));
    out.push(draw_fail_line(ctx.width));
    out.push(String::new());
    out
}

fn render_per_test_failure_details(
    messages_array: &[String],
    merged_for_stack: &[String],
    ctx: &Ctx,
) -> Vec<String> {
    let (expected, received) = extract_expected_received_values(messages_array);
    let expect_line = messages_array.iter().find(|ln| {
        let simple = crate::format::stacks::strip_ansi_simple(ln);
        let trimmed = simple.trim_start();
        trimmed.starts_with("expect(") && !trimmed.starts_with("expect(received).rejects")
    });
    let expect_line_simple = expect_line.as_ref().map(|ln| {
        crate::format::stacks::strip_ansi_simple(ln)
            .trim()
            .to_string()
    });
    let stack_lines_simple = merged_for_stack
        .iter()
        .map(|ln| crate::format::stacks::strip_ansi_simple(ln))
        .collect::<Vec<_>>();
    let first_stack_line = stack_lines_simple
        .iter()
        .find(|ln| crate::format::stacks::is_stack_line(ln))
        .map(|ln| crate::format::fns::color_stack_line(ln, &ctx.project_hint));

    let mut out: Vec<String> = vec![String::new()];

    out.extend(render_expected_received_sections_compact(messages_array));
    out.extend(
        stack_lines_simple
            .iter()
            .filter(|ln| crate::format::stacks::is_stack_line(ln))
            .take(2)
            .map(|ln| {
                format!(
                    "      {}",
                    colors::failure(&format!("    {}", ln.trim_start()))
                )
            }),
    );
    if let Some(ln) = expect_line_simple.as_ref() {
        out.push(format!("      {}", colors::failure(ln.trim_start())));
    }

    if expect_line_simple.is_none() && expected.is_none() && received.is_none() {
        let message_lines = messages_array
            .iter()
            .map(|ln| crate::format::stacks::strip_ansi_simple(ln))
            .map(|ln| ln.trim_end().to_string())
            .filter(|ln| {
                let trimmed = ln.trim_start();
                !(trimmed.is_empty()
                    || crate::format::stacks::is_stack_line(trimmed)
                    || CODE_FRAME_LINE_RE.is_match(trimmed))
            })
            .take(6)
            .collect::<Vec<_>>();
        if !message_lines.is_empty() {
            out.push(format!("    {}", ansi::bold("Message:")));
            message_lines
                .iter()
                .for_each(|ln| out.push(format!("    {}", ansi::yellow(ln))));
        }
    }
    out.push(String::new());

    if let (Some(expected), Some(received)) = (expected.as_ref(), received.as_ref()) {
        if let Some(expect_line) = expect_line.as_ref() {
            out.push(format!("    {}", ansi::bold("Message:")));
            out.push(format!("    {}", ansi::yellow(expect_line.trim_start())));
            out.push(format!("    {}", ansi::yellow("")));
            out.push(format!(
                "    {}",
                ansi::yellow(&format!("Expected: {}", ansi::green(expected)))
            ));
            out.push(format!(
                "    {}",
                ansi::yellow(&format!("Received: {}", ansi::red(received)))
            ));
        }

        if let Some(expect_line_simple) = expect_line_simple.as_ref() {
            out.push(format!("    {}", ansi::bold("Error:")));
            out.push(format!(
                "    {}",
                ansi::yellow(&format!("Error: {expect_line_simple}"))
            ));
            out.push(format!(
                "    {}",
                ansi::yellow(&format!("Expected: {expected}"))
            ));
            out.push(format!(
                "    {}",
                ansi::yellow(&format!("Received: {received}"))
            ));
            out.push(format!(
                "    {}",
                ansi::yellow(&format!("Received: {received}"))
            ));
            if let Some(stack) = first_stack_line.as_ref() {
                out.push(format!("          {}", stack.trim_start()));
            }
            out.push(String::new());
        }
    }

    out
}

fn render_footer(data: &TestRunModel, ctx: &Ctx, only_failures: bool) -> Vec<String> {
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
