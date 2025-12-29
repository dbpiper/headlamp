use std::path::{Path, PathBuf};
use std::time::Instant;

use duct::cmd as duct_cmd;
use once_cell::sync::Lazy;
use regex::Regex;

use headlamp_core::args::ParsedArgs;
use headlamp_core::config::ChangedMode;
use headlamp_core::config::CoverageUi;
use headlamp_core::coverage::istanbul_pretty::format_istanbul_pretty_from_lcov_report;
use headlamp_core::coverage::lcov::read_repo_lcov_filtered;
use headlamp_core::coverage::print::PrintOpts;
use headlamp_core::coverage::thresholds::compare_thresholds_and_print_if_needed;
use headlamp_core::format::cargo_test::{CargoTestStreamEvent, CargoTestStreamParser};
use headlamp_core::format::ctx::make_ctx;
use headlamp_core::format::nextest::{NextestStreamParser, NextestStreamUpdate};
use headlamp_core::format::vitest::render_vitest_from_test_model;
use headlamp_core::test_model::{TestRunAggregated, TestRunModel};

use crate::cargo_select::{changed_rust_seeds, filter_rust_tests_by_seeds, list_rust_test_files};
use crate::git::changed_files;
use crate::live_progress::{LiveProgress, LiveProgressMode, live_progress_mode};
use crate::run::{RunError, run_bootstrap};
use crate::streaming::{OutputStream, StreamAction, StreamAdapter, run_streaming_capture_tail};

static RUST_PANIC_AT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"panicked at (?:[^:]+: )?([^:\s]+:\d+:\d+)"#).unwrap());

pub fn build_cargo_llvm_cov_command_args(
    enable_branch_coverage: bool,
    use_nightly: bool,
    subcommand_args: &[String],
) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    if enable_branch_coverage && use_nightly {
        out.push("+nightly".to_string());
    }
    out.push("llvm-cov".to_string());
    if enable_branch_coverage && use_nightly {
        out.push("--branch".to_string());
    }
    out.extend(subcommand_args.iter().cloned());
    out
}

fn can_use_nightly(repo_root: &Path) -> bool {
    duct_cmd("cargo", ["+nightly", "llvm-cov", "--version"])
        .dir(repo_root)
        .stdout_capture()
        .stderr_capture()
        .unchecked()
        .run()
        .ok()
        .is_some_and(|o| o.status.success())
}

fn headlamp_cargo_target_dir(repo_root: &Path) -> PathBuf {
    repo_root.join("target").join("headlamp-cargo")
}

fn apply_headlamp_cargo_target_dir(cmd: &mut std::process::Command, repo_root: &Path) {
    cmd.env("CARGO_TARGET_DIR", headlamp_cargo_target_dir(repo_root));
}

fn print_runner_tail_if_failed_without_tests(
    exit_code: i32,
    model: &TestRunModel,
    tail: &crate::streaming::RingBuffer,
) {
    if exit_code == 0 {
        return;
    }
    if !model.test_results.is_empty() {
        return;
    }
    let all = tail.lines().cloned().collect::<Vec<_>>();
    let start = all.len().saturating_sub(60);
    let last_lines = all[start..].iter().collect::<Vec<_>>();
    if last_lines.is_empty() {
        return;
    }
    eprintln!("headlamp: runner failed before producing any test results; last output:");
    last_lines.into_iter().for_each(|line| eprintln!("{line}"));
}

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

fn apply_wall_clock_run_time_ms(mut model: TestRunModel, elapsed_ms: u64) -> TestRunModel {
    model.aggregated.run_time_ms = Some(elapsed_ms);
    model
}

#[derive(Debug)]
struct NextestAdapter {
    only_failures: bool,
    parser: NextestStreamParser,
}

