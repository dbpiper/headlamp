use std::path::Path;

use headlamp_core::format::cargo_test::{CargoTestStreamEvent, CargoTestStreamParser};
use headlamp_core::format::nextest::{NextestStreamParser, NextestStreamUpdate};

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
        vec![StreamAction::SetProgressLabel(update.suite_path.clone())]
    }
}

#[derive(Debug)]
pub(super) struct CargoTestAdapter {
    pub(super) only_failures: bool,
    pub(super) parser: CargoTestStreamParser,
}

impl CargoTestAdapter {
    pub(super) fn new(repo_root: &Path, only_failures: bool) -> Self {
        Self {
            only_failures,
            parser: CargoTestStreamParser::new(repo_root),
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
            } => {
                if self.only_failures && status != "failed" {
                    return vec![];
                }
                if test_name.trim().is_empty() {
                    vec![StreamAction::SetProgressLabel(suite_path)]
                } else {
                    vec![StreamAction::SetProgressLabel(format!(
                        "{suite_path}::{test_name}"
                    ))]
                }
            }
            CargoTestStreamEvent::OutputLine {
                suite_path: _,
                test_name: _,
                line: _,
            } => vec![],
        }
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
