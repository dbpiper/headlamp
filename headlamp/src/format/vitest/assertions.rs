use crate::format::ansi;
use crate::format::bridge_http::render_http_card;
use crate::format::codeframe::{Loc, build_code_frame_section, find_code_frame_start};
use crate::format::colors;
use crate::format::console::build_console_section;
use crate::format::ctx::Ctx;
use crate::format::details::{lines_from_details, merge_msg_lines};
use crate::format::fns::{deepest_project_loc, draw_fail_line};
use crate::format::paths::preferred_editor_href;

use super::console::extract_expected_received_values;

pub(super) fn render_inline_failed_assertion_block(
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
                    || super::CODE_FRAME_LINE_RE.is_match(trimmed))
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