impl NextestAdapter {
    fn new(repo_root: &Path, only_failures: bool) -> Self {
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
struct CargoTestAdapter {
    only_failures: bool,
    parser: CargoTestStreamParser,
}

impl CargoTestAdapter {
    fn new(repo_root: &Path, only_failures: bool) -> Self {
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
            } => {
                vec![]
            }
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
        if is_ci_env && !is_tty_output && stream == OutputStream::Stderr && has_useful_status {
            return vec![
                StreamAction::SetProgressLabel(format!("cargo: {}", line.trim())),
                StreamAction::PrintStderr(line.trim().to_string()),
            ];
        }
        self.parser
            .push_line(line)
            .into_iter()
            .flat_map(|evt| self.actions_for_event(evt))
            .collect::<Vec<_>>()
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
        if is_ci_env && !is_tty_output && stream == OutputStream::Stderr && has_useful_status {
            return vec![
                StreamAction::SetProgressLabel(format!("cargo: {}", line.trim())),
                StreamAction::PrintStderr(line.trim().to_string()),
            ];
        }
        self.parser
            .push_line(line)
            .as_ref()
            .map(|u| self.actions_for_update(u))
            .unwrap_or_default()
    }
}

fn run_optional_bootstrap(repo_root: &Path, args: &ParsedArgs) -> Result<(), RunError> {
    let Some(command) = args.bootstrap_command.as_deref() else {
        return Ok(());
    };
    run_bootstrap(repo_root, command)
}

pub fn run_cargo_test(repo_root: &Path, args: &ParsedArgs) -> Result<i32, RunError> {
    run_optional_bootstrap(repo_root, args)?;

    let changed = args
        .changed
        .map(|m| changed_files(repo_root, m))
        .transpose()?
        .unwrap_or_default();

    let selection = derive_cargo_selection(repo_root, args, &changed);
    if selection.changed_selection_attempted
        && selection.selected_test_count == Some(0)
        && args.changed.is_some()
    {
        let changed_mode = args
            .changed
            .map(changed_mode_to_cli_string)
            .unwrap_or("all");
        println!("headlamp: selected 0 tests (changed={changed_mode})");
        let ctx = make_ctx(
            repo_root,
            None,
            false,
            args.show_logs,
            args.editor_cmd.clone(),
        );
        let rendered = render_vitest_from_test_model(
            &empty_test_run_model_for_exit_code(0),
            &ctx,
            args.only_failures,
        );
        if !rendered.trim().is_empty() {
            println!("{rendered}");
        }
        if args.collect_coverage && args.coverage_ui != CoverageUi::Jest {
            print_lcov(repo_root, args);
        }
        return Ok(0);
    }

    if args.collect_coverage && has_cargo_llvm_cov(repo_root) {
        let exit_code =
            run_cargo_llvm_cov_test_and_render(repo_root, args, &selection.extra_cargo_args)?;
        return finish_coverage_after_test_run(repo_root, args, exit_code);
    }

    let mode = live_progress_mode(
        headlamp_core::format::terminal::is_output_terminal(),
        args.ci,
    );
    let live_progress = LiveProgress::start(1, mode);
    let run_start = Instant::now();
    let mut cmd = std::process::Command::new("cargo");
    cmd.args(build_cargo_test_args(
        None,
        args,
        &selection.extra_cargo_args,
    ));
    cmd.current_dir(repo_root);
    apply_headlamp_cargo_target_dir(&mut cmd, repo_root);
    cmd.env("RUST_BACKTRACE", "1");
    cmd.env("RUST_LIB_BACKTRACE", "1");
    let mut adapter = CargoTestAdapter::new(repo_root, args.only_failures);
    let (exit_code, tail) =
        run_streaming_capture_tail(cmd, &live_progress, &mut adapter, 1024 * 1024)?;
    live_progress.increment_done(1);
    live_progress.finish();
    let model = adapter
        .parser
        .finalize()
        .unwrap_or_else(|| empty_test_run_model_for_exit_code(exit_code));
    let elapsed_ms = run_start.elapsed().as_millis() as u64;
    let model = apply_wall_clock_run_time_ms(model, elapsed_ms);
    let model = normalize_cargo_test_model_by_panic_locations(repo_root, model);
    print_runner_tail_if_failed_without_tests(exit_code, &model, &tail);
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
        return Ok(normalize_runner_exit_code(exit_code));
    }

    let mut final_exit = normalize_runner_exit_code(exit_code);
    if args.collect_coverage && args.coverage_ui != CoverageUi::Jest {
        let thresholds_failed = print_lcov(repo_root, args);
        if thresholds_failed {
            final_exit = 1;
        }
    }

    Ok(final_exit)
}

