use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::test_model::TestConsoleEntry;
use crate::test_model::{TestCaseResult, TestRunAggregated, TestRunModel, TestSuiteResult};

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
        let suite = parse_suite_block(
            &self.repo_root,
            &self.dialect,
            &SuiteBlock {
                source_path: state.source_path,
                lines: state.lines,
            },
        );
        let has_any_tests = !suite.test_results.is_empty();
        if has_any_tests {
            self.suites.push(suite);
        }
    }
}

fn parse_suite_block<D: UnstructuredDialect>(
    repo_root: &Path,
    dialect: &D,
    block: &SuiteBlock,
) -> TestSuiteResult {
    let mut tests: Vec<TestCaseResult> = vec![];
    let mut failures_by_name: BTreeMap<String, String> = BTreeMap::new();
    let mut console_entries: Vec<TestConsoleEntry> = vec![];
    let mut last_pending_test_index: Option<usize> = None;

    let mut line_index: usize = 0;
    while line_index < block.lines.len() {
        let line = block.lines[line_index].as_str();
        if let Some(parsed) = dialect.parse_test_line(line) {
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
                            origin: Some(dialect.origin().to_string()),
                        });
                    }
                }
            }
            line_index += 1;
            continue;
        }

        if let Some(status) = dialect.parse_status_only_line(line) {
            if let Some(index) = last_pending_test_index.take()
                && let Some(test_case) = tests.get_mut(index)
            {
                test_case.status = status;
            };
            line_index += 1;
            continue;
        }

        if let Some((failed_name, consumed, failure_text)) =
            dialect.parse_failure_block(&block.lines, line_index)
        {
            failures_by_name.insert(failed_name, failure_text);
            line_index += consumed;
            continue;
        }

        if let Some((failed_name, consumed, failure_text)) =
            dialect.parse_panic_block(&block.lines, line_index)
        {
            failures_by_name.insert(failed_name, failure_text);
            line_index += consumed;
            continue;
        }

        if dialect.should_keep_as_console_line(line) {
            console_entries.push(TestConsoleEntry {
                message: Some(serde_json::Value::String(line.to_string())),
                type_name: Some("log".to_string()),
                origin: Some(dialect.origin().to_string()),
            });
        }

        line_index += 1;
    }

    tests.iter_mut().for_each(|test_case| {
        if test_case.status == "failed" {
            if let Some(text) = failures_by_name.get(&test_case.full_name) {
                test_case.failure_messages = vec![text.clone()];
            }
        }
    });

    let (_num_total_tests, num_failed_tests) = tests.iter().fold((0u64, 0u64), |acc, t| {
        let (tot, fail) = acc;
        let tot2 = tot.saturating_add(1);
        let fail2 = if t.status == "failed" {
            fail.saturating_add(1)
        } else {
            fail
        };
        (tot2, fail2)
    });
    let suite_failed = num_failed_tests > 0;

    TestSuiteResult {
        test_file_path: absolutize_repo_relative(repo_root, &block.source_path),
        status: if suite_failed { "failed" } else { "passed" }.to_string(),
        timed_out: None,
        failure_message: String::new(),
        failure_details: None,
        test_exec_error: None,
        console: (!console_entries.is_empty()).then_some(console_entries),
        test_results: tests,
    }
}

fn absolutize_repo_relative(repo_root: &Path, repo_relative: &str) -> String {
    let path = Path::new(repo_relative);
    if path.is_absolute() {
        return repo_relative.to_string();
    }
    repo_root.join(path).to_string_lossy().to_string()
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
