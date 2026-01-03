use std::path::Path;

use headlamp_core::format::cargo_test::{CargoTestStreamEvent, CargoTestStreamParser};
use headlamp_core::format::nextest::{NextestStreamParser, NextestStreamUpdate};

use crate::live_progress::{outcome_from_status, render_finished_test_line};
use crate::streaming::{OutputStream, StreamAction, StreamAdapter};

#[derive(Debug)]
pub(super) struct NextestAdapter {
    pub(super) only_failures: bool,
    pub(super) parser: NextestStreamParser,
}

impl NextestAdapter {
    pub(super) fn new(repo_root: &Path, only_failures: bool) -> Self {
        Self {
            only_failures,
            parser: NextestStreamParser::new(repo_root),
        }
    }

    fn actions_for_update(&self, update: &NextestStreamUpdate) -> Vec<StreamAction> {
        let should_print = !self.only_failures || update.status == "failed";
        if !should_print {
            return vec![];
        }
        let line = render_finished_test_line(
            outcome_from_status(update.status.as_str()),
            update.duration,
            update.suite_path.as_str(),
            update.test_name.as_str(),
        );
        vec![
            StreamAction::SetProgressLabel(update.suite_path.clone()),
            StreamAction::PrintStdout(line),
        ]
    }
}

#[derive(Debug)]
pub(super) struct CargoTestAdapter {
    pub(super) only_failures: bool,
    pub(super) parser: CargoTestStreamParser,
    last_pending_test_name: Option<String>,
    started_at_by_test: std::collections::BTreeMap<String, std::time::Instant>,
    current_suite_path: Option<String>,
}

impl CargoTestAdapter {
    pub(super) fn new(repo_root: &Path, only_failures: bool) -> Self {
        Self {
            only_failures,
            parser: CargoTestStreamParser::new(repo_root),
            last_pending_test_name: None,
            started_at_by_test: std::collections::BTreeMap::new(),
            current_suite_path: None,
        }
    }

    fn actions_for_event(&mut self, event: CargoTestStreamEvent) -> Vec<StreamAction> {
        match event {
            CargoTestStreamEvent::SuiteStarted { suite_path } => {
                self.current_suite_path = Some(suite_path.clone());
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
                        .map(|started_at| started_at.elapsed())
                });
                let line = render_finished_test_line(
                    outcome_from_status(status.as_str()),
                    duration,
                    suite_path.as_str(),
                    test_name.as_str(),
                );
                vec![
                    StreamAction::SetProgressLabel(format!("{suite_path}::{test_name}")),
                    StreamAction::PrintStdout(line),
                ]
            }
            CargoTestStreamEvent::OutputLine {
                suite_path: _,
                test_name: _,
                line: _,
            } => vec![],
        }
    }

    fn maybe_track_pending_test_start(&mut self, line: &str) {
        // libtest pretty output can emit:
        // - `test name ...` then later `ok` / `FAILED`
        // - or `test name ... ok` in one line
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
        if status.trim().is_empty() {
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
        let suite_path = self.current_suite_path.clone().unwrap_or_default();
        let line = render_finished_test_line(
            outcome_from_status(status),
            duration,
            suite_path.as_str(),
            name.as_str(),
        );
        vec![
            StreamAction::SetProgressLabel(format!("{suite_path}::{name}")),
            StreamAction::PrintStdout(line),
        ]
    }
}

impl StreamAdapter for CargoTestAdapter {
    fn on_start(&mut self) -> Option<String> {
        Some("cargo test".to_string())
    }

    fn on_line(&mut self, stream: OutputStream, line: &str) -> Vec<StreamAction> {
        let is_ci_env = std::env::var_os("CI").is_some();
        let is_tty_output = headlamp_core::format::terminal::is_output_terminal();
        let is_cargo_summary_error = line.starts_with("error: test failed, to rerun pass ")
            || line.starts_with("error: ") && line.contains(" target failed:");
        let has_useful_status = line.contains("Blocking waiting for file lock")
            || line.starts_with("Compiling ")
            || line.starts_with("Finished ")
            || line.starts_with("Running ")
            || line.starts_with("error:");
        if is_ci_env && stream == OutputStream::Stderr && is_cargo_summary_error {
            return vec![];
        }
        let mut actions: Vec<StreamAction> = vec![];
        self.maybe_track_pending_test_start(line);
        actions.extend(self.status_only_actions(line));
        if is_ci_env && !is_tty_output && stream == OutputStream::Stderr && has_useful_status {
            actions.extend([
                StreamAction::SetProgressLabel(format!("cargo: {}", line.trim())),
                StreamAction::PrintStderr(line.trim().to_string()),
            ]);
        }
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

impl StreamAdapter for NextestAdapter {
    fn on_start(&mut self) -> Option<String> {
        Some("cargo nextest".to_string())
    }

    fn on_line(&mut self, stream: OutputStream, line: &str) -> Vec<StreamAction> {
        let is_ci_env = std::env::var_os("CI").is_some();
        let is_tty_output = headlamp_core::format::terminal::is_output_terminal();
        let is_nextest_summary_error = line.trim() == "error: test run failed";
        let has_useful_status = line.contains("Blocking waiting for file lock")
            || line.starts_with("Compiling ")
            || line.starts_with("Finished ")
            || line.starts_with("Running ")
            || line.starts_with("error:");
        if is_ci_env && stream == OutputStream::Stderr && is_nextest_summary_error {
            return vec![];
        }
        let mut actions: Vec<StreamAction> = vec![];
        if is_ci_env && !is_tty_output && stream == OutputStream::Stderr && has_useful_status {
            actions.extend([
                StreamAction::SetProgressLabel(format!("cargo: {}", line.trim())),
                StreamAction::PrintStderr(line.trim().to_string()),
            ]);
        }
        let update = self.parser.push_line(line);
        actions.extend(
            update
                .as_ref()
                .map(|u| self.actions_for_update(u))
                .unwrap_or_default(),
        );
        actions
    }
}