fn normalize_cargo_test_model_by_panic_locations(
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
    let mut failed = vec![];
    let mut other = vec![];
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

    test_results.into_iter().for_each(|test_case| {
        if test_case.status == "failed" {
            failed.push(test_case);
        } else {
            other.push(test_case);
        }
    });
    let inferred_failed_path = failed.iter().find_map(|t| {
        let joined = t.failure_messages.join("\n");
        let caps = RUST_PANIC_AT_RE.captures(&joined)?;
        let loc = caps.get(1)?.as_str();
        let file = loc.split(':').next()?;
        Some(if std::path::Path::new(file).is_absolute() {
            file.to_string()
        } else {
            repo_root.join(file).to_string_lossy().to_string()
        })
    });

    let can_split = inferred_failed_path
        .as_deref()
        .is_some_and(|p| p != test_file_path.as_str())
        && !failed.is_empty()
        && !other.is_empty();
    if !can_split {
        return vec![headlamp_core::test_model::TestSuiteResult {
            test_results: failed.into_iter().chain(other).collect(),
            test_file_path,
            status,
            timed_out,
            failure_message,
            failure_details,
            test_exec_error,
            console,
        }];
    }
    let failed_path = inferred_failed_path.unwrap_or_else(|| test_file_path.clone());
    vec![
        headlamp_core::test_model::TestSuiteResult {
            test_file_path: failed_path,
            status: "failed".to_string(),
            test_results: failed,
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
            test_results: other,
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

pub fn run_cargo_nextest(repo_root: &Path, args: &ParsedArgs) -> Result<i32, RunError> {
    run_optional_bootstrap(repo_root, args)?;

    let changed = args
        .changed
        .map(|m| changed_files(repo_root, m))
        .transpose()?
        .unwrap_or_default();

    let selection = derive_cargo_selection(repo_root, args, &changed);
    if selection.changed_selection_attempted
        && selection.selected_test_count == Some(0)
        && args.changed.is_some()
    {
        let changed_mode = args
            .changed
            .map(changed_mode_to_cli_string)
            .unwrap_or("all");
        println!("headlamp: selected 0 tests (changed={changed_mode})");
        let ctx = make_ctx(
            repo_root,
            None,
            false,
            args.show_logs,
            args.editor_cmd.clone(),
        );
        let rendered = render_vitest_from_test_model(
            &empty_test_run_model_for_exit_code(0),
            &ctx,
            args.only_failures,
        );
        if !rendered.trim().is_empty() {
            println!("{rendered}");
        }
        if args.collect_coverage && args.coverage_ui != CoverageUi::Jest {
            let thresholds_failed = print_lcov(repo_root, args);
            return Ok(if thresholds_failed { 1 } else { 0 });
        }
        return Ok(0);
    }

    if args.collect_coverage && has_cargo_llvm_cov(repo_root) {
        if !has_cargo_nextest(repo_root) {
            return Err(RunError::MissingRunner {
                runner: "cargo-nextest".to_string(),
                hint: "expected `cargo nextest` to be installed and available".to_string(),
            });
        }
        let exit_code =
            run_cargo_llvm_cov_nextest_and_render(repo_root, args, &selection.extra_cargo_args)?;
        return finish_coverage_after_test_run(repo_root, args, exit_code);
    }
    if !has_cargo_nextest(repo_root) {
        return Err(RunError::MissingRunner {
            runner: "cargo-nextest".to_string(),
            hint: "expected `cargo nextest` to be installed and available".to_string(),
        });
    }

    let mode = live_progress_mode(
        headlamp_core::format::terminal::is_output_terminal(),
        args.ci,
    );
    let live_progress = LiveProgress::start(1, mode);
    let run_start = Instant::now();
    let mut cmd = std::process::Command::new("cargo");
    cmd.args(build_nextest_run_args(
        None,
        args,
        &selection.extra_cargo_args,
    ));
    cmd.current_dir(repo_root);
    apply_headlamp_cargo_target_dir(&mut cmd, repo_root);
    cmd.env("NEXTEST_EXPERIMENTAL_LIBTEST_JSON", "1");
    cmd.env("RUST_BACKTRACE", "1");
    cmd.env("RUST_LIB_BACKTRACE", "1");
    let mut adapter = NextestAdapter::new(repo_root, args.only_failures);
    let (exit_code, tail) =
        run_streaming_capture_tail(cmd, &live_progress, &mut adapter, 1024 * 1024)?;
    live_progress.increment_done(1);
    live_progress.finish();
    let model = adapter
        .parser
        .clone()
        .finalize()
        .unwrap_or_else(|| empty_test_run_model_for_exit_code(exit_code));
    let elapsed_ms = run_start.elapsed().as_millis() as u64;
    let model = apply_wall_clock_run_time_ms(model, elapsed_ms);
    print_runner_tail_if_failed_without_tests(exit_code, &model, &tail);
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
        return Ok(normalize_runner_exit_code(exit_code));
    }

    let mut final_exit = normalize_runner_exit_code(exit_code);
    if args.collect_coverage && args.coverage_ui != CoverageUi::Jest {
        let thresholds_failed = print_lcov(repo_root, args);
        if thresholds_failed {
            final_exit = 1;
        }
    }
    Ok(final_exit)
}

fn finish_coverage_after_test_run(
    repo_root: &Path,
    args: &ParsedArgs,
    exit_code: i32,
) -> Result<i32, RunError> {
    if args.coverage_abort_on_failure && exit_code != 0 {
        return Ok(normalize_runner_exit_code(exit_code));
    }
    run_cargo_llvm_cov_report(repo_root)?;
    if args.coverage_ui != CoverageUi::Jest {
        let thresholds_failed = print_lcov(repo_root, args);
        let normalized = normalize_runner_exit_code(exit_code);
        return Ok(if normalized == 0 && thresholds_failed {
            1
        } else {
            normalized
        });
    }
    Ok(normalize_runner_exit_code(exit_code))
}

fn normalize_runner_exit_code(exit_code: i32) -> i32 {
    if exit_code == 0 { 0 } else { 1 }
}

fn run_cargo_llvm_cov_test_and_render(
    repo_root: &Path,
    args: &ParsedArgs,
    extra_cargo_args: &[String],
) -> Result<i32, RunError> {
    let mode = live_progress_mode(
        headlamp_core::format::terminal::is_output_terminal(),
        args.ci,
    );
    let live_progress = LiveProgress::start(1, mode);
    let run_start = Instant::now();

    let use_nightly = can_use_nightly(repo_root);
    let enable_branch_coverage = use_nightly;
    let mut cmd = std::process::Command::new("cargo");
    cmd.args(build_cargo_llvm_cov_command_args(
        enable_branch_coverage,
        use_nightly,
        &build_llvm_cov_test_run_args(args, extra_cargo_args),
    ));
    cmd.current_dir(repo_root);
    apply_headlamp_cargo_target_dir(&mut cmd, repo_root);
    cmd.env("RUST_BACKTRACE", "1");
    cmd.env("RUST_LIB_BACKTRACE", "1");

    let mut adapter = CargoTestAdapter::new(repo_root, args.only_failures);
    let (exit_code, tail) =
        run_streaming_capture_tail(cmd, &live_progress, &mut adapter, 1024 * 1024)?;
    live_progress.increment_done(1);
    live_progress.finish();

    let model = adapter
        .parser
        .finalize()
        .unwrap_or_else(|| empty_test_run_model_for_exit_code(exit_code));
    let elapsed_ms = run_start.elapsed().as_millis() as u64;
    let model = apply_wall_clock_run_time_ms(model, elapsed_ms);
    print_runner_tail_if_failed_without_tests(exit_code, &model, &tail);
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
    Ok(exit_code)
}

fn run_cargo_llvm_cov_nextest_and_render(
    repo_root: &Path,
    args: &ParsedArgs,
    extra_cargo_args: &[String],
) -> Result<i32, RunError> {
    let mode = live_progress_mode(
        headlamp_core::format::terminal::is_output_terminal(),
        args.ci,
    );
    let live_progress = LiveProgress::start(1, mode);
    let run_start = Instant::now();

    let use_nightly = can_use_nightly(repo_root);
    let enable_branch_coverage = use_nightly;
    let mut cmd = std::process::Command::new("cargo");
    cmd.args(build_cargo_llvm_cov_command_args(
        enable_branch_coverage,
        use_nightly,
        &build_llvm_cov_nextest_run_args(args, extra_cargo_args),
    ));
    cmd.current_dir(repo_root);
    apply_headlamp_cargo_target_dir(&mut cmd, repo_root);
    cmd.env("NEXTEST_EXPERIMENTAL_LIBTEST_JSON", "1");
    cmd.env("RUST_BACKTRACE", "1");
    cmd.env("RUST_LIB_BACKTRACE", "1");

    let mut adapter = NextestAdapter::new(repo_root, args.only_failures);
    let (exit_code, tail) =
        run_streaming_capture_tail(cmd, &live_progress, &mut adapter, 1024 * 1024)?;
    live_progress.increment_done(1);
    live_progress.finish();

    let model = adapter
        .parser
        .clone()
        .finalize()
        .unwrap_or_else(|| empty_test_run_model_for_exit_code(exit_code));
    let elapsed_ms = run_start.elapsed().as_millis() as u64;
    let model = apply_wall_clock_run_time_ms(model, elapsed_ms);
    print_runner_tail_if_failed_without_tests(exit_code, &model, &tail);
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
    Ok(exit_code)
}

fn run_cargo_llvm_cov_report(repo_root: &Path) -> Result<(), RunError> {
    let cargo_target_dir = headlamp_cargo_target_dir(repo_root);
    // Only enable branch coverage when the toolchain supports it; otherwise branch export fails.
    // This keeps stable behavior unchanged while allowing accurate branch coverage on nightly.
    let use_nightly = can_use_nightly(repo_root);
    let enable_branch_coverage = use_nightly;
    let lcov_args = build_cargo_llvm_cov_command_args(
        enable_branch_coverage,
        use_nightly,
        &[
            "report".to_string(),
            "--lcov".to_string(),
            "--output-path".to_string(),
            "coverage/lcov.info".to_string(),
            "--color".to_string(),
            "never".to_string(),
        ],
    );
    let out = duct_cmd("cargo", lcov_args)
        .dir(repo_root)
        .env("CARGO_TARGET_DIR", cargo_target_dir.clone())
        .stderr_to_stdout()
        .stdout_capture()
        .unchecked()
        .run()
        .map_err(|e| RunError::Io(std::io::Error::other(e.to_string())))?;
    if !out.status.success() {
        Err(RunError::CommandFailed {
            message: format!(
                "cargo llvm-cov report failed (exit={})",
                out.status.code().unwrap_or(1)
            ),
        })
    } else {
        let json_args = build_cargo_llvm_cov_command_args(
            enable_branch_coverage,
            use_nightly,
            &[
                "report".to_string(),
                "--json".to_string(),
                "--output-path".to_string(),
                "coverage/coverage.json".to_string(),
                "--color".to_string(),
                "never".to_string(),
            ],
        );
        let json_out = duct_cmd("cargo", json_args)
            .dir(repo_root)
            .env("CARGO_TARGET_DIR", cargo_target_dir)
            .stderr_to_stdout()
            .stdout_capture()
            .unchecked()
            .run()
            .map_err(|e| RunError::Io(std::io::Error::other(e.to_string())))?;
        if json_out.status.success() {
            Ok(())
        } else {
            Err(RunError::CommandFailed {
                message: format!(
                    "cargo llvm-cov report --json failed (exit={})",
                    json_out.status.code().unwrap_or(1)
                ),
            })
        }
    }
}

fn build_llvm_cov_test_run_args(args: &ParsedArgs, extra_cargo_args: &[String]) -> Vec<String> {
    let (cargo_args, test_binary_args) = split_cargo_passthrough_args(&args.runner_args);
    let mut cmd_args: Vec<String> = vec![
        "test".to_string(),
        "--no-report".to_string(),
        "--color".to_string(),
        "never".to_string(),
    ];
    cmd_args.extend(extra_cargo_args.iter().cloned());
    if !cargo_args.iter().any(|t| t == "--no-fail-fast")
        && !extra_cargo_args.iter().any(|t| t == "--no-fail-fast")
    {
        cmd_args.push("--no-fail-fast".to_string());
    }
    cmd_args.extend(cargo_args);

    let mut normalized_test_args: Vec<String> = vec!["--color".to_string(), "never".to_string()];
    if should_force_pretty_test_output(&test_binary_args) {
        normalized_test_args.extend(["--format".to_string(), "pretty".to_string()]);
    }
    if args.show_logs && should_force_nocapture(&test_binary_args) {
        normalized_test_args.push("--nocapture".to_string());
    }
    if args.sequential && !test_binary_args.iter().any(|t| t == "--test-threads") {
        normalized_test_args.extend(["--test-threads".to_string(), "1".to_string()]);
    }
    normalized_test_args.extend(test_binary_args);

    cmd_args.push("--".to_string());
    cmd_args.extend(normalized_test_args);
    cmd_args
}

fn build_llvm_cov_nextest_run_args(args: &ParsedArgs, extra_cargo_args: &[String]) -> Vec<String> {
    let (nextest_options, test_binary_args) = split_cargo_passthrough_args(&args.runner_args);
    let has_user_test_threads = nextest_options.iter().any(|t| t == "--test-threads");
    let has_user_color = nextest_options
        .iter()
        .any(|t| t == "--color" || t.starts_with("--color="));
    let translated = translate_libtest_args_to_nextest(&test_binary_args);

    let (success_output, failure_output) = if args.show_logs {
        ("immediate", "immediate")
    } else {
        ("never", "never")
    };
    let is_interactive = live_progress_mode(
        headlamp_core::format::terminal::is_output_terminal(),
        args.ci,
    ) == LiveProgressMode::Interactive;

    let mut cmd_args: Vec<String> = vec!["nextest".to_string(), "--no-report".to_string()];
    cmd_args.extend(extra_cargo_args.iter().cloned());
    cmd_args.extend(nextest_options);

    if !has_user_color {
        cmd_args.extend(["--color".to_string(), "never".to_string()]);
    }
    cmd_args.extend([
        "--status-level".to_string(),
        "none".to_string(),
        "--final-status-level".to_string(),
        "none".to_string(),
        "--no-fail-fast".to_string(),
        "--show-progress".to_string(),
        "none".to_string(),
        "--success-output".to_string(),
        success_output.to_string(),
        "--failure-output".to_string(),
        failure_output.to_string(),
        // In interactive mode, don't pass `--cargo-quiet` so Cargo build output can flow and
        // LiveProgress can surface richer "recent activity" hints (building/installing deps).
        "--no-input-handler".to_string(),
        "--no-output-indent".to_string(),
        "--message-format".to_string(),
        "libtest-json-plus".to_string(),
    ]);
    if !is_interactive {
        cmd_args.push("--cargo-quiet".to_string());
    }

    if args.sequential && translated.test_threads.is_none() && !has_user_test_threads {
        cmd_args.extend(["--test-threads".to_string(), "1".to_string()]);
    } else if let Some(n) = translated.test_threads.as_ref() {
        cmd_args.extend(["--test-threads".to_string(), n.to_string()]);
    }

    if let Some(user_filter) = translated.filter.as_deref() {
        cmd_args.push(user_filter.to_string());
    }
    if !translated.passthrough.is_empty() {
        cmd_args.push("--".to_string());
        cmd_args.extend(translated.passthrough);
    }
    cmd_args
}

fn build_nextest_run_args(
    filter: Option<&str>,
    args: &ParsedArgs,
    extra_cargo_args: &[String],
) -> Vec<String> {
    let (cargo_args, test_binary_args) = split_cargo_passthrough_args(&args.runner_args);
    let mut cmd_args: Vec<String> = vec!["nextest".to_string(), "run".to_string()];
    let (success_output, failure_output) = if args.show_logs {
        ("immediate", "immediate")
    } else {
        ("never", "never")
    };
    let is_interactive = live_progress_mode(
        headlamp_core::format::terminal::is_output_terminal(),
        args.ci,
    ) == LiveProgressMode::Interactive;

    cmd_args.extend([
        "--color".to_string(),
        "never".to_string(),
        "--status-level".to_string(),
        "none".to_string(),
        "--final-status-level".to_string(),
        "none".to_string(),
        "--no-fail-fast".to_string(),
        "--show-progress".to_string(),
        "none".to_string(),
        "--success-output".to_string(),
        success_output.to_string(),
        "--failure-output".to_string(),
        failure_output.to_string(),
        // In interactive mode, don't pass `--cargo-quiet` so Cargo build output can flow and
        // LiveProgress can surface richer "recent activity" hints (building/installing deps).
        "--no-input-handler".to_string(),
        "--no-output-indent".to_string(),
        "--message-format".to_string(),
        "libtest-json-plus".to_string(),
    ]);
    if !is_interactive {
        cmd_args.push("--cargo-quiet".to_string());
    }

    let translated = translate_libtest_args_to_nextest(&test_binary_args);
    if args.sequential
        && translated.test_threads.is_none()
        && !cargo_args.iter().any(|t| t == "--test-threads")
    {
        cmd_args.extend(["--test-threads".to_string(), "1".to_string()]);
    } else if let Some(n) = translated.test_threads.as_ref() {
        cmd_args.extend(["--test-threads".to_string(), n.to_string()]);
    }

    cmd_args.extend(extra_cargo_args.iter().cloned());
    cmd_args.extend(cargo_args);
    if let Some(f) = filter.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        cmd_args.push(f.to_string());
    } else if let Some(user_filter) = translated.filter.as_deref() {
        cmd_args.push(user_filter.to_string());
    }

    if !translated.passthrough.is_empty() {
        cmd_args.push("--".to_string());
        cmd_args.extend(translated.passthrough);
    }
    cmd_args
}

struct NextestArgTranslation {
    test_threads: Option<u32>,
    passthrough: Vec<String>,
    filter: Option<String>,
}

fn translate_libtest_args_to_nextest(test_binary_args: &[String]) -> NextestArgTranslation {
    let mut test_threads: Option<u32> = None;
    let mut passthrough: Vec<String> = vec![];
    let mut filter: Option<String> = None;
    let mut index: usize = 0;
    while index < test_binary_args.len() {
        let token = test_binary_args[index].as_str();
        match token {
            "--test-threads" => {
                test_threads = test_binary_args
                    .get(index + 1)
                    .and_then(|s| s.parse::<u32>().ok());
                index += 2;
            }
            "--nocapture" | "--no-capture" => {
                passthrough.push("--no-capture".to_string());
                index += 1;
            }
            "--ignored" | "--include-ignored" | "--exact" => {
                passthrough.push(token.to_string());
                index += 1;
            }
            "--skip" => {
                passthrough.push("--skip".to_string());
                if let Some(value) = test_binary_args.get(index + 1) {
                    passthrough.push(value.clone());
                    index += 2;
                } else {
                    index += 1;
                }
            }
            _ => {
                if !token.starts_with('-') && filter.is_none() {
                    filter = Some(token.to_string());
                }
                index += 1;
            }
        }
    }
    NextestArgTranslation {
        test_threads,
        passthrough,
        filter,
    }
}

fn build_cargo_test_args(
    filter: Option<&str>,
    args: &ParsedArgs,
    extra_cargo_args: &[String],
) -> Vec<String> {
    let (cargo_args, test_binary_args) = split_cargo_passthrough_args(&args.runner_args);
    let mut cmd_args: Vec<String> = vec!["test".to_string()];
    if let Some(f) = filter.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        cmd_args.push(f.to_string());
    }
    cmd_args.extend(extra_cargo_args.iter().cloned());
    if !cargo_args.iter().any(|t| t == "--no-fail-fast")
        && !extra_cargo_args.iter().any(|t| t == "--no-fail-fast")
    {
        cmd_args.push("--no-fail-fast".to_string());
    }
    cmd_args.extend(cargo_args);

    let mut normalized_test_args: Vec<String> = vec!["--color".to_string(), "never".to_string()];
    if should_force_pretty_test_output(&test_binary_args) {
        normalized_test_args.extend(["--format".to_string(), "pretty".to_string()]);
    }
    if args.show_logs && should_force_show_output(&test_binary_args) {
        normalized_test_args.push("--show-output".to_string());
    }
    if args.sequential && !test_binary_args.iter().any(|t| t == "--test-threads") {
        normalized_test_args.extend(["--test-threads".to_string(), "1".to_string()]);
    }
    normalized_test_args.extend(test_binary_args);

    cmd_args.push("--".to_string());
    cmd_args.extend(normalized_test_args);
    cmd_args
}

