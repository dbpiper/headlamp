use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::test_model::{
    TestCaseResult, TestConsoleEntry, TestRunAggregated, TestRunModel, TestSuiteResult,
};

#[derive(Debug, Clone)]
pub struct NextestStreamUpdate {
    pub suite_path: String,
    pub test_name: String,
    pub status: String,
    pub stdout: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SuiteKey {
    crate_name: String,
    test_binary: String,
    kind: String,
}

#[derive(Debug, Clone, Deserialize)]
struct NextestMeta {
    #[serde(rename = "crate")]
    crate_name: String,
    test_binary: String,
    kind: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
enum NextestEvent {
    #[serde(rename = "suite")]
    Suite {
        event: String,
        passed: Option<u64>,
        failed: Option<u64>,
        ignored: Option<u64>,
        measured: Option<u64>,
        filtered_out: Option<u64>,
        nextest: Option<NextestMeta>,
    },
    #[serde(rename = "test")]
    Test {
        event: String,
        name: String,
        exec_time: Option<f64>,
        stdout: Option<String>,
    },
}

#[derive(Debug, Clone)]
struct SuiteAcc {
    key: SuiteKey,
    tests: BTreeMap<String, TestCaseResult>,
    console_entries: Vec<TestConsoleEntry>,
}

#[derive(Debug, Clone)]
pub struct NextestStreamParser {
    repo_root: PathBuf,
    suites_by_key: BTreeMap<SuiteKey, SuiteAcc>,
    kind_by_crate_and_binary: BTreeMap<(String, String), String>,
    loose_log_lines: Vec<String>,
}

impl NextestStreamParser {
    pub fn new(repo_root: &Path) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
            suites_by_key: BTreeMap::new(),
            kind_by_crate_and_binary: BTreeMap::new(),
            loose_log_lines: vec![],
        }
    }

    pub fn push_line(&mut self, line: &str) -> Option<NextestStreamUpdate> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        if !(trimmed.starts_with('{') && trimmed.ends_with('}')) {
            self.loose_log_lines.push(trimmed.to_string());
            return None;
        }
        let Ok(event) = serde_json::from_str::<NextestEvent>(trimmed) else {
            return None;
        };
        match event {
            NextestEvent::Suite {
                event,
                nextest,
                passed,
                failed,
                ignored,
                measured,
                filtered_out,
                ..
            } => {
                let Some(meta) = nextest else {
                    return None;
                };
                let key = SuiteKey {
                    crate_name: meta.crate_name,
                    test_binary: meta.test_binary,
                    kind: meta.kind,
                };
                self.kind_by_crate_and_binary.insert(
                    (key.crate_name.clone(), key.test_binary.clone()),
                    key.kind.clone(),
                );
                match event.as_str() {
                    "started" => {
                        self.suites_by_key
                            .entry(key.clone())
                            .or_insert_with(|| SuiteAcc {
                                key,
                                tests: BTreeMap::new(),
                                console_entries: vec![],
                            });
                    }
                    "ok" | "failed" => {
                        let _counts = (passed, failed, ignored, measured, filtered_out);
                        self.suites_by_key
                            .entry(key.clone())
                            .or_insert_with(|| SuiteAcc {
                                key: key.clone(),
                                tests: BTreeMap::new(),
                                console_entries: vec![],
                            });
                    }
                    _ => {}
                }
                None
            }
            NextestEvent::Test {
                event,
                name,
                exec_time,
                stdout,
            } => {
                let Some(suite_key) =
                    suite_key_from_test_name(&name, &self.kind_by_crate_and_binary)
                else {
                    return None;
                };
                let display_name = simplify_nextest_test_name(&name);
                let suite = self
                    .suites_by_key
                    .entry(suite_key.clone())
                    .or_insert_with(|| SuiteAcc {
                        key: suite_key.clone(),
                        tests: BTreeMap::new(),
                        console_entries: vec![],
                    });
                match event.as_str() {
                    "ok" | "failed" | "ignored" => {
                        let status = match event.as_str() {
                            "ok" => "passed",
                            "failed" => "failed",
                            "ignored" => "pending",
                            _ => "pending",
                        }
                        .to_string();
                        let duration_ms = exec_time
                            .map(|sec| (sec * 1000.0).max(0.0) as u64)
                            .unwrap_or(0);
                        let mut test_case =
                            suite
                                .tests
                                .remove(&display_name)
                                .unwrap_or_else(|| TestCaseResult {
                                    title: display_name.clone(),
                                    full_name: display_name.clone(),
                                    status: status.clone(),
                                    timed_out: None,
                                    duration: duration_ms,
                                    location: None,
                                    failure_messages: vec![],
                                    failure_details: None,
                                });
                        test_case.status = status.clone();
                        test_case.duration = duration_ms;
                        if test_case.status == "failed" {
                            let msg = stdout.clone().unwrap_or_default();
                            if !msg.trim().is_empty() {
                                test_case.failure_messages = vec![msg.clone()];
                            }
                        }
                        if let Some(out) = stdout.as_deref().filter(|s| !s.trim().is_empty()) {
                            suite.console_entries.extend(
                                out.lines()
                                    .map(str::trim)
                                    .filter(|ln| !ln.is_empty())
                                    .map(|ln| TestConsoleEntry {
                                        message: Some(serde_json::Value::String(ln.to_string())),
                                        type_name: Some("log".to_string()),
                                        origin: Some("cargo-nextest".to_string()),
                                    }),
                            );
                        }
                        suite.tests.insert(display_name.clone(), test_case);
                        Some(NextestStreamUpdate {
                            suite_path: suite_display_path(&self.repo_root, &suite_key),
                            test_name: display_name,
                            status,
                            stdout,
                        })
                    }
                    _ => None,
                }
            }
        }
    }

    pub fn finalize(mut self) -> Option<TestRunModel> {
        if !self.loose_log_lines.is_empty()
            && let Some((_key, first_suite)) = self.suites_by_key.iter_mut().next()
        {
            first_suite
                .console_entries
                .extend(self.loose_log_lines.iter().map(|ln| TestConsoleEntry {
                    message: Some(serde_json::Value::String(ln.to_string())),
                    type_name: Some("log".to_string()),
                    origin: Some("cargo-nextest".to_string()),
                }));
        };
        let suites = self
            .suites_by_key
            .into_values()
            .map(|suite| finalize_suite(&self.repo_root, suite))
            .filter(|suite| !suite.test_results.is_empty())
            .collect::<Vec<_>>();
        (!suites.is_empty()).then(|| build_run_model(suites))
    }
}

