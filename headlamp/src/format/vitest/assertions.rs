use crate::format::ansi;
use crate::format::bridge_http::render_http_card;
use crate::format::codeframe::{Loc, build_code_frame_section, find_code_frame_start};
use crate::format::colors;
use crate::format::console::build_console_section;
use crate::format::ctx::Ctx;
use crate::format::details::{lines_from_details, merge_msg_lines};
use crate::format::fns::draw_fail_line;
use crate::format::paths::preferred_editor_href;

use super::console::extract_expected_received_values;

pub(super) fn render_inline_failed_assertion_block(
    file: &crate::test_model::TestSuiteResult,
    assertion: &crate::test_model::TestCaseResult,
    ctx: &Ctx,
) -> Vec<String> {
    let primary_block = primary_block_for_failed_assertion(file, assertion);
    let (stacks, detail_msgs) = lines_from_details(
        assertion
            .failure_details
            .as_ref()
            .or(file.failure_details.as_ref()),
    );
    let messages_array = merge_msg_lines(&primary_block, &detail_msgs);
    let stacks_for_render = inline_stacks_for_render(file, &stacks, ctx);
    let messages_for_code_frame = inline_messages_for_code_frame(file, &messages_array);
    let synth_loc = inline_synth_loc(file, assertion, &messages_array, &stacks, ctx);
    render_ts_style_assertion_failure(
        &messages_for_code_frame,
        &stacks_for_render,
        ctx,
        synth_loc.as_ref(),
    )
}

fn inline_stacks_for_render(
    file: &crate::test_model::TestSuiteResult,
    stacks: &[String],
    ctx: &Ctx,
) -> Vec<String> {
    if !stacks.is_empty() {
        return stacks.to_vec();
    }
    file.failure_message
        .lines()
        .filter(|line| {
            crate::format::stacks::is_stack_line(&crate::format::stacks::strip_ansi_simple(line))
        })
        .map(|line| crate::format::fns::color_stack_line(line, &ctx.project_hint))
        .collect::<Vec<_>>()
}

