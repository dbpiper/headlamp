use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::test_model::{TestCaseResult, TestLocation, TestRunModel, TestSuiteResult};

#[derive(Debug, Clone)]
pub struct LibtestJsonStreamUpdate {
    pub test_name: String,
    pub status: String,
    pub duration: Option<Duration>,
    pub stdout: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
enum LibtestJsonEvent {
    #[serde(rename = "test")]
    Test {
        event: String,
        name: String,
        exec_time: Option<f64>,
        stdout: Option<String>,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone)]
pub struct LibtestJsonStreamParser {
    repo_root: PathBuf,
    suite_source_path: String,
    tests_by_name: BTreeMap<String, TestCaseResult>,
}

impl LibtestJsonStreamParser {
    pub fn new(repo_root: &Path, suite_source_path: &str) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
            suite_source_path: suite_source_path.to_string(),
            tests_by_name: BTreeMap::new(),
        }
    }

    pub fn push_line(&mut self, line: &str) -> Option<LibtestJsonStreamUpdate> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        if !(trimmed.starts_with('{') && trimmed.ends_with('}')) {
            return None;
        }
        let event = serde_json::from_str::<LibtestJsonEvent>(trimmed).ok()?;
        match event {
            LibtestJsonEvent::Test {
                event,
                name,
                exec_time,
                stdout,
            } => self.handle_test_event(event, name, exec_time, stdout),
            LibtestJsonEvent::Other => None,
        }
    }

    pub fn finalize(self) -> Option<TestRunModel> {
        let test_file_path = self.repo_root.join(&self.suite_source_path);
        let test_file_path = test_file_path.to_string_lossy().to_string();
        let total_tests = self.tests_by_name.len() as u64;
        let tests = self.tests_by_name.into_values().collect::<Vec<_>>();
        if tests.is_empty() {
            return None;
        }
        let failed_tests = tests.iter().filter(|t| t.status == "failed").count() as u64;
        let passed_tests = tests.iter().filter(|t| t.status == "passed").count() as u64;
        let pending_tests = tests.iter().filter(|t| t.status == "pending").count() as u64;
        let failed = failed_tests as usize;
        let status = if failed > 0 { "failed" } else { "passed" }.to_string();
        Some(TestRunModel {
            start_time: 0,
            test_results: vec![TestSuiteResult {
                test_file_path,
                status,
                timed_out: None,
                failure_message: String::new(),
                failure_details: None,
                test_exec_error: None,
                console: None,
                test_results: tests,
            }],
            aggregated: crate::test_model::TestRunAggregated {
                num_total_test_suites: 1,
                num_passed_test_suites: (failed == 0) as u64,
                num_failed_test_suites: (failed > 0) as u64,
                num_total_tests: total_tests,
                num_passed_tests: passed_tests,
                num_failed_tests: failed_tests,
                num_pending_tests: pending_tests,
                num_todo_tests: 0,
                num_timed_out_tests: None,
                num_timed_out_test_suites: None,
                start_time: 0,
                success: failed == 0,
                run_time_ms: None,
            },
        })
    }

    fn handle_test_event(
        &mut self,
        event: String,
        name: String,
        exec_time: Option<f64>,
        stdout: Option<String>,
    ) -> Option<LibtestJsonStreamUpdate> {
        let status = match event.as_str() {
            "ok" => "passed",
            "failed" => "failed",
            "ignored" => "pending",
            _ => return None,
        }
        .to_string();

        let duration = exec_time
            .filter(|sec| *sec >= 0.0)
            .map(Duration::from_secs_f64);
        let duration_ms = duration.map(|d| d.as_millis() as u64).unwrap_or(0);

        let mut test_case =
            self.tests_by_name
                .remove(name.as_str())
                .unwrap_or_else(|| TestCaseResult {
                    title: name.clone(),
                    full_name: name.clone(),
                    status: "pending".to_string(),
                    timed_out: None,
                    duration: 0,
                    location: None,
                    failure_messages: vec![],
                    failure_details: None,
                });

        test_case.status = status.clone();
        test_case.duration = duration_ms;

        if test_case.status == "failed" {
            if let Some(out) = stdout.as_deref().filter(|s| !s.trim().is_empty()) {
                test_case.failure_messages = vec![out.to_string()];
                test_case.location = parse_location_if_matches_suite(out, &self.suite_source_path);
            }
        }

        self.tests_by_name.insert(name.clone(), test_case);

        Some(LibtestJsonStreamUpdate {
            test_name: name,
            status,
            duration,
            stdout,
        })
    }
}

fn parse_location_if_matches_suite(stdout: &str, suite_source_path: &str) -> Option<TestLocation> {
    let suite_file_name = Path::new(suite_source_path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if suite_file_name.trim().is_empty() {
        return None;
    }
    stdout
        .lines()
        .find_map(crate::format::failure_diagnostics::parse_rust_panic_location)
        .and_then(|(file, line_number, col_number)| {
            let matches_suite = Path::new(&file)
                .file_name()
                .is_some_and(|s| s.to_string_lossy() == suite_file_name);
            (matches_suite && line_number > 0 && col_number > 0).then_some(TestLocation {
                line: line_number,
                column: col_number,
            })
        })
}
