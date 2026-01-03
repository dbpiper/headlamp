use std::path::Path;

use crate::format::cargo_test::{CargoTestStreamEvent, CargoTestStreamParser};
use crate::format::libtest_json::{LibtestJsonStreamParser, LibtestJsonStreamUpdate};
use crate::live_progress::{outcome_from_status, render_finished_test_line};
use crate::streaming::{OutputStream, StreamAction, StreamAdapter};
use crate::test_model::{TestRunAggregated, TestRunModel, TestSuiteResult};

#[derive(Debug)]
pub(crate) struct DirectLibtestAdapter {
    only_failures: bool,
    pub(crate) parser: CargoTestStreamParser,
    suite_source_path: String,
    suite_path_display: String,
    last_pending_test_name: Option<String>,
    started_at_by_test: std::collections::BTreeMap<String, std::time::Instant>,
}

#[derive(Debug)]
pub(crate) struct LibtestJsonAdapter {
    only_failures: bool,
    pub(crate) parser: LibtestJsonStreamParser,
    suite_source_path: String,
    suite_path_display: String,
}

impl LibtestJsonAdapter {
    pub(crate) fn new(repo_root: &Path, only_failures: bool, suite_source_path: &str) -> Self {
        let suite_path_display = repo_root
            .join(suite_source_path)
            .to_string_lossy()
            .to_string();
        Self {
            only_failures,
            parser: LibtestJsonStreamParser::new(repo_root, suite_source_path),
            suite_source_path: suite_source_path.to_string(),
            suite_path_display,
        }
    }

    fn actions_for_update(&self, update: LibtestJsonStreamUpdate) -> Vec<StreamAction> {
        if self.only_failures && update.status != "failed" {
            return vec![];
        }
        let line = render_finished_test_line(
            outcome_from_status(update.status.as_str()),
            update.duration,
            self.suite_path_display.as_str(),
            update.test_name.as_str(),
        );
        vec![
            StreamAction::SetProgressLabel(format!(
                "{}::{}",
                self.suite_path_display, update.test_name
            )),
            StreamAction::PrintStdout(line),
        ]
    }
}

impl DirectLibtestAdapter {
    pub(crate) fn new(repo_root: &Path, only_failures: bool, suite_source_path: &str) -> Self {
        let mut parser = CargoTestStreamParser::new(repo_root);
        let suite_line = format!("Running {suite_source_path}");
        let _ = parser.push_line(&suite_line);
        let suite_path_display = repo_root
            .join(suite_source_path)
            .to_string_lossy()
            .to_string();
        Self {
            only_failures,
            parser,
            suite_source_path: suite_source_path.to_string(),
            suite_path_display,
            last_pending_test_name: None,
            started_at_by_test: std::collections::BTreeMap::new(),
        }
    }

    fn actions_for_event(&mut self, event: CargoTestStreamEvent) -> Vec<StreamAction> {
        match event {
            CargoTestStreamEvent::SuiteStarted { suite_path } => {
                vec![StreamAction::SetProgressLabel(suite_path)]
            }
            CargoTestStreamEvent::TestFinished {
                suite_path,
                test_name,
                status,
                duration,
            } => {
                if self.only_failures && status != "failed" {
                    return vec![];
                }
                let duration = duration.or_else(|| {
                    self.started_at_by_test
                        .remove(test_name.as_str())
                        .map(|t| t.elapsed())
                });
                let line = render_finished_test_line(
                    outcome_from_status(status.as_str()),
                    duration,
                    suite_path.as_str(),
                    test_name.as_str(),
                );
                if test_name.trim().is_empty() {
                    vec![StreamAction::SetProgressLabel(suite_path)]
                } else {
                    vec![
                        StreamAction::SetProgressLabel(format!("{suite_path}::{test_name}")),
                        StreamAction::PrintStdout(line),
                    ]
                }
            }
            CargoTestStreamEvent::OutputLine { .. } => vec![],
        }
    }

    fn maybe_track_pending_test_start(&mut self, line: &str) {
        let trimmed = line.trim();
        if !trimmed.starts_with("test ") {
            return;
        }
        let rest = trimmed.strip_prefix("test ").unwrap_or(trimmed);
        let Some((name, status)) = rest.split_once(" ... ") else {
            return;
        };
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let status_trimmed = status.trim();
        let is_terminal_status =
            status_trimmed == "ok" || status_trimmed == "FAILED" || status_trimmed == "ignored";
        if status_trimmed.is_empty() || !is_terminal_status {
            self.started_at_by_test
                .entry(name.to_string())
                .or_insert_with(std::time::Instant::now);
            self.last_pending_test_name = Some(name.to_string());
        }
    }

