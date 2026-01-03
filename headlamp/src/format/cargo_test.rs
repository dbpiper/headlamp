use std::borrow::Cow;
use std::path::Path;
use std::time::Duration;

use crate::format::unstructured_engine::{
    ParsedTestLine, UnstructuredDialect, UnstructuredStreamEvent, UnstructuredStreamParser,
};
use crate::test_model::TestRunModel;

pub type CargoTestStreamEvent = UnstructuredStreamEvent;

#[derive(Debug, Clone, Copy, Default)]
pub struct CargoTestDialect;

impl UnstructuredDialect for CargoTestDialect {
    fn origin(&self) -> &'static str {
        "cargo-test"
    }

    fn parse_suite_header_source_path(&self, line: &str) -> Option<String> {
        parse_suite_header_source_path(line)
    }

    fn parse_test_line(&self, line: &str) -> Option<ParsedTestLine> {
        parse_test_line_extended(line)
    }

    fn parse_status_only_line(&self, line: &str) -> Option<String> {
        parse_status_only_line(line)
    }

    fn parse_failure_block(
        &self,
        lines: &[String],
        start_index: usize,
    ) -> Option<(String, usize, String)> {
        parse_failure_block(lines, start_index)
    }

    fn parse_panic_block(
        &self,
        lines: &[String],
        start_index: usize,
    ) -> Option<(String, usize, String)> {
        parse_panic_block(lines, start_index)
    }

    fn is_output_section_header(&self, line: &str) -> Option<String> {
        let trimmed = line.trim();
        if !trimmed.starts_with("---- ") || !trimmed.ends_with(" ----") {
            return None;
        }
        trimmed
            .strip_prefix("---- ")
            .and_then(|s| s.strip_suffix(" ----"))
            .map(|s| s.trim().to_string())
    }

    fn should_keep_as_console_line(&self, line: &str) -> bool {
        should_keep_as_console_line(line)
    }
}

#[derive(Debug, Clone)]
pub struct CargoTestStreamParser {
    inner: UnstructuredStreamParser<CargoTestDialect>,
}

impl CargoTestStreamParser {
    pub fn new(repo_root: &Path) -> Self {
        Self {
            inner: UnstructuredStreamParser::new_default(repo_root),
        }
    }

    pub fn push_line(&mut self, line: &str) -> Vec<CargoTestStreamEvent> {
        self.inner.push_line(line)
    }

    pub fn finalize(self) -> Option<TestRunModel> {
        self.inner.finalize()
    }
}

pub fn parse_cargo_test_output(repo_root: &Path, combined_output: &str) -> Option<TestRunModel> {
    let mut parser = CargoTestStreamParser::new(repo_root);
    combined_output.lines().for_each(|line| {
        let _ = parser.push_line(line);
    });
    parser.finalize()
}

fn parse_suite_header_source_path(line: &str) -> Option<String> {
    let stripped = if line.contains('\u{1b}') {
        Cow::Owned(String::from_utf8_lossy(&strip_ansi_escapes::strip(line.as_bytes())).to_string())
    } else {
        Cow::Borrowed(line)
    };
    let trimmed = stripped.trim();
    // Cargo can emit `Running <path>` with styling that sometimes removes the space after
    // "Running" once ANSI escapes are stripped (e.g. `Running<path>`).
    // Be tolerant and treat any whitespace as optional.
    let rest = trimmed.strip_prefix("Running")?;
    let rest = rest.trim_start();
    let (path_like, _) = rest.split_once(" (").unwrap_or((rest, ""));
    let cleaned = path_like.trim();
    let cleaned = cleaned.strip_prefix("unittests ").unwrap_or(cleaned).trim();
    (!cleaned.is_empty()).then(|| cleaned.to_string())
}

fn parse_test_line_extended(line: &str) -> Option<ParsedTestLine> {
    let trimmed = line.trim();
    if !trimmed.starts_with("test ") {
        return None;
    }
    let rest = trimmed.strip_prefix("test ")?;
    let (name, rest) = rest.split_once(" ... ")?;
    let rest_trimmed = rest.trim();
    let (status_word, duration) = split_status_and_report_time(rest_trimmed);
    if status_word == "ok" {
        return Some(ParsedTestLine::Completed {
            name: name.to_string(),
            status: "passed".to_string(),
            duration,
        });
    }
    if status_word == "FAILED" {
        return Some(ParsedTestLine::Completed {
            name: name.to_string(),
            status: "failed".to_string(),
            duration,
        });
    }
    if status_word == "ignored" {
        return Some(ParsedTestLine::Completed {
            name: name.to_string(),
            status: "pending".to_string(),
            duration,
        });
    }
    Some(ParsedTestLine::Pending {
        name: name.to_string(),
        inline_output: None,
    })
}

fn split_status_and_report_time(rest: &str) -> (&str, Option<Duration>) {
    let status_word = rest.split_whitespace().next().unwrap_or(rest).trim();
    let duration = parse_report_time_suffix(rest);
    (status_word, duration)
}

