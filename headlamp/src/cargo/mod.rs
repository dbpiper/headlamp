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
mod run_trace;
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

pub fn run_cargo_test(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
) -> Result<i32, RunError> {
    let started_at = Instant::now();
    run_optional_bootstrap(repo_root, args)?;
    let changed = changed_files_for_args(repo_root, args)?;
    let selection = selection::derive_cargo_selection(repo_root, args, &changed);
    if early_exit_for_zero_changed_selection_cargo_test(repo_root, args, session, &selection) {
        run_trace::trace_cargo_test_early_exit(
            repo_root,
            args,
            started_at,
            changed.len(),
            selection.selected_test_count,
        );
        return Ok(0);
    }
    if let Some(exit_code) =
        maybe_run_cargo_test_with_llvm_cov(repo_root, args, session, &selection)?
    {
        return run_trace::finish_cargo_test_llvm_cov_and_trace(
            repo_root,
            args,
            session,
            started_at,
            changed.len(),
            &selection,
            exit_code,
        );
    }
    let run = run_cargo_test_streaming(repo_root, args, session, &selection.extra_cargo_args)?;
    print_runner_tail_if_failed_without_tests(run.exit_code, &run.model, &run.tail);
    maybe_print_rendered_model(repo_root, args, run.exit_code, &run.model);
    if args.coverage_abort_on_failure && run.exit_code != 0 {
        return Ok(run_trace::normalize_and_trace_cargo_test_coverage_abort(
            repo_root,
            args,
            started_at,
            changed.len(),
            &selection,
            run.exit_code,
        ));
    }
    let final_exit = maybe_print_lcov_and_adjust_exit(repo_root, args, session, run.exit_code);
    run_trace::trace_cargo_test_final_exit(
        repo_root,
        args,
        started_at,
        changed.len(),
        &selection,
        final_exit,
    );
    Ok(final_exit)
}

fn early_exit_for_zero_changed_selection_cargo_test(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    selection: &selection::CargoSelection,
) -> bool {
    let should_early_exit = selection.changed_selection_attempted
        && selection.selected_test_count == Some(0)
        && args.changed.is_some();
    if !should_early_exit {
        return false;
    }
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
    let _ = if args.collect_coverage && args.coverage_ui != CoverageUi::Jest {
        coverage::print_lcov(repo_root, args, session)
    } else {
        false
    };
    true
}

fn maybe_run_cargo_test_with_llvm_cov(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    selection: &selection::CargoSelection,
) -> Result<Option<i32>, RunError> {
    if !(args.collect_coverage && coverage::has_cargo_llvm_cov(repo_root, args, session)) {
        return Ok(None);
    }
    let exit_code = llvm_cov::run_cargo_llvm_cov_test_and_render(
        repo_root,
        args,
        session,
        &selection.extra_cargo_args,
    )?;
    Ok(Some(exit_code))
}

#[derive(Debug)]
struct CargoTestRunOutput {
    exit_code: i32,
    model: headlamp_core::test_model::TestRunModel,
    tail: crate::streaming::RingBuffer,
}

fn run_cargo_test_streaming(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    extra_cargo_args: &[String],
) -> Result<CargoTestRunOutput, RunError> {
    let mode = live_progress_mode(
        headlamp_core::format::terminal::is_output_terminal(),
        args.ci,
    );
    let live_progress = LiveProgress::start(1, mode);
    let run_start = Instant::now();
    let cmd = build_cargo_test_command(repo_root, args, session, extra_cargo_args);
    headlamp_core::diagnostics_trace::maybe_write_run_trace(
        repo_root,
        "cargo-test",
        args,
        Some(run_start),
        serde_json::json!({
            "phase": "before_run_streaming_capture_tail",
            "command": headlamp_core::diagnostics_trace::command_summary_json(&cmd),
        }),
    );
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
    Ok(CargoTestRunOutput {
        exit_code,
        model,
        tail,
    })
}

fn build_cargo_test_command(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    extra_cargo_args: &[String],
) -> std::process::Command {
    let mut cmd = std::process::Command::new("cargo");
    cmd.args(runner_args::build_cargo_test_args(
        None,
        args,
        extra_cargo_args,
    ));
    cmd.current_dir(repo_root);
    paths::apply_headlamp_cargo_target_dir(&mut cmd, args.keep_artifacts, repo_root, session);
    cmd.env("RUST_BACKTRACE", "1");
    cmd.env("RUST_LIB_BACKTRACE", "1");
    cmd
}

pub fn run_cargo_nextest(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
) -> Result<i32, RunError> {
    run_optional_bootstrap(repo_root, args)?;
    let changed = changed_files_for_args(repo_root, args)?;
    let selection = selection::derive_cargo_selection(repo_root, args, &changed);
    if let Some(exit_code) =
        early_exit_for_zero_changed_selection(repo_root, args, session, &selection)
    {
        return Ok(exit_code);
    }
    ensure_cargo_nextest_is_available(repo_root, args, session)?;
    if let Some(exit_code) = maybe_run_nextest_with_llvm_cov(repo_root, args, session, &selection)?
    {
        return llvm_cov::finish_coverage_after_test_run(
            repo_root,
            args,
            session,
            exit_code,
            &selection.extra_cargo_args,
        );
    }
    let run = run_nextest_streaming(repo_root, args, session, &selection.extra_cargo_args)?;
    print_runner_tail_if_failed_without_tests(run.exit_code, &run.model, &run.tail);
    maybe_print_rendered_model(repo_root, args, run.exit_code, &run.model);
    if args.coverage_abort_on_failure && run.exit_code != 0 {
        return Ok(normalize_runner_exit_code(run.exit_code));
    }
    let final_exit = maybe_print_lcov_and_adjust_exit(repo_root, args, session, run.exit_code);
    Ok(final_exit)
}

