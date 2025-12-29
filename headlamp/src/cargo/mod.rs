use std::path::Path;
use std::time::Instant;

use headlamp_core::args::ParsedArgs;
use headlamp_core::config::CoverageUi;
use headlamp_core::format::ctx::make_ctx;
use headlamp_core::format::vitest::render_vitest_from_test_model;

use crate::git::changed_files;
use crate::live_progress::{LiveProgress, live_progress_mode};
use crate::run::{RunError, run_bootstrap};
use crate::streaming::run_streaming_capture_tail;

mod adapters;
mod coverage;
mod llvm_cov;
mod model_norm;
mod paths;
mod runner_args;
mod selection;

pub use paths::build_cargo_llvm_cov_command_args;

pub(crate) use model_norm::empty_test_run_model_for_exit_code;

fn apply_wall_clock_run_time_ms(
    mut model: headlamp_core::test_model::TestRunModel,
    elapsed_ms: u64,
) -> headlamp_core::test_model::TestRunModel {
    model.aggregated.run_time_ms = Some(elapsed_ms);
    model
}

fn run_optional_bootstrap(repo_root: &Path, args: &ParsedArgs) -> Result<(), RunError> {
    let Some(command) = args.bootstrap_command.as_deref() else {
        return Ok(());
    };
    run_bootstrap(repo_root, command)
}

fn normalize_runner_exit_code(exit_code: i32) -> i32 {
    if exit_code == 0 { 0 } else { 1 }
}

fn print_runner_tail_if_failed_without_tests(
    exit_code: i32,
    model: &headlamp_core::test_model::TestRunModel,
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

pub fn run_cargo_test(repo_root: &Path, args: &ParsedArgs) -> Result<i32, RunError> {
    run_optional_bootstrap(repo_root, args)?;

    let changed = args
        .changed
        .map(|m| changed_files(repo_root, m))
        .transpose()?
        .unwrap_or_default();

    let selection = selection::derive_cargo_selection(repo_root, args, &changed);
    if selection.changed_selection_attempted
        && selection.selected_test_count == Some(0)
        && args.changed.is_some()
    {
        let changed_mode = args
            .changed
            .map(selection::changed_mode_to_cli_string)
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
            coverage::print_lcov(repo_root, args);
        }
        return Ok(0);
    }

    if args.collect_coverage && coverage::has_cargo_llvm_cov(repo_root) {
        let exit_code = llvm_cov::run_cargo_llvm_cov_test_and_render(
            repo_root,
            args,
            &selection.extra_cargo_args,
        )?;
        return llvm_cov::finish_coverage_after_test_run(repo_root, args, exit_code);
    }

    let mode = live_progress_mode(
        headlamp_core::format::terminal::is_output_terminal(),
        args.ci,
    );
    let live_progress = LiveProgress::start(1, mode);
    let run_start = Instant::now();
    let mut cmd = std::process::Command::new("cargo");
    cmd.args(runner_args::build_cargo_test_args(
        None,
        args,
        &selection.extra_cargo_args,
    ));
    cmd.current_dir(repo_root);
    paths::apply_headlamp_cargo_target_dir(&mut cmd, repo_root);
    cmd.env("RUST_BACKTRACE", "1");
    cmd.env("RUST_LIB_BACKTRACE", "1");
    let mut adapter = adapters::CargoTestAdapter::new(repo_root, args.only_failures);
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
    let model = model_norm::normalize_cargo_test_model_by_panic_locations(repo_root, model);
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
        let thresholds_failed = coverage::print_lcov(repo_root, args);
        if thresholds_failed {
            final_exit = 1;
        }
    }

    Ok(final_exit)
}

pub fn run_cargo_nextest(repo_root: &Path, args: &ParsedArgs) -> Result<i32, RunError> {
    run_optional_bootstrap(repo_root, args)?;

    let changed = args
        .changed
        .map(|m| changed_files(repo_root, m))
        .transpose()?
        .unwrap_or_default();

    let selection = selection::derive_cargo_selection(repo_root, args, &changed);
    if selection.changed_selection_attempted
        && selection.selected_test_count == Some(0)
        && args.changed.is_some()
    {
        let changed_mode = args
            .changed
            .map(selection::changed_mode_to_cli_string)
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
            let thresholds_failed = coverage::print_lcov(repo_root, args);
            return Ok(if thresholds_failed { 1 } else { 0 });
        }
        return Ok(0);
    }

    if args.collect_coverage && coverage::has_cargo_llvm_cov(repo_root) {
        if !coverage::has_cargo_nextest(repo_root) {
            return Err(RunError::MissingRunner {
                runner: "cargo-nextest".to_string(),
                hint: "expected `cargo nextest` to be installed and available".to_string(),
            });
        }
        let exit_code = llvm_cov::run_cargo_llvm_cov_nextest_and_render(
            repo_root,
            args,
            &selection.extra_cargo_args,
        )?;
        return llvm_cov::finish_coverage_after_test_run(repo_root, args, exit_code);
    }
    if !coverage::has_cargo_nextest(repo_root) {
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
    cmd.args(runner_args::build_nextest_run_args(
        None,
        args,
        &selection.extra_cargo_args,
    ));
    cmd.current_dir(repo_root);
    paths::apply_headlamp_cargo_target_dir(&mut cmd, repo_root);
    cmd.env("NEXTEST_EXPERIMENTAL_LIBTEST_JSON", "1");
    cmd.env("RUST_BACKTRACE", "1");
    cmd.env("RUST_LIB_BACKTRACE", "1");
    let mut adapter = adapters::NextestAdapter::new(repo_root, args.only_failures);
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
        let thresholds_failed = coverage::print_lcov(repo_root, args);
        if thresholds_failed {
            final_exit = 1;
        }
    }
    Ok(final_exit)
}
