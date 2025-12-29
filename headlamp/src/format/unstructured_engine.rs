use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::test_model::TestConsoleEntry;
use crate::test_model::{
    TestCaseResult, TestLocation, TestRunAggregated, TestRunModel, TestSuiteResult,
};

#[derive(Debug, Clone)]
pub enum UnstructuredStreamEvent {
    SuiteStarted {
        suite_path: String,
    },
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
pub enum ParsedTestLine {
    Completed {
        name: String,
        status: String,
    },
    Pending {
        name: String,
        inline_output: Option<String>,
    },
}

pub trait UnstructuredDialect {
    fn origin(&self) -> &'static str;

    fn parse_suite_header_source_path(&self, line: &str) -> Option<String>;

    fn parse_test_line(&self, line: &str) -> Option<ParsedTestLine>;

    fn parse_status_only_line(&self, line: &str) -> Option<String>;

    fn parse_failure_block(
        &self,
        lines: &[String],
        start_index: usize,
    ) -> Option<(String, usize, String)>;

    fn parse_panic_block(
        &self,
        lines: &[String],
        start_index: usize,
    ) -> Option<(String, usize, String)>;

    fn is_output_section_header(&self, line: &str) -> Option<String>;

    fn should_keep_as_console_line(&self, line: &str) -> bool;
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

#[derive(Debug, Default)]
struct SuiteParseAcc {
    tests: Vec<TestCaseResult>,
    failures_by_name: BTreeMap<String, String>,
    console_entries: Vec<TestConsoleEntry>,
    last_pending_test_index: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct UnstructuredStreamParser<D: UnstructuredDialect> {
    repo_root: PathBuf,
    dialect: D,
    suites: Vec<TestSuiteResult>,
    current: Option<SuiteState>,
}

impl<D: UnstructuredDialect> UnstructuredStreamParser<D> {
    pub fn new(repo_root: &Path, dialect: D) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
            dialect,
            suites: vec![],
            current: None,
        }
    }

    pub fn new_default(repo_root: &Path) -> Self
    where
        D: Default,
    {
        Self::new(repo_root, D::default())
    }

