use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::test_model::TestConsoleEntry;
use crate::test_model::{TestCaseResult, TestRunAggregated, TestRunModel, TestSuiteResult};

#[derive(Debug, Clone)]
pub enum CargoTestStreamEvent {
    SuiteStarted { suite_path: String },
    TestFinished {
        suite_path: String,
        test_name: String,
        status: String,
    },
    OutputLine {
        suite_path: String,
        test_name: Option<String>,
        line: String,
    },
}

#[derive(Debug, Clone)]
enum ParsedTestLine {
    Completed {
        name: String,
        status: String,
    },
    Pending {
        name: String,
        inline_output: Option<String>,
    },
}

#[derive(Debug, Clone)]
struct SuiteBlock {
    source_path: String,
    lines: Vec<String>,
}

#[derive(Debug, Clone)]
struct SuiteState {
    source_path: String,
    lines: Vec<String>,
    active_output_test_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CargoTestStreamParser {
    repo_root: PathBuf,
    suites: Vec<TestSuiteResult>,
    current: Option<SuiteState>,
}

impl CargoTestStreamParser {
    pub fn new(repo_root: &Path) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
            suites: vec![],
            current: None,
        }
    }

    pub fn push_line(&mut self, line: &str) -> Vec<CargoTestStreamEvent> {
        let mut events: Vec<CargoTestStreamEvent> = vec![];
        if let Some(source_path) = parse_suite_header_source_path(line) {
            self.flush_current_suite();
            self.current = Some(SuiteState {
                source_path: source_path.clone(),
                lines: vec![],
                active_output_test_name: None,
            });
            let abs_suite_path = absolutize_repo_relative(&self.repo_root, &source_path);
            events.push(CargoTestStreamEvent::SuiteStarted {
                suite_path: abs_suite_path,
            });
            return events;
        }

        let Some(state) = self.current.as_mut() else {
            return events;
        };
        state.lines.push(line.to_string());

        if let Some(parsed) = parse_test_line_extended(line) {
            match parsed {
                ParsedTestLine::Completed { name, status } => {
                    state.active_output_test_name = None;
                    let abs_suite_path =
                        absolutize_repo_relative(&self.repo_root, &state.source_path);
                    events.push(CargoTestStreamEvent::TestFinished {
                        suite_path: abs_suite_path,
                        test_name: name,
                        status,
                    });
                    return events;
                }
                ParsedTestLine::Pending {
                    ..
                } => {
                    return events;
                }
            }
        }

        // Best-effort grouping of libtest output blocks:
        // `---- test_name stdout ----` begins a per-test output section.
        let trimmed = line.trim();
        if trimmed.starts_with("---- ") && trimmed.ends_with(" ----") {
            let name = trimmed
                .strip_prefix("---- ")
                .and_then(|s| s.strip_suffix(" ----"))
                .map(|s| s.trim().to_string());
            state.active_output_test_name = name.clone();
            let abs_suite_path = absolutize_repo_relative(&self.repo_root, &state.source_path);
            events.push(CargoTestStreamEvent::OutputLine {
                suite_path: abs_suite_path,
                test_name: name,
                line: trimmed.to_string(),
            });
            return events;
        }

        if !trimmed.is_empty() {
            let abs_suite_path = absolutize_repo_relative(&self.repo_root, &state.source_path);
            events.push(CargoTestStreamEvent::OutputLine {
                suite_path: abs_suite_path,
                test_name: state.active_output_test_name.clone(),
                line: line.to_string(),
            });
        }

        events
    }

    pub fn finalize(mut self) -> Option<TestRunModel> {
        self.flush_current_suite();
        (!self.suites.is_empty()).then(|| build_test_run_model(self.suites))
    }

    fn flush_current_suite(&mut self) {
        let Some(state) = self.current.take() else {
            return;
        };
        let suite = parse_suite_block(
            &self.repo_root,
            &SuiteBlock {
                source_path: state.source_path,
                lines: state.lines,
            },
        );
        if !suite.test_results.is_empty() {
            self.suites.push(suite);
        }
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

fn parse_suite_block(repo_root: &Path, block: &SuiteBlock) -> TestSuiteResult {
    let mut tests: Vec<TestCaseResult> = vec![];
    let mut failures_by_name: BTreeMap<String, String> = BTreeMap::new();
    let mut console_entries: Vec<TestConsoleEntry> = vec![];
    let mut last_pending_test_index: Option<usize> = None;

    let mut line_index: usize = 0;
    while line_index < block.lines.len() {
        let line = block.lines[line_index].as_str();
        if let Some(parsed) = parse_test_line_extended(line) {
            match parsed {
                ParsedTestLine::Completed { name, status } => {
                    last_pending_test_index = None;
                    tests.push(TestCaseResult {
                        title: name.clone(),
                        full_name: name,
                        status,
                        timed_out: None,
                        duration: 0,
                        location: None,
                        failure_messages: vec![],
                        failure_details: None,
                    });
                }
                ParsedTestLine::Pending {
                    name,
                    inline_output,
                } => {
                    tests.push(TestCaseResult {
                        title: name.clone(),
                        full_name: name,
                        status: "pending".to_string(),
                        timed_out: None,
                        duration: 0,
                        location: None,
                        failure_messages: vec![],
                        failure_details: None,
                    });
                    last_pending_test_index = Some(tests.len().saturating_sub(1));
                    if let Some(inline) = inline_output.as_deref().filter(|s| !s.trim().is_empty())
                    {
                        console_entries.push(TestConsoleEntry {
                            message: Some(serde_json::Value::String(inline.to_string())),
                            type_name: Some("log".to_string()),
                            origin: Some("cargo-test".to_string()),
                        });
                    }
                }
            }
            line_index += 1;
            continue;
        }
        if let Some(status) = parse_status_only_line(line) {
            if let Some(index) = last_pending_test_index.take()
                && let Some(test_case) = tests.get_mut(index)
            {
                test_case.status = status;
            };
            line_index += 1;
            continue;
        }
        if let Some((failed_name, consumed, failure_text)) =
            parse_failure_block(&block.lines, line_index)
        {
            failures_by_name.insert(failed_name, failure_text);
            line_index += consumed;
            continue;
        }
        if let Some((failed_name, consumed, failure_text)) =
            parse_panic_block(&block.lines, line_index)
        {
            failures_by_name.insert(failed_name, failure_text);
            line_index += consumed;
            continue;
        }
        if should_keep_as_console_line(line) {
            console_entries.push(TestConsoleEntry {
                message: Some(serde_json::Value::String(line.to_string())),
                type_name: Some("log".to_string()),
                origin: Some("cargo-test".to_string()),
            });
        }
        line_index += 1;
    }

    tests.iter_mut().for_each(|test_case| {
        if test_case.status == "failed" {
            test_case.failure_messages = failures_by_name
                .get(&test_case.full_name)
                .into_iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>();
        }
    });

    let (_, failed, _) = count_test_statuses(&tests);
    let status = if failed > 0 { "failed" } else { "passed" }.to_string();
    let abs_suite_path = absolutize_repo_relative(repo_root, &block.source_path);

    TestSuiteResult {
        test_file_path: abs_suite_path,
        status,
        timed_out: None,
        failure_message: String::new(),
        failure_details: None,
        test_exec_error: None,
        console: (!console_entries.is_empty()).then_some(console_entries),
        test_results: tests,
    }
}

fn should_keep_as_console_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    if parse_test_line(trimmed).is_some() {
        return false;
    }
    if parse_status_only_line(trimmed).is_some() {
        return false;
    }
    if parse_suite_header_source_path(trimmed).is_some() {
        return false;
    }
    if trimmed.starts_with("running ") {
        return false;
    }
    if trimmed.starts_with("---- ") && trimmed.ends_with(" ----") {
        return false;
    }
    if trimmed == "failures:" {
        return false;
    }
    if trimmed.starts_with("failures:") || trimmed.starts_with("test result:") {
        return false;
    }
    if trimmed.starts_with("thread '") || trimmed.contains("panicked at") {
        return false;
    }
    if trimmed.starts_with("stack backtrace:") || trimmed.starts_with("note:") {
        return false;
    }
    true
}

