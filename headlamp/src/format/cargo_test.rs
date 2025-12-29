use std::path::Path;

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
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("Running ")?;
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
    let status = rest.trim();
    if status == "ok" {
        return Some(ParsedTestLine::Completed {
            name: name.to_string(),
            status: "passed".to_string(),
        });
    }
    if status == "FAILED" {
        return Some(ParsedTestLine::Completed {
            name: name.to_string(),
            status: "failed".to_string(),
        });
    }
    Some(ParsedTestLine::Pending {
        name: name.to_string(),
        inline_output: None,
    })
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
    Some((name, consumed, collected.join("\n")))
}

fn should_keep_as_console_line(line: &str) -> bool {
    let trimmed = line.trim();
    !(trimmed.is_empty()
        || trimmed.starts_with("running ")
        || trimmed.starts_with("test ")
        || trimmed.starts_with("test result:"))
}