    pub fn push_line(&mut self, line: &str) -> Vec<UnstructuredStreamEvent> {
        let mut events: Vec<UnstructuredStreamEvent> = vec![];
        if let Some(source_path) = self.dialect.parse_suite_header_source_path(line) {
            self.flush_current_suite();
            self.current = Some(SuiteState {
                source_path: source_path.clone(),
                lines: vec![],
                active_output_test_name: None,
            });
            let abs_suite_path = absolutize_repo_relative(&self.repo_root, &source_path);
            events.push(UnstructuredStreamEvent::SuiteStarted {
                suite_path: abs_suite_path,
            });
            return events;
        }

        let Some(state) = self.current.as_mut() else {
            return events;
        };
        state.lines.push(line.to_string());

        if let Some(parsed) = self.dialect.parse_test_line(line) {
            match parsed {
                ParsedTestLine::Completed { name, status } => {
                    state.active_output_test_name = None;
                    let abs_suite_path =
                        absolutize_repo_relative(&self.repo_root, &state.source_path);
                    events.push(UnstructuredStreamEvent::TestFinished {
                        suite_path: abs_suite_path,
                        test_name: name,
                        status,
                    });
                    return events;
                }
                ParsedTestLine::Pending { .. } => {
                    return events;
                }
            }
        }

        if let Some(name) = self.dialect.is_output_section_header(line) {
            state.active_output_test_name = Some(name.clone());
            let abs_suite_path = absolutize_repo_relative(&self.repo_root, &state.source_path);
            events.push(UnstructuredStreamEvent::OutputLine {
                suite_path: abs_suite_path,
                test_name: Some(name),
                line: line.trim().to_string(),
            });
            return events;
        }

        if !line.trim().is_empty() {
            let abs_suite_path = absolutize_repo_relative(&self.repo_root, &state.source_path);
            events.push(UnstructuredStreamEvent::OutputLine {
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
        let SuiteState {
            source_path,
            lines,
            active_output_test_name: _,
        } = state;
        let suite = parse_suite_block(
            &self.repo_root,
            &self.dialect,
            &SuiteBlock {
                source_path,
                lines: lines.clone(),
            },
        );
        if !suite.test_results.is_empty() {
            self.suites.push(suite);
        }
    }
}

fn parse_suite_block<D: UnstructuredDialect>(
    repo_root: &Path,
    dialect: &D,
    block: &SuiteBlock,
) -> TestSuiteResult {
    let mut acc = parse_suite_lines(dialect, &block.lines);
    apply_failure_messages(&mut acc.tests, &acc.failures_by_name);
    apply_failure_locations(repo_root, &block.source_path, &mut acc.tests);
    let suite_failed = acc.tests.iter().any(|t| t.status == "failed");

    TestSuiteResult {
        test_file_path: absolutize_repo_relative(repo_root, &block.source_path),
        status: if suite_failed { "failed" } else { "passed" }.to_string(),
        timed_out: None,
        failure_message: String::new(),
        failure_details: None,
        test_exec_error: None,
        console: (!acc.console_entries.is_empty()).then_some(acc.console_entries),
        test_results: acc.tests,
    }
}

fn parse_suite_lines<D: UnstructuredDialect>(dialect: &D, lines: &[String]) -> SuiteParseAcc {
    let mut acc = SuiteParseAcc::default();
    let mut line_index: usize = 0;
    while line_index < lines.len() {
        line_index = parse_suite_step(dialect, lines, line_index, &mut acc);
    }
    acc
}

fn parse_suite_step<D: UnstructuredDialect>(
    dialect: &D,
    lines: &[String],
    line_index: usize,
    acc: &mut SuiteParseAcc,
) -> usize {
    let line = lines[line_index].as_str();
    dialect
        .parse_test_line(line)
        .map(|parsed| {
            apply_parsed_test_line(dialect, acc, parsed);
            line_index.saturating_add(1)
        })
        .or_else(|| {
            dialect.parse_status_only_line(line).map(|status| {
                apply_status_only(acc, status);
                line_index.saturating_add(1)
            })
        })
        .or_else(|| parse_failure_any(dialect, lines, line_index, acc))
        .unwrap_or_else(|| {
            maybe_push_console_line(dialect, acc, line);
            line_index.saturating_add(1)
        })
}

fn parse_failure_any<D: UnstructuredDialect>(
    dialect: &D,
    lines: &[String],
    line_index: usize,
    acc: &mut SuiteParseAcc,
) -> Option<usize> {
    dialect
        .parse_failure_block(lines, line_index)
        .or_else(|| dialect.parse_panic_block(lines, line_index))
        .map(|(failed_name, consumed, failure_text)| {
            acc.failures_by_name.insert(failed_name, failure_text);
            line_index.saturating_add(consumed)
        })
}

fn apply_parsed_test_line<D: UnstructuredDialect>(
    dialect: &D,
    acc: &mut SuiteParseAcc,
    parsed: ParsedTestLine,
) {
    match parsed {
        ParsedTestLine::Completed { name, status } => {
            acc.last_pending_test_index = None;
            acc.tests.push(empty_test_case(name, status));
        }
        ParsedTestLine::Pending {
            name,
            inline_output,
        } => {
            acc.tests
                .push(empty_test_case(name.clone(), "pending".to_string()));
            acc.last_pending_test_index = Some(acc.tests.len().saturating_sub(1));
            inline_output
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .into_iter()
                .for_each(|inline| {
                    acc.console_entries.push(TestConsoleEntry {
                        message: Some(serde_json::Value::String(inline.to_string())),
                        type_name: Some("log".to_string()),
                        origin: Some(dialect.origin().to_string()),
                    });
                });
        }
    }
}

fn apply_status_only(acc: &mut SuiteParseAcc, status: String) {
    if let Some(index) = acc.last_pending_test_index.take()
        && let Some(test_case) = acc.tests.get_mut(index)
    {
        test_case.status = status;
    };
}

fn maybe_push_console_line<D: UnstructuredDialect>(
    dialect: &D,
    acc: &mut SuiteParseAcc,
    line: &str,
) {
    if dialect.should_keep_as_console_line(line) {
        acc.console_entries.push(TestConsoleEntry {
            message: Some(serde_json::Value::String(line.to_string())),
            type_name: Some("log".to_string()),
            origin: Some(dialect.origin().to_string()),
        });
    }
}

fn empty_test_case(full_name: String, status: String) -> TestCaseResult {
    TestCaseResult {
        title: full_name.clone(),
        full_name,
        status,
        timed_out: None,
        duration: 0,
        location: None,
        failure_messages: vec![],
        failure_details: None,
    }
}

fn apply_failure_messages(
    tests: &mut [TestCaseResult],
    failures_by_name: &BTreeMap<String, String>,
) {
    tests.iter_mut().for_each(|test_case| {
        if test_case.status != "failed" {
            return;
        }
        if let Some(text) = failures_by_name.get(&test_case.full_name) {
            test_case.failure_messages = vec![text.clone()];
        }
    });
}

fn absolutize_repo_relative(repo_root: &Path, repo_relative: &str) -> String {
    let path = Path::new(repo_relative);
    if path.is_absolute() {
        return repo_relative.to_string();
    }
    let joined = repo_root.join(path);
    if joined.exists() {
        return joined.to_string_lossy().to_string();
    }
    crate::format::failure_diagnostics::resolve_existing_path_best_effort(
        &repo_root.to_string_lossy(),
        repo_relative,
    )
    .unwrap_or_else(|| joined.to_string_lossy().to_string())
}

fn apply_failure_locations(
    repo_root: &Path,
    suite_source_path: &str,
    tests: &mut [TestCaseResult],
) {
    let suite_abs = absolutize_repo_relative(repo_root, suite_source_path);
    let suite_file_name = Path::new(&suite_abs)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if suite_file_name.trim().is_empty() {
        return;
    }

    tests.iter_mut().for_each(|test_case| {
        if test_case.status != "failed" || test_case.location.is_some() {
            return;
        }
        let loc = test_case
            .failure_messages
            .iter()
            .flat_map(|msg| msg.lines())
            .find_map(crate::format::failure_diagnostics::parse_rust_panic_location)
            .and_then(|(file, line_number, col_number)| {
                crate::format::failure_diagnostics::resolve_existing_path_best_effort(
                    &repo_root.to_string_lossy(),
                    &file,
                )
                .map(|resolved| (resolved, line_number, col_number))
                .or(Some((file, line_number, col_number)))
            })
            .and_then(|(resolved_file, line_number, col_number)| {
                let matches_suite = Path::new(&resolved_file)
                    .file_name()
                    .is_some_and(|s| s.to_string_lossy() == suite_file_name);
                (matches_suite && line_number > 0 && col_number > 0).then_some(TestLocation {
                    line: line_number,
                    column: col_number,
                })
            });
        if let Some(loc) = loc {
            test_case.location = Some(loc);
        }
    });
}

fn build_test_run_model(suites: Vec<TestSuiteResult>) -> TestRunModel {
    let aggregated = suites.iter().fold(
        TestRunAggregated {
            num_total_test_suites: 0,
            num_passed_test_suites: 0,
            num_failed_test_suites: 0,
            num_total_tests: 0,
            num_passed_tests: 0,
            num_failed_tests: 0,
            num_pending_tests: 0,
            num_todo_tests: 0,
            num_timed_out_tests: None,
            num_timed_out_test_suites: None,
            start_time: 0,
            success: true,
            run_time_ms: Some(0),
        },
        |acc, suite| {
            let suite_failed = suite.status == "failed";
            let (passed_tests, failed_tests) =
                suite.test_results.iter().fold((0u64, 0u64), |(p, f), t| {
                    if t.status == "failed" {
                        (p, f.saturating_add(1))
                    } else {
                        (p.saturating_add(1), f)
                    }
                });
            TestRunAggregated {
                num_total_test_suites: acc.num_total_test_suites.saturating_add(1),
                num_passed_test_suites: acc
                    .num_passed_test_suites
                    .saturating_add((!suite_failed) as u64),
                num_failed_test_suites: acc
                    .num_failed_test_suites
                    .saturating_add(suite_failed as u64),
                num_total_tests: acc
                    .num_total_tests
                    .saturating_add(passed_tests.saturating_add(failed_tests)),
                num_passed_tests: acc.num_passed_tests.saturating_add(passed_tests),
                num_failed_tests: acc.num_failed_tests.saturating_add(failed_tests),
                success: acc.success && !suite_failed,
                ..acc
            }
        },
    );
    TestRunModel {
        start_time: 0,
        test_results: suites,
        aggregated,
    }
}
