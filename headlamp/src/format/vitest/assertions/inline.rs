use crate::format::ansi;
use crate::format::codeframe::{Loc, build_code_frame_section, find_code_frame_start};
use crate::format::colors;
use crate::format::ctx::Ctx;

use super::super::console::extract_expected_received_values;

pub(in crate::format::vitest) fn render_inline_failed_assertion_block(
    file: &crate::test_model::TestSuiteResult,
    assertion: &crate::test_model::TestCaseResult,
    ctx: &Ctx,
) -> Vec<String> {
    let primary_block = super::primary_block_for_failed_assertion(file, assertion);
    let (stacks, detail_msgs) = crate::format::details::lines_from_details(
        assertion
            .failure_details
            .as_ref()
            .or(file.failure_details.as_ref()),
    );
    let messages_array = crate::format::details::merge_msg_lines(&primary_block, &detail_msgs);
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
        .and_then(|f| {
            crate::format::failure_diagnostics::resolve_existing_path_best_effort(&ctx.cwd, f)
        });
    let file_path = preferred_file.unwrap_or_else(|| file.test_file_path.clone());
    (line > 0).then_some(Loc {
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
                || super::super::CODE_FRAME_LINE_RE.is_match(trimmed)
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
