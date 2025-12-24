use std::path::Path;

use duct::cmd as duct_cmd;

use headlamp_core::args::ParsedArgs;
use headlamp_core::config::CoverageUi;
use headlamp_core::config::ChangedMode;
use headlamp_core::coverage::lcov::{merge_reports, read_lcov_file, resolve_lcov_paths_to_root};
use headlamp_core::coverage::print::{
    PrintOpts, filter_report, format_compact, format_hotspots, format_summary,
};
use headlamp_core::format::cargo_test::{
    CargoTestStreamEvent, CargoTestStreamParser, parse_cargo_test_output,
};
use headlamp_core::format::ctx::make_ctx;
use headlamp_core::format::nextest::{
    NextestStreamParser, NextestStreamUpdate, parse_nextest_libtest_json_output,
};
use headlamp_core::format::vitest::render_vitest_from_test_model;
use headlamp_core::test_model::{TestRunAggregated, TestRunModel, TestSuiteResult};

use crate::cargo_select::{changed_rust_seeds, filter_rust_tests_by_seeds, list_rust_test_files};
use crate::git::changed_files;
use crate::live_progress::{LiveProgress, should_enable_live_progress};
use crate::run::{RunError, run_bootstrap};
use crate::streaming::{OutputStream, StreamAction, StreamAdapter, run_streaming_capture_tail};

