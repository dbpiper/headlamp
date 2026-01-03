use std::collections::BTreeMap;

use headlamp_core::test_model::{TestConsoleEntry, TestRunModel};

use crate::live_progress::{outcome_from_status, render_finished_test_line};
use crate::streaming::{OutputStream, StreamAction, StreamAdapter};

#[derive(Debug)]
pub(super) struct JestStreamingAdapter {
    pub(super) emit_raw_lines: bool,
    pub(super) only_failures: bool,
    pub(super) captured_stdout: Vec<String>,
    pub(super) captured_stderr: Vec<String>,
    pub(super) extra_bridge_entries_by_test_path: BTreeMap<String, Vec<TestConsoleEntry>>,
}

impl JestStreamingAdapter {
    pub(super) fn new(emit_raw_lines: bool, only_failures: bool) -> Self {
        Self {
            emit_raw_lines,
            only_failures,
            captured_stdout: vec![],
            captured_stderr: vec![],
            extra_bridge_entries_by_test_path: BTreeMap::new(),
        }
    }

    fn push_non_event_line(&mut self, stream: OutputStream, line: &str) {
        match stream {
            OutputStream::Stdout => self.captured_stdout.push(line.to_string()),
            OutputStream::Stderr => self.captured_stderr.push(line.to_string()),
        }
    }

    fn actions_for_bridge_event_line(&mut self, line: &str) -> Vec<StreamAction> {
        let Some(payload) = line.strip_prefix("[JEST-BRIDGE-EVENT] ") else {
            return vec![];
        };
        let event = serde_json::from_str::<JestBridgeEvent>(payload).ok();
        let test_path = event
            .as_ref()
            .and_then(|m| m.test_path.as_deref())
            .unwrap_or("")
            .replace('\\', "/");
        if !test_path.trim().is_empty() {
            self.extra_bridge_entries_by_test_path
                .entry(test_path.clone())
                .or_default()
                .push(TestConsoleEntry {
                    message: Some(serde_json::Value::String(format!(
                        "[JEST-BRIDGE-EVENT] {payload}"
                    ))),
                    type_name: None,
                    origin: None,
                });
        }
        let Some(event) = event else {
            return vec![];
        };
        if event.type_name != "caseComplete" {
            return vec![];
        }
        let Some(full_name) = event
            .full_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            return vec![];
        };
        let Some(status) = event
            .status
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            return vec![];
        };
        if self.only_failures && !status.eq_ignore_ascii_case("failed") {
            return vec![];
        }
        let duration = event.duration_ms.map(std::time::Duration::from_millis);
        let line = render_finished_test_line(
            outcome_from_status(status),
            duration,
            test_path.as_str(),
            full_name,
        );
        vec![StreamAction::PrintStdout(line)]
    }
}

impl StreamAdapter for JestStreamingAdapter {
    fn on_start(&mut self) -> Option<String> {
        Some("jest".to_string())
    }

    fn on_line(&mut self, stream: OutputStream, line: &str) -> Vec<StreamAction> {
        if line.starts_with("[JEST-BRIDGE-EVENT] ") {
            return self.actions_for_bridge_event_line(line);
        }

        self.push_non_event_line(stream, line);

        if !self.emit_raw_lines {
            return vec![];
        }
        match stream {
            OutputStream::Stdout => vec![StreamAction::PrintStdout(line.to_string())],
            OutputStream::Stderr => vec![StreamAction::PrintStderr(line.to_string())],
        }
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct JestBridgeEvent {
    #[serde(rename = "type")]
    type_name: String,
    #[serde(rename = "testPath")]
    test_path: Option<String>,
    #[serde(rename = "fullName")]
    full_name: Option<String>,
    status: Option<String>,
    #[serde(rename = "duration")]
    duration_ms: Option<u64>,
}

pub(super) fn merge_console_entries_into_bridge_json(
    bridge: &mut TestRunModel,
    extra_console_by_test_path: &BTreeMap<String, Vec<TestConsoleEntry>>,
) {
    bridge.test_results.iter_mut().for_each(|file| {
        let key = file.test_file_path.replace('\\', "/");
        let Some(extra) = extra_console_by_test_path.get(&key) else {
            return;
        };
        if extra.is_empty() {
            return;
        }
        match file.console.as_mut() {
            Some(existing) => existing.extend(extra.iter().cloned()),
            None => file.console = Some(extra.clone()),
        }
    });
}
