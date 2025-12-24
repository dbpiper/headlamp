use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::test_model::{TestCaseResult, TestRunAggregated, TestRunModel, TestSuiteResult};

#[derive(Debug, Clone)]
struct SuiteBlock {
    source_path: String,
    lines: Vec<String>,
}

pub fn parse_cargo_test_output(repo_root: &Path, combined_output: &str) -> Option<TestRunModel> {
    let suite_blocks = split_suite_blocks(combined_output);
    let suites = suite_blocks
        .into_iter()
        .map(|block| parse_suite_block(repo_root, &block))
        .filter(|suite| !suite.test_results.is_empty())
        .collect::<Vec<_>>();
    if suites.is_empty() {
        return None;
    }
    Some(build_test_run_model(suites))
}

fn split_suite_blocks(combined_output: &str) -> Vec<SuiteBlock> {
    let mut blocks: Vec<SuiteBlock> = vec![];
    let mut current: Option<SuiteBlock> = None;

    for line in combined_output.lines() {
        if let Some(source_path) = parse_suite_header_source_path(line) {
            if let Some(prev) = current.take() {
                blocks.push(prev);
            }
            current = Some(SuiteBlock {
                source_path,
                lines: vec![],
            });
            continue;
        }
        if let Some(block) = current.as_mut() {
            block.lines.push(line.to_string());
        }
    }
    current.into_iter().for_each(|b| blocks.push(b));
    blocks
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

    let mut line_index: usize = 0;
    while line_index < block.lines.len() {
        let line = block.lines[line_index].as_str();
        if let Some((test_name, status)) = parse_test_line(line) {
            tests.push(TestCaseResult {
                title: test_name.clone(),
                full_name: test_name,
                status,
                timed_out: None,
                duration: 0,
                location: None,
                failure_messages: vec![],
                failure_details: None,
            });
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
    let status = (failed > 0)
        .then(|| "failed")
        .unwrap_or("passed")
        .to_string();
    let abs_suite_path = absolutize_repo_relative(repo_root, &block.source_path);

    TestSuiteResult {
        test_file_path: abs_suite_path,
        status,
        timed_out: None,
        failure_message: String::new(),
        failure_details: None,
        test_exec_error: None,
        console: None,
        test_results: tests,
    }
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