fn empty_test_run_model_for_exit_code(exit_code: i32) -> TestRunModel {
    let success = exit_code == 0;
    let (num_total_test_suites, num_failed_test_suites, num_total_tests, num_failed_tests) =
        if success { (0, 0, 0, 0) } else { (1, 1, 1, 1) };
    TestRunModel {
        start_time: 0,
        test_results: vec![],
        aggregated: TestRunAggregated {
            num_total_test_suites,
            num_passed_test_suites: num_total_test_suites.saturating_sub(num_failed_test_suites),
            num_failed_test_suites,
            num_total_tests,
            num_passed_tests: num_total_tests.saturating_sub(num_failed_tests),
            num_failed_tests,
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

#[derive(Debug)]
struct NextestAdapter {
    only_failures: bool,
    show_logs: bool,
    parser: NextestStreamParser,
}

impl NextestAdapter {
    fn new(repo_root: &Path, only_failures: bool, show_logs: bool) -> Self {
        Self {
            only_failures,
            show_logs,
            parser: NextestStreamParser::new(repo_root),
        }
    }

    fn actions_for_update(&self, update: &NextestStreamUpdate) -> Vec<StreamAction> {
        let should_print = !self.only_failures || update.status == "failed";
        if !should_print {
            return vec![];
        }
        let mut out = vec![StreamAction::SetProgressLabel(update.suite_path.clone())];
        let header = format!(
            "{} {} > {}",
            if update.status == "failed" { "FAIL" } else { "PASS" },
            update.suite_path,
            update.test_name
        );
        out.push(StreamAction::PrintStdout(header));
        if self.show_logs {
            if let Some(stdout) = update.stdout.as_deref().filter(|s| !s.trim().is_empty()) {
                out.extend(
                    stdout
                        .lines()
                        .map(str::trim)
                        .filter(|ln| !ln.is_empty())
                        .map(|ln| StreamAction::PrintStdout(format!("  {ln}"))),
                );
            }
        }
        out
    }
}

#[derive(Debug)]
struct CargoTestAdapter {
    only_failures: bool,
    show_logs: bool,
    parser: CargoTestStreamParser,
    failed_tests: std::collections::BTreeSet<String>,
}

impl CargoTestAdapter {
    fn new(repo_root: &Path, only_failures: bool, show_logs: bool) -> Self {
        Self {
            only_failures,
            show_logs,
            parser: CargoTestStreamParser::new(repo_root),
            failed_tests: std::collections::BTreeSet::new(),
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
                if status == "failed" {
                    self.failed_tests
                        .insert(format!("{suite_path}::{test_name}"));
                }
                if self.only_failures && status != "failed" {
                    return vec![];
                }
                vec![
                    StreamAction::SetProgressLabel(suite_path.clone()),
                    StreamAction::PrintStdout(format!(
                        "{} {} > {}",
                        if status == "failed" { "FAIL" } else { "PASS" },
                        suite_path,
                        test_name
                    )),
                ]
            }
            CargoTestStreamEvent::OutputLine {
                suite_path,
                test_name,
                line,
            } => {
                if !self.show_logs {
                    return vec![];
                }
                if self.only_failures {
                    let Some(test_name) = test_name else {
                        return vec![];
                    };
                    if !self.failed_tests.contains(&format!("{suite_path}::{test_name}")) {
                        return vec![];
                    }
                }
                vec![StreamAction::PrintStdout(line)]
            }
        }
    }
}

impl StreamAdapter for CargoTestAdapter {
    fn on_start(&mut self) -> Option<String> {
        Some("cargo test".to_string())
    }

    fn on_line(&mut self, _stream: OutputStream, line: &str) -> Vec<StreamAction> {
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

    fn on_line(&mut self, _stream: OutputStream, line: &str) -> Vec<StreamAction> {
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
        let changed_mode = args.changed.map(changed_mode_to_cli_string).unwrap_or("all");
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

    let live_progress_enabled = should_enable_live_progress(
        headlamp_core::format::terminal::is_output_terminal(),
        args.ci,
    );
    let live_progress = LiveProgress::start(1, live_progress_enabled);
    let mut cmd = std::process::Command::new("cargo");
    cmd.args(build_cargo_test_args(None, args, &selection.extra_cargo_args));
    cmd.current_dir(repo_root);
    let mut adapter = CargoTestAdapter::new(repo_root, args.only_failures, args.show_logs);
    let (exit_code, _tail) =
        run_streaming_capture_tail(cmd, &live_progress, &mut adapter, 1024 * 1024)?;
    live_progress.increment_done(1);
    live_progress.finish();
    let model = adapter
        .parser
        .finalize()
        .unwrap_or_else(|| empty_test_run_model_for_exit_code(exit_code));
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

    if args.collect_coverage && args.coverage_ui != CoverageUi::Jest {
        print_lcov(repo_root, args);
    }

    Ok(normalize_runner_exit_code(exit_code))
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
        let changed_mode = args.changed.map(changed_mode_to_cli_string).unwrap_or("all");
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

    let live_progress_enabled = should_enable_live_progress(
        headlamp_core::format::terminal::is_output_terminal(),
        args.ci,
    );
    let live_progress = LiveProgress::start(1, live_progress_enabled);
    let mut cmd = std::process::Command::new("cargo");
    cmd.args(build_nextest_run_args(None, args, &selection.extra_cargo_args));
    cmd.current_dir(repo_root);
    cmd.env("NEXTEST_EXPERIMENTAL_LIBTEST_JSON", "1");
    let mut adapter = NextestAdapter::new(repo_root, args.only_failures, args.show_logs);
    let (exit_code, _tail) =
        run_streaming_capture_tail(cmd, &live_progress, &mut adapter, 1024 * 1024)?;
    live_progress.increment_done(1);
    live_progress.finish();
    let model = adapter
        .parser
        .clone()
        .finalize()
        .unwrap_or_else(|| empty_test_run_model_for_exit_code(exit_code));
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

    if args.collect_coverage && args.coverage_ui != CoverageUi::Jest {
        print_lcov(repo_root, args);
    }

    Ok(normalize_runner_exit_code(exit_code))
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
        print_lcov(repo_root, args);
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
    let cmd_args = build_llvm_cov_test_run_args(args, extra_cargo_args);
    let out = duct_cmd("cargo", cmd_args)
        .dir(repo_root)
        .stderr_to_stdout()
        .stdout_capture()
        .unchecked()
        .run()
        .map_err(|e| RunError::Io(std::io::Error::other(e.to_string())))?;
    let exit_code = out.status.code().unwrap_or(1);
    let combined = String::from_utf8_lossy(&out.stdout).to_string();
    print_test_output(repo_root, args, exit_code, &combined);
    Ok(exit_code)
}

fn run_cargo_llvm_cov_nextest_and_render(
    repo_root: &Path,
    args: &ParsedArgs,
    extra_cargo_args: &[String],
) -> Result<i32, RunError> {
    let cmd_args = build_llvm_cov_nextest_run_args(args, extra_cargo_args);
    let out = duct_cmd("cargo", cmd_args)
        .dir(repo_root)
        .env("NEXTEST_EXPERIMENTAL_LIBTEST_JSON", "1")
        .stderr_to_stdout()
        .stdout_capture()
        .unchecked()
        .run()
        .map_err(|e| RunError::Io(std::io::Error::other(e.to_string())))?;
    let exit_code = out.status.code().unwrap_or(1);
    let combined = String::from_utf8_lossy(&out.stdout).to_string();
    print_test_output_nextest(repo_root, args, exit_code, &combined);
    Ok(exit_code)
}

fn run_cargo_llvm_cov_report(repo_root: &Path) -> Result<(), RunError> {
    let out = duct_cmd(
        "cargo",
        [
            "llvm-cov",
            "report",
            "--lcov",
            "--output-path",
            "coverage/lcov.info",
            "--color",
            "never",
        ],
    )
    .dir(repo_root)
    .stderr_to_stdout()
    .stdout_capture()
    .unchecked()
    .run()
    .map_err(|e| RunError::Io(std::io::Error::other(e.to_string())))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(RunError::CommandFailed {
            message: format!(
                "cargo llvm-cov report failed (exit={})",
                out.status.code().unwrap_or(1)
            ),
        })
    }
}

fn build_llvm_cov_test_run_args(args: &ParsedArgs, extra_cargo_args: &[String]) -> Vec<String> {
    let (cargo_args, test_binary_args) = split_cargo_passthrough_args(&args.runner_args);
    let mut cmd_args: Vec<String> = vec![
        "llvm-cov".to_string(),
        "test".to_string(),
        "--no-report".to_string(),
        "--color".to_string(),
        "never".to_string(),
    ];
    cmd_args.extend(extra_cargo_args.iter().cloned());
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

    let mut cmd_args: Vec<String> = vec![
        "llvm-cov".to_string(),
        "nextest".to_string(),
        "--no-report".to_string(),
    ];
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
        "--cargo-quiet".to_string(),
        "--no-input-handler".to_string(),
        "--no-output-indent".to_string(),
        "--message-format".to_string(),
        "libtest-json-plus".to_string(),
    ]);

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

fn print_test_output(repo_root: &Path, args: &ParsedArgs, exit_code: i32, combined: &str) {
    let ctx = make_ctx(
        repo_root,
        None,
        exit_code != 0,
        args.show_logs,
        args.editor_cmd.clone(),
    );
    if combined.trim().is_empty() {
        let rendered = render_vitest_from_test_model(
            &empty_test_run_model_for_exit_code(exit_code),
            &ctx,
            args.only_failures,
        );
        if !rendered.trim().is_empty() {
            println!("{rendered}");
        }
        return;
    }
    let rendered = parse_cargo_test_output(repo_root, combined)
        .map(|mut model| {
            reorder_test_results_original_style_for_cargo(model.test_results.as_mut_slice());
            render_vitest_from_test_model(&model, &ctx, args.only_failures)
        })
        .unwrap_or_else(|| combined.to_string());
    if !rendered.trim().is_empty() {
        println!("{rendered}");
    }
}

fn print_test_output_nextest(repo_root: &Path, args: &ParsedArgs, exit_code: i32, combined: &str) {
    let ctx = make_ctx(
        repo_root,
        None,
        exit_code != 0,
        args.show_logs,
        args.editor_cmd.clone(),
    );
    if combined.trim().is_empty() {
        let rendered = render_vitest_from_test_model(
            &empty_test_run_model_for_exit_code(exit_code),
            &ctx,
            args.only_failures,
        );
        if !rendered.trim().is_empty() {
            println!("{rendered}");
        }
        return;
    }
    let rendered = parse_nextest_libtest_json_output(repo_root, combined)
        .map(|mut model| {
            reorder_test_results_original_style_for_cargo(model.test_results.as_mut_slice());
            render_vitest_from_test_model(&model, &ctx, args.only_failures)
        })
        .unwrap_or_else(|| combined.to_string());
    if !rendered.trim().is_empty() {
        println!("{rendered}");
    }
}

fn reorder_test_results_original_style_for_cargo(test_results: &mut [TestSuiteResult]) {
    let file_failed = |file: &TestSuiteResult| -> bool {
        file.status == "failed"
            || file
                .test_results
                .iter()
                .any(|assertion| assertion.status == "failed")
    };

    if test_results.iter().all(|file| !file_failed(file)) {
        test_results.reverse();
        return;
    }

    test_results.sort_by(|left, right| {
        normalize_abs_posix_for_ordering(&left.test_file_path)
            .cmp(&normalize_abs_posix_for_ordering(&right.test_file_path))
    });
}

fn normalize_abs_posix_for_ordering(input: &str) -> String {
    input.replace('\\', "/")
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
        "--cargo-quiet".to_string(),
        "--no-input-handler".to_string(),
        "--no-output-indent".to_string(),
        "--message-format".to_string(),
        "libtest-json-plus".to_string(),
    ]);

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
        .stdout_capture()
        .stderr_capture()
        .unchecked()
        .run()
        .ok()
        .is_some_and(|o| o.status.success())
}

