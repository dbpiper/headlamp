use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use headlamp_core::args::ParsedArgs;
use headlamp_core::coverage::istanbul_pretty::format_istanbul_pretty_from_lcov_report;
use headlamp_core::coverage::lcov::read_repo_lcov_filtered;
use headlamp_core::coverage::model::apply_statement_totals_to_report;
use headlamp_core::coverage::print::PrintOpts;
use headlamp_core::coverage::thresholds::compare_thresholds_and_print_if_needed;
use headlamp_core::format::ctx::make_ctx;
use headlamp_core::format::vitest::render_vitest_from_test_model;
use headlamp_core::test_model::{
    TestConsoleEntry, TestRunAggregated, TestRunModel, TestSuiteResult,
};

use crate::git::changed_files;
use crate::live_progress;
use crate::pytest_select::{changed_seeds, discover_pytest_test_files, filter_tests_by_seeds};
use crate::run::{RunError, run_bootstrap};
use crate::streaming::{
    OutputStream, StreamAction, StreamAdapter, run_streaming_capture_tail_merged,
};

const PYTEST_PLUGIN_BYTES: &[u8] = include_bytes!("../assets/pytest/headlamp_pytest_plugin.py");
const PYTEST_EVENT_PREFIX: &str = "HEADLAMP_PYTEST_EVENT ";

pub fn run_pytest(repo_root: &Path, args: &ParsedArgs) -> Result<i32, RunError> {
    if let Some(cmd) = args
        .bootstrap_command
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        run_bootstrap(repo_root, cmd)?;
    }

    let selected = resolve_pytest_selection(repo_root, args)?;

    let pytest_bin = if cfg!(windows) {
        "pytest.exe"
    } else {
        "pytest"
    };

    let tmp = std::env::temp_dir().join("headlamp").join("pytest");
    let plugin_path = write_asset(&tmp.join("headlamp_pytest_plugin.py"), PYTEST_PLUGIN_BYTES)?;
    let pythonpath = crate::pythonpath::build_pytest_pythonpath(
        repo_root,
        &[tmp.as_path()],
        std::env::var("PYTHONPATH").ok(),
    );

    let mut cmd_args: Vec<String> = vec![
        "-p".to_string(),
        "headlamp_pytest_plugin".to_string(),
        "--no-header".to_string(),
        "--no-summary".to_string(),
        "-q".to_string(),
    ];
    cmd_args.extend(args.runner_args.iter().cloned());
    cmd_args.extend(selected.iter().cloned());
    if args.collect_coverage && !args.runner_args.iter().any(|a| a.starts_with("--cov")) {
        cmd_args.push("--cov=.".to_string());
        cmd_args.push("--cov-report=lcov:coverage/lcov.info".to_string());
    }

    let mode = live_progress::live_progress_mode(
        headlamp_core::format::terminal::is_output_terminal(),
        args.ci,
    );
    let live_progress = live_progress::LiveProgress::start(1, mode);

    let mut command = Command::new(pytest_bin);
    command
        .args(cmd_args)
        .current_dir(repo_root)
        .env("CI", "1")
        .env("PYTHONPATH", pythonpath);
    if !plugin_path.is_empty() {
        let _ = plugin_path;
    }
    let mut adapter = PytestAdapter::new(args.show_logs, args.ci);
    let (exit_code, _tail) =
        run_streaming_capture_tail_merged(command, &live_progress, &mut adapter, 1024 * 1024)?;
    live_progress.increment_done(1);
    live_progress.finish();

    let model = adapter.finalize(exit_code);
    let ctx = make_ctx(
        repo_root,
        None,
        exit_code != 0,
        args.show_logs,
        args.editor_cmd.clone(),
    );
    let rendered = render_vitest_from_test_model(&model, &ctx, args.only_failures);
    if !rendered.trim().is_empty() {
        println!("{rendered}");
    }

    if args.coverage_abort_on_failure && exit_code != 0 {
        return Ok(exit_code);
    }

    let mut final_exit = exit_code;
    if args.collect_coverage {
        let _ = run_coveragepy_json_report(repo_root);
        let Some(filtered) =
            read_repo_lcov_filtered(repo_root, &args.include_globs, &args.exclude_globs)
        else {
            return Ok(final_exit);
        };
        let filtered =
            match headlamp_core::coverage::coveragepy_json::read_repo_coveragepy_json_statement_totals(
                repo_root,
            )
            .as_ref()
            {
                Some(statement_totals_by_path) => {
                    apply_statement_totals_to_report(filtered, statement_totals_by_path)
                }
                None => filtered,
            };
        let print_opts =
            PrintOpts::for_run(args, headlamp_core::format::terminal::is_output_terminal());

        let pretty = format_istanbul_pretty_from_lcov_report(
            repo_root,
            &filtered,
            &print_opts,
            &[],
            &args.include_globs,
            &args.exclude_globs,
            args.coverage_detail,
        );
        if args.coverage_ui != headlamp_core::config::CoverageUi::Jest {
            println!("{pretty}");
        }
        let thresholds_failed = compare_thresholds_and_print_if_needed(
            args.coverage_thresholds.as_ref(),
            Some(&filtered),
        );
        if final_exit == 0 && thresholds_failed {
            final_exit = 1;
        }
    }

    Ok(final_exit)
}