fn parse_report_time_suffix(rest: &str) -> Option<Duration> {
    let open = rest.rfind('(')?;
    let close = rest[open..].find(')')? + open;
    let inside = rest[open.saturating_add(1)..close].trim();
    let seconds_text = inside.strip_suffix('s')?.trim();
    seconds_text
        .parse::<f64>()
        .ok()
        .filter(|sec| *sec >= 0.0)
        .map(Duration::from_secs_f64)
}

fn parse_status_only_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    match trimmed {
        "ok" => Some("passed".to_string()),
        "FAILED" => Some("failed".to_string()),
        _ => None,
    }
}

fn parse_failure_block(_lines: &[String], _start_index: usize) -> Option<(String, usize, String)> {
    None
}

fn parse_panic_block(lines: &[String], start_index: usize) -> Option<(String, usize, String)> {
    let first = lines.get(start_index)?.as_str();
    if !first.trim_start().starts_with("thread '") {
        return None;
    }
    let name = first
        .trim_start()
        .strip_prefix("thread '")?
        .split_once("'")?
        .0
        .to_string();

    let mut collected: Vec<String> = vec![first.to_string()];
    let mut index = start_index + 1;
    while index < lines.len() {
        let current = lines[index].as_str();
        let trimmed = current.trim();
        let prev_blank = lines
            .get(index.wrapping_sub(1))
            .is_some_and(|prev| prev.trim().is_empty());
        let is_next_section = prev_blank
            && (trimmed == "failures:"
                || trimmed.starts_with("test result:")
                || trimmed.starts_with("Running "));
        if is_next_section {
            break;
        }
        collected.push(current.to_string());
        index += 1;
    }

    let consumed = index.saturating_sub(start_index);
    Some((
        name,
        consumed,
        extract_failure_message_from_panic_block(&collected),
    ))
}

fn should_skip_panic_block_line(trimmed: &str) -> bool {
    trimmed.is_empty() || trimmed.starts_with("note: run with `RUST_BACKTRACE=")
}

fn is_panic_message_noise_line(trimmed: &str) -> bool {
    trimmed == "stack backtrace:"
        || trimmed
            == "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace"
}

fn push_location_line_from_panic_headline(out: &mut Vec<String>, trimmed: &str) -> bool {
    let Some((_prefix, rest)) = trimmed.split_once("' panicked at ") else {
        return false;
    };
    let location = rest.trim();
    if !location.is_empty() {
        out.push(format!("panicked at {}", location.trim()));
    }
    true
}

fn push_rust_assertion_line(out: &mut Vec<String>, trimmed: &str) -> bool {
    if trimmed.starts_with("assertion ") && trimmed.contains(" failed") {
        out.push(trimmed.to_string());
        true
    } else {
        false
    }
}

fn is_left_right_line(trimmed: &str) -> bool {
    let stripped = trimmed.trim_start();
    stripped.starts_with("left: ") || stripped.starts_with("right: ")
}

fn first_non_empty_stripped_line(lines: &[String]) -> String {
    lines
        .iter()
        .map(|line| crate::format::stacks::strip_ansi_simple(line))
        .map(|line| line.trim_end().to_string())
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
}

fn extract_failure_message_from_panic_block(lines: &[String]) -> String {
    let mut out: Vec<String> = vec![];

    let mut had_assertion_line = false;
    let mut had_left_right = false;

    for raw in lines {
        let stripped = crate::format::stacks::strip_ansi_simple(raw);
        let trimmed = stripped.trim_end();
        if should_skip_panic_block_line(trimmed) {
            continue;
        }

        if push_location_line_from_panic_headline(&mut out, trimmed) {
            continue;
        }

        if push_rust_assertion_line(&mut out, trimmed) {
            had_assertion_line = true;
            continue;
        }

        if is_left_right_line(trimmed) {
            had_left_right = true;
            out.push(trimmed.to_string());
            continue;
        }

        if crate::format::stacks::is_stack_line(trimmed) {
            out.push(trimmed.to_string());
            continue;
        }

        // Preserve the panic message body (e.g. assert!() formatted messages) so we don't lose
        // critical diagnostics like "found N files over limit ...".
        if !is_panic_message_noise_line(trimmed) && !trimmed.trim().is_empty() {
            out.push(trimmed.to_string());
        }
    }

    if out.is_empty() {
        return first_non_empty_stripped_line(lines);
    }

    if had_left_right && !had_assertion_line {
        out.insert(0, "assertion failed".to_string());
    }

    out.join("\n")
}

fn should_keep_as_console_line(line: &str) -> bool {
    let trimmed = line.trim();
    !(trimmed.is_empty()
        || trimmed.starts_with("running ")
        || trimmed.starts_with("test ")
        || trimmed.starts_with("test result:"))
}
