use crate::format::ansi;
use crate::format::bridge_http::render_http_card;
use crate::format::codeframe::{Loc, build_code_frame_section};
use crate::format::colors;
use crate::format::console::build_console_section;
use crate::format::ctx::Ctx;
use crate::format::details::{lines_from_details, merge_msg_lines};
use crate::format::fns::draw_fail_line;
use crate::format::paths::preferred_editor_href;

use super::console::extract_expected_received_values;

mod inline;
pub(super) use inline::render_inline_failed_assertion_block;

pub(super) fn render_failed_assertion(
    rel: &str,
    file: &crate::test_model::TestSuiteResult,
    assertion: &crate::test_model::TestCaseResult,
    ctx: &Ctx,
    console_list: &[crate::format::console::ConsoleEntry],
    assertion_events: &[crate::format::bridge_console::AssertionEvt],
    http_sorted: &[crate::format::bridge_console::HttpEvent],
) -> Vec<String> {
    let header = format!("{rel} > {}", assertion.full_name);
    let primary_block = primary_block_for_failed_assertion(file, assertion);
    let (stacks, detail_msgs) = lines_from_details(
        assertion
            .failure_details
            .as_ref()
            .or(file.failure_details.as_ref()),
    );
    let messages_array = merge_msg_lines(&primary_block, &detail_msgs);
    let merged_for_stack = merged_for_stack_for_failed_assertion(&messages_array, &stacks);
    let deepest = crate::format::fns::deepest_project_loc_resolved(
        &merged_for_stack,
        &ctx.project_hint,
        &ctx.cwd,
    );
    let loc_link = editor_loc_link(deepest.as_ref(), ctx);

    let mut out: Vec<String> = failed_assertion_prelude_lines(ctx, &header, loc_link.as_ref());
    maybe_push_failed_assertion_stack_sections(
        &mut out,
        ctx,
        file,
        assertion,
        deepest.as_ref(),
        &messages_array,
        &merged_for_stack,
    );
    push_failed_assertion_http_card(
        &mut out,
        rel,
        file,
        assertion,
        &primary_block,
        assertion_events,
        http_sorted,
    );
    out.extend(build_console_section(console_list, ctx.show_logs));
    out.push(draw_fail_line(ctx.width));
    out.push(String::new());
    out
}

fn failed_assertion_prelude_lines(
    ctx: &Ctx,
    header: &str,
    loc_link: Option<&String>,
) -> Vec<String> {
    let header_line = loc_link
        .map(|link| {
            format!(
                "{}  {}",
                ansi::white(header),
                ansi::dim(&format!("({link})"))
            )
        })
        .unwrap_or_else(|| ansi::white(header));
    vec![
        String::new(),
        draw_fail_line(ctx.width),
        failure_bullet(&header_line),
        String::new(),
    ]
}

fn failure_bullet(text: &str) -> String {
    format!("{} {}", colors::failure("×"), ansi::white(text))
}

fn maybe_push_failed_assertion_stack_sections(
    out: &mut Vec<String>,
    ctx: &Ctx,
    file: &crate::test_model::TestSuiteResult,
    assertion: &crate::test_model::TestCaseResult,
    deepest: Option<&(String, i64, i64)>,
    messages_array: &[String],
    merged_for_stack: &[String],
) {
    if !ctx.show_stacks {
        return;
    }
    let synth = synth_loc_for_failed_assertion(file, assertion, deepest);
    out.extend(build_code_frame_section(
        messages_array,
        ctx.show_stacks,
        synth.as_ref(),
    ));
    out.extend(render_per_test_failure_details(
        messages_array,
        merged_for_stack,
        ctx,
    ));
}

fn push_failed_assertion_http_card(
    out: &mut Vec<String>,
    rel: &str,
    file: &crate::test_model::TestSuiteResult,
    assertion: &crate::test_model::TestCaseResult,
    primary_block: &str,
    assertion_events: &[crate::format::bridge_console::AssertionEvt],
    http_sorted: &[crate::format::bridge_console::HttpEvent],
) {
    let http_card = render_http_card(
        rel,
        &assertion.full_name,
        &assertion.title,
        primary_block,
        &file.test_file_path.replace('\\', "/"),
        assertion_events,
        http_sorted,
    );
    if http_card.is_empty() {
        return;
    }
    out.extend(http_card);
}