fn run_coveragepy_json_report(repo_root: &Path) -> Result<(), RunError> {
    let python_bin = if cfg!(windows) {
        "python.exe"
    } else {
        "python"
    };
    let status = Command::new(python_bin)
        .args([
            "-m",
            "coverage",
            "json",
            "-q",
            "-o",
            "coverage/coverage.json",
        ])
        .current_dir(repo_root)
        .status()
        .map_err(|e| RunError::Io(std::io::Error::other(e.to_string())))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| RunError::CommandFailed {
            message: "python -m coverage json failed".to_string(),
        })
}

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
struct PytestAdapter {
    show_logs: bool,
    emit_raw_lines: bool,
    suites: BTreeMap<String, SuiteBuilder>,
}

impl PytestAdapter {
    fn new(show_logs: bool, emit_raw_lines: bool) -> Self {
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
            location: None,
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

    fn finalize(self, exit_code: i32) -> TestRunModel {
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

fn write_asset(path: &Path, bytes: &[u8]) -> Result<String, RunError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(RunError::Io)?;
    }
    std::fs::write(path, bytes).map_err(RunError::Io)?;
    Ok(path.to_string_lossy().to_string())
}

fn resolve_pytest_selection(repo_root: &Path, args: &ParsedArgs) -> Result<Vec<String>, RunError> {
    let changed = args
        .changed
        .map(|m| changed_files(repo_root, m))
        .transpose()?
        .unwrap_or_default();

    let all_tests = discover_pytest_test_files(repo_root, args.no_cache)?;
    let all_tests_set = all_tests
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<std::collections::BTreeSet<_>>();

    let (explicit, seed_inputs) = args
        .selection_paths
        .iter()
        .filter(|token| {
            token.ends_with(".py")
                || token.contains('/')
                || token.contains('\\')
                || token.contains("::")
        })
        .filter_map(|token| {
            let file_part = token.split("::").next().unwrap_or(token.as_str());
            let abs = repo_root.join(file_part);
            abs.exists().then_some((token, abs))
        })
        .fold(
            (Vec::<String>::new(), Vec::<PathBuf>::new()),
            |(mut explicit_acc, mut seeds_acc), (token, abs)| {
                let abs_key = abs.to_string_lossy().to_string();
                if all_tests_set.contains(&abs_key) {
                    explicit_acc.push((*token).clone());
                } else {
                    seeds_acc.push(abs);
                }
                (explicit_acc, seeds_acc)
            },
        );

    if !explicit.is_empty() {
        return Ok(explicit);
    }
    if !seed_inputs.is_empty() {
        let seeds = changed_seeds(repo_root, &seed_inputs);
        let kept = filter_tests_by_seeds(&all_tests, &seeds);
        return Ok(kept
            .into_iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect());
    }
    if changed.is_empty() {
        return Ok(all_tests
            .into_iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect());
    }

    let seeds = changed_seeds(repo_root, &changed);
    let kept = filter_tests_by_seeds(&all_tests, &seeds);

    Ok(kept
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect())
}