fn parse_test_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("test ")?;
    let (name, tail) = rest.split_once(" ... ")?;
    let status_raw = tail.trim();
    let status = match status_raw {
        "ok" => "passed",
        "FAILED" => "failed",
        "ignored" => "pending",
        _ => return None,
    };
    Some((name.trim().to_string(), status.to_string()))
}

fn parse_test_line_extended(line: &str) -> Option<ParsedTestLine> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("test ")?;
    let (name, tail) = rest.split_once(" ... ")?;
    let status_raw = tail.trim();
    if let Some((_, status)) = parse_test_line(trimmed) {
        return Some(ParsedTestLine::Completed {
            name: name.trim().to_string(),
            status,
        });
    }
    Some(ParsedTestLine::Pending {
        name: name.trim().to_string(),
        inline_output: (!status_raw.is_empty()).then(|| status_raw.to_string()),
    })
}

fn parse_status_only_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let status = match trimmed {
        "ok" => "passed",
        "FAILED" => "failed",
        "ignored" => "pending",
        _ => return None,
    };
    Some(status.to_string())
}

fn parse_panic_block(lines: &[String], start_index: usize) -> Option<(String, usize, String)> {
    let header = lines.get(start_index)?.trim();
    let thread_prefix = header.strip_prefix("thread '")?;
    let (test_name, _) = thread_prefix.split_once("' panicked at")?;

    let mut collected: Vec<String> = vec![header.to_string()];
    let mut consumed: usize = 1;
    for line in lines.iter().skip(start_index + 1) {
        let trimmed = line.trim();
        if parse_status_only_line(trimmed).is_some() {
            break;
        }
        if trimmed == "failures:" {
            break;
        }
        if trimmed.starts_with("test result:") {
            break;
        }
        collected.push(line.to_string());
        consumed += 1;
    }
    let text = collected.join("\n").trim().to_string();
    Some((test_name.trim().to_string(), consumed, text))
}