fn should_force_pretty_test_output(test_binary_args: &[String]) -> bool {
    let overrides_format = test_binary_args.iter().any(|token| {
        token == "--format" || token.starts_with("--format=") || token == "-q" || token == "--quiet"
    });
    !overrides_format
}

fn should_force_nocapture(test_binary_args: &[String]) -> bool {
    let overrides_capture = test_binary_args.iter().any(|token| {
        token == "--nocapture"
            || token == "--no-capture"
            || token == "--capture"
            || token == "--show-output"
    });
    !overrides_capture
}

fn should_force_show_output(test_binary_args: &[String]) -> bool {
    // Prefer `--show-output` (grouped per test) over `--nocapture` (interleaved chaos).
    should_force_nocapture(test_binary_args)
}

fn split_cargo_passthrough_args(passthrough: &[String]) -> (Vec<String>, Vec<String>) {
    let sanitized = passthrough
        .iter()
        .filter(|t| !is_jest_default_runner_arg(t))
        .cloned()
        .collect::<Vec<_>>();
    sanitized
        .iter()
        .position(|t| t == "--")
        .map(|index| (sanitized[..index].to_vec(), sanitized[index + 1..].to_vec()))
        .unwrap_or((sanitized, vec![]))
}

fn is_jest_default_runner_arg(token: &str) -> bool {
    token == "--runInBand"
        || token == "--no-silent"
        || token == "--coverage"
        || token.starts_with("--coverageProvider=")
        || token.starts_with("--coverageReporters=")
}