fn changed_files_for_args(
    repo_root: &Path,
    args: &ParsedArgs,
) -> Result<Vec<std::path::PathBuf>, RunError> {
    args.changed
        .map(|mode| changed_files(repo_root, mode))
        .transpose()
        .map(|v| v.unwrap_or_default())
}

fn early_exit_for_zero_changed_selection(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    selection: &selection::CargoSelection,
) -> Option<i32> {
    let should_early_exit = selection.changed_selection_attempted
        && selection.selected_test_count == Some(0)
        && args.changed.is_some();
    if !should_early_exit {
        return None;
    }
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
    let thresholds_failed = if args.collect_coverage && args.coverage_ui != CoverageUi::Jest {
        coverage::print_lcov(repo_root, args, session)
    } else {
        false
    };
    Some(thresholds_failed as i32)
}

fn ensure_cargo_nextest_is_available(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
) -> Result<(), RunError> {
    coverage::has_cargo_nextest(repo_root, args, session)
        .then_some(())
        .ok_or_else(|| RunError::MissingRunner {
            runner: "cargo-nextest".to_string(),
            hint: "expected `cargo nextest` to be installed and available".to_string(),
        })
}

fn maybe_run_nextest_with_llvm_cov(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    selection: &selection::CargoSelection,
) -> Result<Option<i32>, RunError> {
    if !(args.collect_coverage && coverage::has_cargo_llvm_cov(repo_root, args, session)) {
        return Ok(None);
    }
    let exit_code = llvm_cov::run_cargo_llvm_cov_nextest_and_render(
        repo_root,
        args,
        session,
        &selection.extra_cargo_args,
    )?;
    Ok(Some(exit_code))
}

#[derive(Debug)]
struct NextestRunOutput {
    exit_code: i32,
    model: headlamp_core::test_model::TestRunModel,
    tail: crate::streaming::RingBuffer,
}

fn run_nextest_streaming(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    extra_cargo_args: &[String],
) -> Result<NextestRunOutput, RunError> {
    let mode = live_progress_mode(
        headlamp_core::format::terminal::is_output_terminal(),
        args.ci,
    );
    let live_progress = LiveProgress::start(1, mode);
    let run_start = Instant::now();
    let cmd = build_nextest_command(repo_root, args, session, extra_cargo_args);
    headlamp_core::diagnostics_trace::maybe_write_run_trace(
        repo_root,
        "cargo-nextest",
        args,
        Some(run_start),
        serde_json::json!({
            "phase": "before_run_streaming_capture_tail",
            "command": headlamp_core::diagnostics_trace::command_summary_json(&cmd),
        }),
    );
    let mut adapter = adapters::NextestAdapter::new(repo_root, args.only_failures);
    let (exit_code, tail) =
        run_streaming_capture_tail(cmd, &live_progress, &mut adapter, 1024 * 1024)?;
    live_progress.increment_done(1);
    live_progress.finish();
    let adapters::NextestAdapter { parser, .. } = adapter;
    let model = parser
        .finalize()
        .unwrap_or_else(|| empty_test_run_model_for_exit_code(exit_code));
    let elapsed_ms = run_start.elapsed().as_millis() as u64;
    let model = apply_wall_clock_run_time_ms(model, elapsed_ms);
    Ok(NextestRunOutput {
        exit_code,
        model,
        tail,
    })
}

fn build_nextest_command(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    extra_cargo_args: &[String],
) -> std::process::Command {
    let mut cmd = std::process::Command::new("cargo");
    cmd.args(runner_args::build_nextest_run_args(
        None,
        args,
        extra_cargo_args,
    ));
    cmd.current_dir(repo_root);
    paths::apply_headlamp_cargo_target_dir(&mut cmd, args.keep_artifacts, repo_root, session);
    cmd.env("NEXTEST_EXPERIMENTAL_LIBTEST_JSON", "1");
    cmd.env("RUST_BACKTRACE", "1");
    cmd.env("RUST_LIB_BACKTRACE", "1");
    cmd
}

fn maybe_print_rendered_model(
    repo_root: &Path,
    args: &ParsedArgs,
    exit_code: i32,
    model: &headlamp_core::test_model::TestRunModel,
) {
    let ctx = make_ctx(
        repo_root,
        None,
        exit_code != 0,
        args.show_logs,
        args.editor_cmd.clone(),
    );
    let rendered = render_vitest_from_test_model(model, &ctx, args.only_failures);
    if !rendered.trim().is_empty() {
        println!("{rendered}");
    }
}

fn maybe_print_lcov_and_adjust_exit(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    exit_code: i32,
) -> i32 {
    let thresholds_failed = if args.collect_coverage && args.coverage_ui != CoverageUi::Jest {
        coverage::print_lcov(repo_root, args, session)
    } else {
        false
    };
    let normalized_exit_code = normalize_runner_exit_code(exit_code);
    if normalized_exit_code == 0 && thresholds_failed {
        1
    } else {
        normalized_exit_code
    }
}