    fn status_only_actions(&mut self, line: &str) -> Vec<StreamAction> {
        let trimmed = line.trim();
        if trimmed != "ok" && trimmed != "FAILED" {
            return vec![];
        }
        let Some(name) = self.last_pending_test_name.take() else {
            return vec![];
        };
        let duration = self
            .started_at_by_test
            .remove(name.as_str())
            .map(|t| t.elapsed());
        let status = if trimmed == "ok" { "passed" } else { "failed" };
        if self.only_failures && status != "failed" {
            return vec![];
        }
        let line = render_finished_test_line(
            outcome_from_status(status),
            duration,
            self.suite_path_display.as_str(),
            name.as_str(),
        );
        vec![
            StreamAction::SetProgressLabel(format!("{}::{name}", self.suite_path_display)),
            StreamAction::PrintStdout(line),
        ]
    }
}

impl StreamAdapter for DirectLibtestAdapter {
    fn on_start(&mut self) -> Option<String> {
        Some(format!("rust: {}", self.suite_source_path))
    }

    fn on_line(&mut self, _stream: OutputStream, line: &str) -> Vec<StreamAction> {
        let mut actions: Vec<StreamAction> = vec![];
        self.maybe_track_pending_test_start(line);
        let trimmed = line.trim();
        if (trimmed == "ok" || trimmed == "FAILED") && self.last_pending_test_name.is_some() {
            let name = self.last_pending_test_name.take().unwrap_or_default();
            let duration = self
                .started_at_by_test
                .remove(name.as_str())
                .map(|t| t.elapsed());
            let status = if trimmed == "ok" { "passed" } else { "failed" };
            if !(self.only_failures && status != "failed") {
                let line = render_finished_test_line(
                    outcome_from_status(status),
                    duration,
                    self.suite_path_display.as_str(),
                    name.as_str(),
                );
                actions.push(StreamAction::SetProgressLabel(format!(
                    "{}::{name}",
                    self.suite_path_display
                )));
                actions.push(StreamAction::PrintStdout(line));
            }
            actions.extend(
                self.parser
                    .push_line(line)
                    .into_iter()
                    .flat_map(|evt| self.actions_for_event(evt))
                    .collect::<Vec<_>>(),
            );
            return actions;
        }
        actions.extend(self.status_only_actions(line));
        actions.extend(
            self.parser
                .push_line(line)
                .into_iter()
                .flat_map(|evt| self.actions_for_event(evt))
                .collect::<Vec<_>>(),
        );
        actions
    }
}

impl StreamAdapter for LibtestJsonAdapter {
    fn on_start(&mut self) -> Option<String> {
        Some(format!("rust: {}", self.suite_source_path))
    }

    fn on_line(&mut self, _stream: OutputStream, line: &str) -> Vec<StreamAction> {
        self.parser
            .push_line(line)
            .map(|u| self.actions_for_update(u))
            .unwrap_or_default()
    }
}

pub(crate) fn build_run_model(suites: Vec<TestSuiteResult>, run_time_ms: u64) -> TestRunModel {
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
            run_time_ms: Some(run_time_ms),
        },
        |acc, suite| {
            let suite_failed = suite.status == "failed";
            let (passed_tests, failed_tests, pending_tests) =
                suite
                    .test_results
                    .iter()
                    .fold((0u64, 0u64, 0u64), |(p, f, n), t| match t.status.as_str() {
                        "failed" => (p, f.saturating_add(1), n),
                        "pending" => (p, f, n.saturating_add(1)),
                        _ => (p.saturating_add(1), f, n),
                    });
            TestRunAggregated {
                num_total_test_suites: acc.num_total_test_suites.saturating_add(1),
                num_passed_test_suites: acc
                    .num_passed_test_suites
                    .saturating_add((!suite_failed) as u64),
                num_failed_test_suites: acc
                    .num_failed_test_suites
                    .saturating_add(suite_failed as u64),
                num_total_tests: acc.num_total_tests.saturating_add(
                    passed_tests
                        .saturating_add(failed_tests)
                        .saturating_add(pending_tests),
                ),
                num_passed_tests: acc.num_passed_tests.saturating_add(passed_tests),
                num_failed_tests: acc.num_failed_tests.saturating_add(failed_tests),
                num_pending_tests: acc.num_pending_tests.saturating_add(pending_tests),
                success: acc.success && !suite_failed,
                run_time_ms: Some(run_time_ms),
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