fn has_cargo_nextest(repo_root: &Path) -> bool {
    duct_cmd("cargo", ["nextest", "--version"])
        .dir(repo_root)
        .env("CARGO_TARGET_DIR", headlamp_cargo_target_dir(repo_root))
        .stdout_capture()
        .stderr_capture()
        .unchecked()
        .run()
        .ok()
        .is_some_and(|o| o.status.success())
}

fn print_lcov(repo_root: &Path, args: &ParsedArgs) -> bool {
    let Some(filtered) =
        read_repo_lcov_filtered(repo_root, &args.include_globs, &args.exclude_globs)
    else {
        return false;
    };
    let filtered =
        match crate::coverage::llvm_cov_json::read_repo_llvm_cov_json_statement_hits(repo_root)
            .as_ref()
        {
            Some(statement_hits_by_path) => crate::coverage::model::apply_statement_hits_to_report(
                filtered,
                statement_hits_by_path,
            ),
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
    println!("{pretty}");
    compare_thresholds_and_print_if_needed(args.coverage_thresholds.as_ref(), Some(&filtered))
}
fn has_cargo_llvm_cov(repo_root: &Path) -> bool {
    duct_cmd("cargo", ["llvm-cov", "--version"])
        .dir(repo_root)
        .env("CARGO_TARGET_DIR", headlamp_cargo_target_dir(repo_root))
        .stdout_capture()
        .stderr_capture()
        .unchecked()
        .run()
        .ok()
        .is_some_and(|o| o.status.success())
}

#[derive(Debug, Clone)]
struct CargoSelection {
    extra_cargo_args: Vec<String>,
    changed_selection_attempted: bool,
    selected_test_count: Option<usize>,
}

fn derive_cargo_selection(
    repo_root: &Path,
    args: &ParsedArgs,
    changed: &[std::path::PathBuf],
) -> CargoSelection {
    if !args.selection_paths.is_empty() {
        return derive_selection_from_selection_paths(repo_root, &args.selection_paths);
    }

    if changed.is_empty() {
        return CargoSelection {
            extra_cargo_args: vec![],
            changed_selection_attempted: false,
            selected_test_count: None,
        };
    }

    let tests = list_rust_test_files(repo_root);
    if tests.is_empty() {
        return CargoSelection {
            extra_cargo_args: vec![],
            changed_selection_attempted: true,
            selected_test_count: None,
        };
    }

    let seeds = changed_rust_seeds(repo_root, changed);
    let kept = filter_rust_tests_by_seeds(&tests, &seeds);
    let test_targets = kept
        .iter()
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()))
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

    let selected_count = test_targets.len();
    CargoSelection {
        extra_cargo_args: build_test_target_args(&test_targets),
        changed_selection_attempted: true,
        selected_test_count: if selected_count == 0 {
            None
        } else {
            Some(selected_count)
        },
    }
}

