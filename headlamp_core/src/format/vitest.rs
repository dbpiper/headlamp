use crate::format::ansi;
use crate::format::bridge::{BridgeAggregated, BridgeJson};
use crate::format::bridge_console::parse_bridge_console;
use crate::format::bridge_http::render_http_card;
use crate::format::codeframe::{Loc, build_code_frame_section};
use crate::format::colors;
use crate::format::console::ConsoleEntry;
use crate::format::console::build_console_section;
use crate::format::ctx::Ctx;
use crate::format::details::{lines_from_details, merge_msg_lines};
use crate::format::fns::{
    build_file_badge_line, build_per_file_overview, deepest_project_loc, draw_fail_line, draw_rule,
    render_run_line,
};
use crate::format::paths::preferred_editor_href;
use path_slash::PathExt;

pub fn render_vitest_from_jest_json(data: &BridgeJson, ctx: &Ctx, only_failures: bool) -> String {
    let mut lines: Vec<String> = vec![];

    if !only_failures {
        lines.push(render_run_line(&ctx.cwd));
        lines.push(String::new());
    }

    for file in &data.test_results {
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
        let has_suite_failure = !file.failure_message.trim().is_empty() || file.status == "failed";
        let badge_count = if has_suite_failure {
            failed.max(1)
        } else {
            failed
        };
        let (http, assertion_events, console_list) = parse_bridge_console(file.console.as_ref());
        let mut http_sorted = http;
        http_sorted.sort_by_key(|evt| evt.timestamp_ms);

        if !only_failures {
            let assertions = file
                .test_results
                .iter()
                .map(|a| (a.full_name.clone(), a.status.clone()))
                .collect::<Vec<_>>();
            lines.extend(build_per_file_overview(&rel, &assertions));
        }
        if !(only_failures && badge_count == 0) {
            lines.push(build_file_badge_line(&rel, badge_count));
            if badge_count > 0 {
                lines.push(String::new());
            }
        }

        if !only_failures {
            let failed_assertions = file.test_results.iter().filter(|a| a.status == "failed");
            if let Some(first_failed) = failed_assertions.clone().next() {
                lines.extend(render_inline_failed_assertion_block(file, first_failed, ctx));
            }
        }

        if has_suite_failure {
            lines.extend(render_file_level_failure(file, ctx, &console_list));
        }

        for assertion in file.test_results.iter().filter(|a| a.status == "failed") {
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

    lines.extend(render_footer(data, ctx));
    lines.join("\n")
}

fn render_inline_failed_assertion_block(
    file: &crate::format::bridge::BridgeFileResult,
    assertion: &crate::format::bridge::BridgeAssertion,
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
    let messages_array = merge_msg_lines(&primary_block, &detail_msgs);
    render_ts_style_assertion_failure(&messages_array, &stacks, ctx)
}

fn render_ts_style_assertion_failure(
    messages_array: &[String],
    stacks: &[String],
    ctx: &Ctx,
) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    out.extend(build_code_frame_section(
        &messages_array.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        false,
        None,
    ));
    out.extend(render_expected_received_sections(messages_array));
    out.extend(render_assertion_block(messages_array));
    if ctx.show_stacks && !stacks.is_empty() {
        out.push(ansi::dim("    Stack:"));
        stacks
            .iter()
            .take(6)
            .for_each(|ln| out.push(format!("            {ln}")));
        out.push(String::new());
    }
    out
}

fn render_expected_received_sections(messages_array: &[String]) -> Vec<String> {
    let stripped = messages_array
        .iter()
        .map(|ln| crate::format::stacks::strip_ansi_simple(ln))
        .collect::<Vec<_>>();
    let expected = stripped
        .iter()
        .find_map(|ln| ln.strip_prefix("Expected: ").map(|v| v.trim().to_string()));
    let received = stripped
        .iter()
        .find_map(|ln| ln.strip_prefix("Received: ").map(|v| v.trim().to_string()));
    if expected.is_none() && received.is_none() {
        return vec![];
    }
    let mut out: Vec<String> = vec![String::new(), String::new()];
    out.push(ansi::bold("    Expected"));
    out.push(String::new());
    if let Some(v) = expected {
        out.push(format!("      {}", colors::success(&v)));
        out.push(String::new());
    }
    out.push(ansi::bold("    Received"));
    out.push(String::new());
    if let Some(v) = received {
        out.push(format!("      {}", colors::failure(&v)));
        out.push(String::new());
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
    let mut out: Vec<String> = vec![String::new(), String::new(), ansi::bold("    Assertion:")];
    out.push(String::new());
    stripped
        .iter()
        .skip(start)
        .take_while(|ln| !ln.trim_start().starts_with("at "))
        .filter(|ln| !ln.trim().is_empty())
        .for_each(|ln| out.push(ansi::yellow(&format!("    {ln}"))));
    out.push(String::new());
    out.push(String::new());
    out
}

fn render_file_level_failure(
    file: &crate::format::bridge::BridgeFileResult,
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
    file: &crate::format::bridge::BridgeFileResult,
    assertion: &crate::format::bridge::BridgeAssertion,
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
    let loc_link = deepest.as_ref().map(|(file, line)| {
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
    if !messages_array.is_empty() {
        out.push(ansi::dim("    Message:"));
        messages_array
            .iter()
            .for_each(|ln| out.push(format!("      {ln}")));
        out.push(String::new());
    }
    if ctx.show_stacks {
        let synth = deepest.as_ref().map(|(file, line)| Loc {
            file: file.to_string(),
            line: *line,
        });
        out.extend(build_code_frame_section(
            &messages_array
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            ctx.show_stacks,
            synth.as_ref(),
        ));

        if !stacks.is_empty() {
            out.push(ansi::dim("    Stack:"));
            merged_for_stack
                .iter()
                .filter(|ln| {
                    crate::format::stacks::is_stack_line(&crate::format::stacks::strip_ansi_simple(
                        ln,
                    ))
                })
                .take(6)
                .for_each(|ln| out.push(format!("      {ln}")));
            out.push(String::new());
        }
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
    out.extend(build_console_section(
        &filter_console_for_assertion(console_list, &assertion.full_name),
        ctx.show_logs,
    ));
    out.push(draw_fail_line(ctx.width));
    out.push(String::new());
    out
}

fn filter_console_for_assertion(
    console_list: &[ConsoleEntry],
    assertion_full_name: &str,
) -> Vec<ConsoleEntry> {
    let matching = console_list
        .iter()
        .filter(|e| e.current_test_name.as_deref() == Some(assertion_full_name))
        .cloned()
        .collect::<Vec<_>>();
    if !matching.is_empty() {
        return matching;
    }
    console_list.to_vec()
}

fn render_footer(data: &BridgeJson, ctx: &Ctx) -> Vec<String> {
    let failed_count = data.aggregated.num_failed_tests;
    let timed_out_count = data.aggregated.num_timed_out_tests.unwrap_or(0);

    let mut out: Vec<String> = vec![
        draw_rule(
            ctx.width,
            Some(&colors::bg_failure(&ansi::white(&format!(
                " Failed Tests {failed_count} "
            )))),
        ),
        String::new(),
        vitest_footer(&data.aggregated),
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

fn vitest_footer(agg: &BridgeAggregated) -> String {
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
        .map(|ms| format!("{}ms", ms.max(0)))
        .unwrap_or_else(|| String::new());
    let thread = ansi::dim("(in thread 0ms, 0.00%)");

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
        format!("{}      {} {}", ansi::bold("Time"), time, thread),
    ]
    .join("\n")
}