fn parse_failure_block(lines: &[String], start_index: usize) -> Option<(String, usize, String)> {
    let header = lines.get(start_index)?.trim();
    let rest = header.strip_prefix("---- ")?;
    let (test_name, _) = rest
        .split_once(" stdout ----")
        .or_else(|| rest.split_once(" stderr ----"))?;

    let mut collected: Vec<String> = vec![];
    let mut consumed: usize = 1;
    for line in lines.iter().skip(start_index + 1) {
        let trimmed = line.trim();
        if trimmed.starts_with("---- ") && trimmed.ends_with(" ----") {
            break;
        }
        if trimmed == "failures:" {
            break;
        }
        if trimmed.starts_with("test result:") {
            break;
        }
        collected.push(line.to_string());
        consumed += 1;
    }

    let text = collected.join("\n").trim().to_string();
    Some((test_name.trim().to_string(), consumed, text))
}

fn count_test_statuses(tests: &[TestCaseResult]) -> (u64, u64, u64) {
    let passed = tests.iter().filter(|t| t.status == "passed").count() as u64;
    let failed = tests.iter().filter(|t| t.status == "failed").count() as u64;
    let ignored = tests.iter().filter(|t| t.status == "pending").count() as u64;
    (passed, failed, ignored)
}

fn absolutize_repo_relative(repo_root: &Path, suite_source_path: &str) -> String {
    let mut buf = PathBuf::from(repo_root);
    buf.push(suite_source_path);
    buf.to_string_lossy().to_string()
}

fn build_test_run_model(suites: Vec<TestSuiteResult>) -> TestRunModel {
    let start_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let total_tests = suites
        .iter()
        .map(|s| s.test_results.len() as u64)
        .sum::<u64>();
    let passed_tests = suites
        .iter()
        .flat_map(|s| s.test_results.iter())
        .filter(|t| t.status == "passed")
        .count() as u64;
    let failed_tests = suites
        .iter()
        .flat_map(|s| s.test_results.iter())
        .filter(|t| t.status == "failed")
        .count() as u64;
    let pending_tests = suites
        .iter()
        .flat_map(|s| s.test_results.iter())
        .filter(|t| t.status == "pending")
        .count() as u64;

    let failed_suites = suites.iter().filter(|s| s.status == "failed").count() as u64;
    let total_suites = suites.len() as u64;
    let passed_suites = total_suites.saturating_sub(failed_suites);

    TestRunModel {
        start_time,
        test_results: suites,
        aggregated: TestRunAggregated {
            num_total_test_suites: total_suites,
            num_passed_test_suites: passed_suites,
            num_failed_test_suites: failed_suites,
            num_total_tests: total_tests,
            num_passed_tests: passed_tests,
            num_failed_tests: failed_tests,
            num_pending_tests: pending_tests,
            num_todo_tests: 0,
            num_timed_out_tests: None,
            num_timed_out_test_suites: None,
            start_time,
            success: failed_suites == 0 && failed_tests == 0,
            run_time_ms: None,
        },
    }
}