fn derive_selection_from_selection_paths(
    repo_root: &Path,
    selection_paths: &[String],
) -> CargoSelection {
    let abs = selection_paths
        .iter()
        .map(|p| repo_root.join(p))
        .filter(|p| p.exists())
        .collect::<Vec<_>>();
    if abs.is_empty() {
        return CargoSelection {
            extra_cargo_args: vec![],
            changed_selection_attempted: false,
            selected_test_count: None,
        };
    }

    let direct_test_stems = abs
        .iter()
        .filter(|p| is_rust_test_file(p))
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()))
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    if !direct_test_stems.is_empty() {
        return CargoSelection {
            extra_cargo_args: build_test_target_args(&direct_test_stems),
            changed_selection_attempted: false,
            selected_test_count: Some(direct_test_stems.len()),
        };
    }

    let test_targets = derive_test_targets_from_seeds(repo_root, &abs);
    CargoSelection {
        extra_cargo_args: build_test_target_args(&test_targets),
        changed_selection_attempted: false,
        selected_test_count: Some(test_targets.len()),
    }
}

fn changed_mode_to_cli_string(mode: ChangedMode) -> &'static str {
    match mode {
        ChangedMode::All => "all",
        ChangedMode::Staged => "staged",
        ChangedMode::Unstaged => "unstaged",
        ChangedMode::Branch => "branch",
        ChangedMode::LastCommit => "lastCommit",
    }
}

fn derive_test_targets_from_seeds(
    repo_root: &Path,
    seeds_input: &[std::path::PathBuf],
) -> Vec<String> {
    let tests = list_rust_test_files(repo_root);
    if tests.is_empty() {
        return vec![];
    }
    let seeds = changed_rust_seeds(repo_root, seeds_input);
    let kept = filter_rust_tests_by_seeds(&tests, &seeds);
    let mut stems = kept
        .iter()
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()))
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    stems.sort();
    stems.dedup();
    stems
}

fn is_rust_test_file(path: &std::path::Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("rs")
        && path
            .components()
            .any(|c| c.as_os_str().to_string_lossy() == "tests")
}

fn build_test_target_args(test_targets: &[String]) -> Vec<String> {
    let mut sorted = test_targets.to_vec();
    sorted.sort();
    sorted.dedup();

    sorted
        .into_iter()
        .flat_map(|stem| ["--test".to_string(), stem])
        .collect::<Vec<_>>()
}