fn primary_block_for_failed_assertion(
    file: &crate::test_model::TestSuiteResult,
    assertion: &crate::test_model::TestCaseResult,
) -> String {
    if !assertion.failure_messages.is_empty() {
        assertion.failure_messages.join("\n")
    } else {
        file.failure_message.clone()
    }
}

fn merged_for_stack_for_failed_assertion(
    messages_array: &[String],
    stacks: &[String],
) -> Vec<String> {
    crate::format::stacks::collapse_stacks(
        &messages_array
            .iter()
            .chain(stacks.iter())
            .cloned()
            .collect::<Vec<_>>(),
    )
}

fn editor_loc_link(deepest: Option<&(String, i64, i64)>, ctx: &Ctx) -> Option<String> {
    deepest.as_ref().map(|(file, line, _)| {
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
    })
}

fn synth_loc_for_failed_assertion(
    file: &crate::test_model::TestSuiteResult,
    assertion: &crate::test_model::TestCaseResult,
    deepest: Option<&(String, i64, i64)>,
) -> Option<Loc> {
    assertion
        .location
        .as_ref()
        .filter(|loc| loc.line > 0)
        .map(|loc| Loc {
            file: file.test_file_path.replace('\\', "/"),
            line: loc.line,
            column: None,
        })
        .or_else(|| {
            deepest.map(|(file, line, _)| Loc {
                file: file.to_string(),
                line: *line,
                column: None,
            })
        })
}

fn render_per_test_failure_details(
    messages_array: &[String],
    merged_for_stack: &[String],
    ctx: &Ctx,
) -> Vec<String> {
    let (expected, received) = extract_expected_received_values(messages_array);
    let expect_line = find_expect_line(messages_array);
    let expect_line_simple = expect_line.as_ref().map(|ln| {
        crate::format::stacks::strip_ansi_simple(ln)
            .trim()
            .to_string()
    });

    let mut out: Vec<String> = vec![String::new()];
    let has_pretty = expected.is_some() && received.is_some();
    if let (Some(expected), Some(received)) = (expected.as_ref(), received.as_ref()) {
        out.extend(render_pretty_expected_received(expected, received));
    }
    let stack_preview = build_stack_preview(merged_for_stack, ctx);
    out.extend(render_message_section_like_legacy(
        messages_array,
        expect_line_simple.as_deref(),
        has_pretty,
        &stack_preview,
    ));
    if ctx.show_stacks && stack_preview.is_empty() {
        out.extend(render_stack_tail_like_legacy(merged_for_stack, ctx));
    }
    out
}

fn find_expect_line(messages_array: &[String]) -> Option<&String> {
    messages_array.iter().find(|ln| {
        let simple = crate::format::stacks::strip_ansi_simple(ln);
        let trimmed = simple.trim_start();
        trimmed.starts_with("expect(") && !trimmed.starts_with("expect(received).rejects")
    })
}

fn render_pretty_expected_received(expected: &str, received: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    out.push(format!("    {}", ansi::bold("Expected")));
    expected
        .lines()
        .for_each(|line| out.push(format!("      {}", colors::success(line))));
    out.push(format!("    {}", ansi::bold("Received")));
    received
        .lines()
        .for_each(|line| out.push(format!("      {}", colors::failure(line))));
    out.push(String::new());
    out
}

fn build_stack_preview(merged_for_stack: &[String], ctx: &Ctx) -> Vec<String> {
    if !ctx.show_stacks {
        return vec![];
    }
    merged_for_stack
        .iter()
        .map(|ln| crate::format::stacks::strip_ansi_simple(ln))
        .filter(|ln| crate::format::stacks::is_stack_line(ln))
        .filter(|ln| ctx.project_hint.is_match(ln))
        .take(2)
        .map(|ln| {
            format!(
                "      {}",
                crate::format::fns::color_stack_line(&ln, &ctx.project_hint)
            )
        })
        .collect::<Vec<_>>()
}

