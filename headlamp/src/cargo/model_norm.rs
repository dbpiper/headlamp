use std::path::Path;

use regex::Regex;
use std::sync::LazyLock;

use headlamp_core::test_model::{TestRunAggregated, TestRunModel};

#[derive(Debug)]
struct UnsplitSuiteParts {
    test_file_path: String,
    status: String,
    timed_out: Option<bool>,
    failure_message: String,
    failure_details: Option<Vec<serde_json::Value>>,
    test_exec_error: Option<serde_json::Value>,
    console: Option<Vec<headlamp_core::test_model::TestConsoleEntry>>,
    failed_tests: Vec<headlamp_core::test_model::TestCaseResult>,
    non_failed_tests: Vec<headlamp_core::test_model::TestCaseResult>,
}

#[derive(Debug)]
struct SplitSuiteParts {
    test_file_path: String,
    timed_out: Option<bool>,
    failure_message: String,
    failure_details: Option<Vec<serde_json::Value>>,
    test_exec_error: Option<serde_json::Value>,
    console: Option<Vec<headlamp_core::test_model::TestConsoleEntry>>,
    inferred_failed_path: Option<String>,
    failed_tests: Vec<headlamp_core::test_model::TestCaseResult>,
    non_failed_tests: Vec<headlamp_core::test_model::TestCaseResult>,
}

static RUST_PANIC_AT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"panicked at (?:[^:]+: )?([^:\s]+:\d+:\d+)"#).unwrap());

pub(crate) fn empty_test_run_model_for_exit_code(exit_code: i32) -> TestRunModel {
    let success = exit_code == 0;
    TestRunModel {
        start_time: 0,
        test_results: vec![],
        aggregated: TestRunAggregated {
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
            success,
            run_time_ms: Some(0),
        },
    }
}

pub(super) fn normalize_cargo_test_model_by_panic_locations(
    repo_root: &Path,
    model: TestRunModel,
) -> TestRunModel {
    let suites = model
        .test_results
        .into_iter()
        .flat_map(|suite| split_cargo_suite_by_failure_location(repo_root, suite))
        .collect::<Vec<_>>();
    let aggregated = recompute_aggregated(&suites, model.aggregated.run_time_ms);
    TestRunModel {
        start_time: model.start_time,
        test_results: suites,
        aggregated,
    }
}

fn split_cargo_suite_by_failure_location(
    repo_root: &Path,
    suite: headlamp_core::test_model::TestSuiteResult,
) -> Vec<headlamp_core::test_model::TestSuiteResult> {
    let headlamp_core::test_model::TestSuiteResult {
        test_file_path,
        status,
        timed_out,
        failure_message,
        failure_details,
        test_exec_error,
        console,
        test_results,
    } = suite;

    let (failed_tests, non_failed_tests) = partition_tests_by_failure(test_results);
    let inferred_failed_path = infer_failed_path(repo_root, &failed_tests);

    if !should_split_suite(
        &test_file_path,
        &inferred_failed_path,
        &failed_tests,
        &non_failed_tests,
    ) {
        return vec![build_unsplit_suite(UnsplitSuiteParts {
            test_file_path,
            status,
            timed_out,
            failure_message,
            failure_details,
            test_exec_error,
            console,
            failed_tests,
            non_failed_tests,
        })];
    }

    build_split_suites(SplitSuiteParts {
        test_file_path,
        timed_out,
        failure_message,
        failure_details,
        test_exec_error,
        console,
        inferred_failed_path,
        failed_tests,
        non_failed_tests,
    })
}

fn partition_tests_by_failure(
    test_results: Vec<headlamp_core::test_model::TestCaseResult>,
) -> (
    Vec<headlamp_core::test_model::TestCaseResult>,
    Vec<headlamp_core::test_model::TestCaseResult>,
) {
    test_results
        .into_iter()
        .fold((vec![], vec![]), |(mut failed, mut other), test_case| {
            if test_case.status == "failed" {
                failed.push(test_case);
            } else {
                other.push(test_case);
            }
            (failed, other)
        })
}

fn infer_failed_path(
    repo_root: &Path,
    failed_tests: &[headlamp_core::test_model::TestCaseResult],
) -> Option<String> {
    failed_tests.iter().find_map(|t| {
        let joined = t.failure_messages.join("\n");
        let caps = RUST_PANIC_AT_RE.captures(&joined)?;
        let loc = caps.get(1)?.as_str();
        let file = loc.split(':').next()?;
        Some(if std::path::Path::new(file).is_absolute() {
            file.to_string()
        } else {
            repo_root.join(file).to_string_lossy().to_string()
        })
    })
}

fn should_split_suite(
    test_file_path: &str,
    inferred_failed_path: &Option<String>,
    failed_tests: &[headlamp_core::test_model::TestCaseResult],
    non_failed_tests: &[headlamp_core::test_model::TestCaseResult],
) -> bool {
    inferred_failed_path
        .as_deref()
        .is_some_and(|p| p != test_file_path)
        && !failed_tests.is_empty()
        && !non_failed_tests.is_empty()
}

fn build_unsplit_suite(parts: UnsplitSuiteParts) -> headlamp_core::test_model::TestSuiteResult {
    let UnsplitSuiteParts {
        test_file_path,
        status,
        timed_out,
        failure_message,
        failure_details,
        test_exec_error,
        console,
        failed_tests,
        non_failed_tests,
    } = parts;
    headlamp_core::test_model::TestSuiteResult {
        test_results: failed_tests.into_iter().chain(non_failed_tests).collect(),
        test_file_path,
        status,
        timed_out,
        failure_message,
        failure_details,
        test_exec_error,
        console,
    }
}

fn build_split_suites(parts: SplitSuiteParts) -> Vec<headlamp_core::test_model::TestSuiteResult> {
    let SplitSuiteParts {
        test_file_path,
        timed_out,
        failure_message,
        failure_details,
        test_exec_error,
        console,
        inferred_failed_path,
        failed_tests,
        non_failed_tests,
    } = parts;
    let failed_path = inferred_failed_path.unwrap_or_else(|| test_file_path.clone());
    vec![
        headlamp_core::test_model::TestSuiteResult {
            test_file_path: failed_path,
            status: "failed".to_string(),
            test_results: failed_tests,
            timed_out,
            failure_message,
            failure_details,
            test_exec_error,
            console: console.clone(),
        },
        headlamp_core::test_model::TestSuiteResult {
            test_file_path,
            status: "passed".to_string(),
            failure_message: String::new(),
            failure_details: None,
            test_exec_error: None,
            test_results: non_failed_tests,
            timed_out,
            console,
        },
    ]
}

fn recompute_aggregated(
    suites: &[headlamp_core::test_model::TestSuiteResult],
    run_time_ms: Option<u64>,
) -> TestRunAggregated {
    suites.iter().fold(
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
            run_time_ms,
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
    )
}