pub fn parse_nextest_libtest_json_output(
    repo_root: &Path,
    combined_output: &str,
) -> Option<TestRunModel> {
    let mut parser = NextestStreamParser::new(repo_root);
    combined_output.lines().for_each(|line| {
        let _ = parser.push_line(line);
    });
    parser.finalize()
}

fn simplify_nextest_test_name(full: &str) -> String {
    full.split('$').nth(1).unwrap_or(full).to_string()
}

fn suite_key_from_test_name(
    full_name: &str,
    kind_by_crate_and_binary: &BTreeMap<(String, String), String>,
) -> Option<SuiteKey> {
    let (crate_name, rest) = full_name.split_once("::")?;
    let (test_binary, _) = rest.split_once('$').unwrap_or((rest, ""));
    let kind = kind_by_crate_and_binary
        .get(&(crate_name.to_string(), test_binary.to_string()))
        .cloned()
        .unwrap_or_else(|| "test".to_string());
    Some(SuiteKey {
        crate_name: crate_name.to_string(),
        test_binary: test_binary.to_string(),
        kind,
    })
}

fn finalize_suite(repo_root: &Path, suite: SuiteAcc) -> TestSuiteResult {
    let tests = suite.tests.into_values().collect::<Vec<_>>();
    let failed = tests.iter().filter(|t| t.status == "failed").count() as u64;
    let status = if failed > 0 { "failed" } else { "passed" }.to_string();
    let test_file_path = suite_display_path(repo_root, &suite.key);
    TestSuiteResult {
        test_file_path,
        status,
        timed_out: None,
        failure_message: String::new(),
        failure_details: None,
        test_exec_error: None,
        console: (!suite.console_entries.is_empty()).then_some(suite.console_entries),
        test_results: tests,
    }
}

fn suite_display_path(repo_root: &Path, key: &SuiteKey) -> String {
    let package_root = resolve_package_root(repo_root, &key.crate_name);
    let rel = match key.kind.as_str() {
        "lib" => "src/lib.rs".to_string(),
        "test" => format!("tests/{}.rs", key.test_binary),
        "bench" => format!("benches/{}.rs", key.test_binary),
        _ => key.test_binary.clone(),
    };
    package_root.join(rel).to_string_lossy().to_string()
}

fn resolve_package_root(repo_root: &Path, crate_name: &str) -> PathBuf {
    let candidate = repo_root.join(crate_name);
    let cargo_toml = candidate.join("Cargo.toml");
    if cargo_toml.exists() {
        candidate
    } else {
        repo_root.to_path_buf()
    }
}

fn build_run_model(suites: Vec<TestSuiteResult>) -> TestRunModel {
    let start_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let total_suites = suites.len() as u64;
    let failed_suites = suites.iter().filter(|s| s.status == "failed").count() as u64;
    let passed_suites = total_suites.saturating_sub(failed_suites);

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
