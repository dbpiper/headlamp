use crate::format::ansi;
use crate::format::console::build_console_section;
use crate::format::ctx::Ctx;
use crate::format::details::{lines_from_details, merge_msg_lines};

pub(super) fn render_file_level_failure(
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