fn render_message_section_like_legacy(
    messages_array: &[String],
    expect_line_simple: Option<&str>,
    suppress_diff: bool,
    stack_preview: &[String],
) -> Vec<String> {
    let label = if expect_line_simple.is_some() {
        "Assertion:"
    } else {
        "Message:"
    };
    let body_lines = expect_line_simple
        .map(|ln| vec![ln.trim_start().to_string()])
        .unwrap_or_else(|| fallback_message_lines(messages_array));

    let filtered_body = if suppress_diff {
        body_lines
            .into_iter()
            .filter(|ln| {
                let trimmed = ln.trim_start();
                !(trimmed.starts_with("Expected:")
                    || trimmed.starts_with("Received:")
                    || trimmed.starts_with("Difference:")
                    || trimmed.starts_with("- Expected")
                    || trimmed.starts_with("+ Received"))
            })
            .collect::<Vec<_>>()
    } else {
        body_lines
    };

    if filtered_body.is_empty() && stack_preview.is_empty() {
        return vec![];
    }

    let mut out: Vec<String> = vec![];
    out.push(format!("    {}", ansi::bold(label)));
    filtered_body
        .iter()
        .for_each(|ln| out.push(format!("    {}", ansi::yellow(ln))));
    stack_preview.iter().for_each(|ln| out.push(ln.to_string()));
    out.push(String::new());
    out
}

fn render_stack_tail_like_legacy(merged_for_stack: &[String], ctx: &Ctx) -> Vec<String> {
    if !ctx.show_stacks {
        return vec![];
    }
    let only_stack = merged_for_stack
        .iter()
        .map(|ln| crate::format::stacks::strip_ansi_simple(ln))
        .filter(|ln| crate::format::stacks::is_stack_line(ln))
        .collect::<Vec<_>>();
    let tail = only_stack
        .into_iter()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    if tail.is_empty() {
        return vec![];
    }
    let mut out: Vec<String> = vec![ansi::dim("    Stack:")];
    tail.iter().for_each(|ln| {
        out.push(format!(
            "      {}",
            crate::format::fns::color_stack_line(ln, &ctx.project_hint)
        ));
    });
    out.push(String::new());
    out
}

fn fallback_message_lines(messages_array: &[String]) -> Vec<String> {
    let raw = messages_array
        .iter()
        .map(|ln| crate::format::stacks::strip_ansi_simple(ln))
        .map(|ln| ln.trim_end().to_string())
        .filter(|ln| {
            let trimmed = ln.trim_start();
            !(trimmed.is_empty()
                || crate::format::stacks::is_stack_line(trimmed)
                || super::CODE_FRAME_LINE_RE.is_match(trimmed))
        })
        .map(|ln| normalize_message_line(&ln))
        .filter(|ln| !ln.trim().is_empty())
        .take(12)
        .collect::<Vec<_>>();

    let (seen, out) = raw.into_iter().fold(
        (
            std::collections::BTreeSet::<String>::new(),
            Vec::<String>::new(),
        ),
        |(mut seen, mut out), line| {
            if seen.contains(&line) {
                return (seen, out);
            }
            seen.insert(line.clone());
            out.push(line);
            (seen, out)
        },
    );
    let _ = seen;
    out.into_iter().take(6).collect::<Vec<_>>()
}

fn normalize_message_line(line: &str) -> String {
    let trimmed = line.trim_start();
    let trimmed = trimmed
        .strip_prefix('E')
        .and_then(|rest| rest.strip_prefix(' '))
        .map(|rest| rest.trim_start())
        .unwrap_or(trimmed);
    if trimmed.starts_with("thread '") && trimmed.contains("' panicked at ") {
        return String::new();
    }
    if trimmed.starts_with("panicked at ") {
        return String::new();
    }
    if trimmed.trim() == "stack backtrace:" {
        return String::new();
    }
    if let Some(rest) = trimmed.strip_prefix("Error: ") {
        return rest.trim_start().to_string();
    }
    if let Some(rest) = trimmed.strip_prefix("AssertionError: ") {
        return rest.trim_start().to_string();
    }
    if trimmed.starts_with("note: Some details are omitted") {
        return String::new();
    }
    if trimmed.starts_with("note: run with `RUST_BACKTRACE=") {
        return String::new();
    }
    if let Some((_, rest)) = trimmed.split_once(": ")
        && trimmed
            .split_once(": ")
            .is_some_and(|(head, _)| head.ends_with("Error") || head.ends_with("Exception"))
    {
        return rest.trim_start().to_string();
    }
    trimmed.to_string()
}

// legacy-style rendering no longer uses the older compact prefix and expected/received block helpers