fn inline_messages_for_code_frame(
    file: &crate::test_model::TestSuiteResult,
    messages_array: &[String],
) -> Vec<String> {
    let mut messages_for_code_frame = messages_array.to_vec();
    let file_failure_lines = file
        .failure_message
        .lines()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    let Some(start) = find_code_frame_start(&file_failure_lines) else {
        return messages_for_code_frame;
    };
    let frame_block = file_failure_lines
        .iter()
        .skip(start)
        .take_while(|line| !line.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>();
    if frame_block.is_empty() {
        return messages_for_code_frame;
    }
    messages_for_code_frame.extend(frame_block);
    messages_for_code_frame
}

fn inline_synth_loc(
    file: &crate::test_model::TestSuiteResult,
    assertion: &crate::test_model::TestCaseResult,
    messages_array: &[String],
    stacks: &[String],
    ctx: &Ctx,
) -> Option<Loc> {
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
    let preferred_file = stack_loc
        .as_ref()
        .map(|(f, _, _)| f.as_str())
        .and_then(|f| crate::format::failure_diagnostics::resolve_existing_path_best_effort(&ctx.cwd, f));
    let file_path = preferred_file.unwrap_or_else(|| file.test_file_path.clone());
    (line > 0).then(|| Loc {
        file: file_path,
        line,
        column,
    })
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
                || super::CODE_FRAME_LINE_RE.is_match(trimmed)
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
    let stack_lines_simple = merged_for_stack
        .iter()
        .map(|ln| crate::format::stacks::strip_ansi_simple(ln))
        .collect::<Vec<_>>();
    let first_stack_line = stack_lines_simple
        .iter()
        .find(|ln| crate::format::stacks::is_stack_line(ln))
        .map(|ln| crate::format::fns::color_stack_line(ln, &ctx.project_hint));

    let mut out: Vec<String> = vec![String::new()];
    out.extend(failure_details_compact_prefix(
        messages_array,
        &stack_lines_simple,
        expect_line_simple.as_deref(),
        expected.as_ref(),
        received.as_ref(),
    ));
    out.push(String::new());
    out.extend(failure_details_expected_received_blocks(
        expect_line.map(|s| s.as_str()),
        expect_line_simple.as_deref(),
        expected.as_ref(),
        received.as_ref(),
        first_stack_line.as_deref(),
    ));
    out
}

fn find_expect_line(messages_array: &[String]) -> Option<&String> {
    messages_array.iter().find(|ln| {
        let simple = crate::format::stacks::strip_ansi_simple(ln);
        let trimmed = simple.trim_start();
        trimmed.starts_with("expect(") && !trimmed.starts_with("expect(received).rejects")
    })
}

fn failure_details_compact_prefix(
    messages_array: &[String],
    stack_lines_simple: &[String],
    expect_line_simple: Option<&str>,
    _expected: Option<&String>,
    _received: Option<&String>,
) -> Vec<String> {
    let mut out: Vec<String> = vec![];
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
    expect_line_simple
        .into_iter()
        .for_each(|ln| out.push(format!("      {}", colors::failure(ln.trim_start()))));
    if expect_line_simple.is_none() {
        let message_lines = fallback_message_lines(messages_array);
        if !message_lines.is_empty() {
            out.push(format!("    {}", ansi::bold("Message:")));
            message_lines
                .iter()
                .for_each(|ln| out.push(format!("    {}", ansi::yellow(ln))));
        }
    }
    out
}

fn fallback_message_lines(messages_array: &[String]) -> Vec<String> {
    let lines = messages_array
        .iter()
        .map(|ln| crate::format::stacks::strip_ansi_simple(ln))
        .map(|ln| ln.trim_end().to_string())
        .map(|ln| normalize_message_line(&ln))
        .filter(|ln| {
            let trimmed = ln.trim_start();
            !(trimmed.is_empty()
                || crate::format::stacks::is_stack_line(trimmed)
                || super::CODE_FRAME_LINE_RE.is_match(trimmed))
        })
        .take(6)
        .collect::<Vec<_>>();
    lines
}

fn normalize_message_line(line: &str) -> String {
    let trimmed = line.trim_start();
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
    if let Some((_, rest)) = trimmed.split_once(": ")
        && trimmed
            .split_once(": ")
            .is_some_and(|(head, _)| head.ends_with("Error") || head.ends_with("Exception"))
    {
        return rest.trim_start().to_string();
    }
    trimmed.to_string()
}

fn failure_details_expected_received_blocks(
    expect_line: Option<&str>,
    expect_line_simple: Option<&str>,
    expected: Option<&String>,
    received: Option<&String>,
    first_stack_line: Option<&str>,
) -> Vec<String> {
    let Some(expected) = expected else {
        return vec![];
    };
    let Some(received) = received else {
        return vec![];
    };
    let mut out: Vec<String> = vec![];
    expect_line.into_iter().for_each(|line| {
        out.push(format!("    {}", ansi::bold("Message:")));
        out.push(format!("    {}", ansi::yellow(line.trim_start())));
        out.push(format!("    {}", ansi::yellow("")));
        out.push(format!(
            "    {}",
            ansi::yellow(&format!("Expected: {}", ansi::green(expected)))
        ));
        out.push(format!(
            "    {}",
            ansi::yellow(&format!("Received: {}", ansi::red(received)))
        ));
    });
    expect_line_simple.into_iter().for_each(|simple| {
        out.push(format!("    {}", ansi::bold("Error:")));
        out.push(format!("    {}", ansi::yellow(&format!("Error: {simple}"))));
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
        first_stack_line
            .into_iter()
            .for_each(|stack| out.push(format!("          {}", stack.trim_start())));
        out.push(String::new());
    });
    out
}
