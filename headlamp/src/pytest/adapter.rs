use std::collections::BTreeMap;

use headlamp_core::test_model::{
    TestConsoleEntry, TestRunAggregated, TestRunModel, TestSuiteResult,
};

use crate::streaming::{OutputStream, StreamAction, StreamAdapter};

const PYTEST_EVENT_PREFIX: &str = "HEADLAMP_PYTEST_EVENT ";

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PytestCaseEvent {
    #[serde(rename = "type")]
    type_name: String,
    nodeid: String,
    outcome: Option<String>,
    duration: Option<f64>,
    stdout: Option<String>,
    stderr: Option<String>,
    longrepr: Option<String>,
}

#[derive(Debug, Default)]
struct SuiteBuilder {
    test_file_path: String,
    cases: Vec<headlamp_core::test_model::TestCaseResult>,
    console: Vec<TestConsoleEntry>,
}

#[derive(Debug, Default)]
pub(super) struct PytestAdapter {
    show_logs: bool,
    emit_raw_lines: bool,
    suites: BTreeMap<String, SuiteBuilder>,
}

impl PytestAdapter {
    pub(super) fn new(show_logs: bool, emit_raw_lines: bool) -> Self {
        Self {
            show_logs,
            emit_raw_lines,
            suites: BTreeMap::new(),
        }
    }

    fn push_event(&mut self, event: PytestCaseEvent) {
        if event.type_name != "case" {
            return;
        }
        let file = event.nodeid.split("::").next().unwrap_or("").to_string();
        let title = event
            .nodeid
            .split("::")
            .last()
            .unwrap_or(event.nodeid.as_str())
            .to_string();
        let duration_ms = ((event.duration.unwrap_or(0.0)).max(0.0) * 1000.0) as u64;
        let location = event
            .longrepr
            .as_deref()
            .and_then(|lr| super::infer_test_location_from_pytest_longrepr(&file, lr));
        let failure_messages = event
            .longrepr
            .as_ref()
            .filter(|s| !s.trim().is_empty())
            .map(|s| vec![s.clone()])
            .unwrap_or_default();
        let case = headlamp_core::test_model::TestCaseResult {
            title: title.clone(),
            full_name: title.clone(),
            status: event
                .outcome
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            timed_out: None,
            duration: duration_ms,
            location,
            failure_messages,
            failure_details: None,
        };
        let suite = self
            .suites
            .entry(file.clone())
            .or_insert_with(|| SuiteBuilder {
                test_file_path: file.clone(),
                cases: vec![],
                console: vec![],
            });
        suite.cases.push(case);
        if self.show_logs {
            let stdout = event.stdout.unwrap_or_default();
            let stderr = event.stderr.unwrap_or_default();
            if !stdout.trim().is_empty() {
                suite.console.push(TestConsoleEntry {
                    message: Some(serde_json::Value::String(stdout)),
                    type_name: Some("log".to_string()),
                    origin: Some(title.clone()),
                });
            }
            if !stderr.trim().is_empty() {
                suite.console.push(TestConsoleEntry {
                    message: Some(serde_json::Value::String(stderr)),
                    type_name: Some("error".to_string()),
                    origin: Some(title),
                });
            }
        }
    }

    pub(super) fn finalize(self, exit_code: i32) -> TestRunModel {
        let mut test_results: Vec<TestSuiteResult> = self
            .suites
            .into_values()
            .map(|suite| {
                let any_failed = suite
                    .cases
                    .iter()
                    .any(|c| c.status.eq_ignore_ascii_case("failed"));
                let status = if any_failed { "failed" } else { "passed" }.to_string();
                let failure_message = suite
                    .cases
                    .iter()
                    .flat_map(|c| c.failure_messages.iter())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n");
                TestSuiteResult {
                    test_file_path: suite.test_file_path,
                    status,
                    timed_out: None,
                    failure_message,
                    failure_details: None,
                    test_exec_error: None,
                    console: (!suite.console.is_empty()).then_some(suite.console),
                    test_results: suite.cases,
                }
            })
            .collect();
        test_results.sort_by(|a, b| a.test_file_path.cmp(&b.test_file_path));

        let num_total_test_suites = test_results.len() as u64;
        let num_failed_test_suites =
            test_results.iter().filter(|s| s.status == "failed").count() as u64;
        let num_passed_test_suites = num_total_test_suites.saturating_sub(num_failed_test_suites);
        let num_total_tests = test_results
            .iter()
            .map(|s| s.test_results.len() as u64)
            .sum::<u64>();
        let num_failed_tests = test_results
            .iter()
            .flat_map(|s| s.test_results.iter())
            .filter(|c| c.status.eq_ignore_ascii_case("failed"))
            .count() as u64;
        let num_passed_tests = num_total_tests.saturating_sub(num_failed_tests);

        TestRunModel {
            start_time: 0,
            test_results,
            aggregated: TestRunAggregated {
                num_total_test_suites,
                num_passed_test_suites,
                num_failed_test_suites,
                num_total_tests,
                num_passed_tests,
                num_failed_tests,
                num_pending_tests: 0,
                num_todo_tests: 0,
                num_timed_out_tests: None,
                num_timed_out_test_suites: None,
                start_time: 0,
                success: exit_code == 0,
                run_time_ms: None,
            },
        }
    }
}

impl StreamAdapter for PytestAdapter {
    fn on_start(&mut self) -> Option<String> {
        Some("pytest".to_string())
    }

    fn on_line(&mut self, stream: OutputStream, line: &str) -> Vec<StreamAction> {
        let mut actions: Vec<StreamAction> = vec![];
        if self.emit_raw_lines {
            match stream {
                OutputStream::Stdout => actions.push(StreamAction::PrintStdout(line.to_string())),
                OutputStream::Stderr => actions.push(StreamAction::PrintStderr(line.to_string())),
            }
        }
        let Some((_prefix, json)) = line.split_once(PYTEST_EVENT_PREFIX) else {
            return actions;
        };
        let event = serde_json::from_str::<PytestCaseEvent>(json).ok();
        if let Some(evt) = event {
            if evt.type_name == "case_start" {
                if !evt.nodeid.trim().is_empty() {
                    actions.push(StreamAction::SetProgressLabel(
                        evt.nodeid.trim().to_string(),
                    ));
                }
                return actions;
            }
            self.push_event(evt);
        }
        actions
    }
}