fn print_lcov(repo_root: &Path, args: &ParsedArgs) {
    let lcov = repo_root.join("coverage").join("lcov.info");
    if !lcov.exists() {
        return;
    }
    let reports = read_lcov_file(&lcov).ok().into_iter().collect::<Vec<_>>();
    let merged = merge_reports(&reports, repo_root);
    let resolved = resolve_lcov_paths_to_root(merged, repo_root);
    let filtered = filter_report(
        resolved,
        repo_root,
        &args.include_globs,
        &args.exclude_globs,
    );
    println!("{}", format_summary(&filtered));
    let print_opts = PrintOpts {
        max_files: args.coverage_max_files,
        max_hotspots: args.coverage_max_hotspots,
        page_fit: args.coverage_page_fit,
        tty: headlamp_core::format::terminal::is_output_terminal(),
        editor_cmd: args.editor_cmd.clone(),
    };
    println!("{}", format_compact(&filtered, &print_opts, repo_root));
    if let Some(detail) = args.coverage_detail
        && detail != headlamp_core::args::CoverageDetail::Auto
    {
        let hs = format_hotspots(&filtered, &print_opts, repo_root);
        if !hs.trim().is_empty() {
            println!("{hs}");
        }
    };
}

fn has_cargo_llvm_cov(repo_root: &Path) -> bool {
    duct_cmd("cargo", ["llvm-cov", "--version"])
        .dir(repo_root)
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
            selected_test_count: Some(0),
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
        selected_test_count: Some(selected_count),
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
