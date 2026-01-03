use std::path::Path;
use std::time::Instant;

use headlamp_core::args::ParsedArgs;
use headlamp_core::config::CoverageUi;
use headlamp_core::format::ctx::make_ctx;
use headlamp_core::format::vitest::render_vitest_from_test_model;

use crate::git::changed_files;
use crate::live_progress::{LiveProgress, live_progress_mode};
use crate::run::{RunError, run_bootstrap};
use crate::streaming::run_streaming_capture_tail_merged;
use crate::test_model::TestRunModel;

mod adapters;
pub(crate) mod coverage;
#[cfg(test)]
mod coverage_abort_on_failure_semantics_test;
mod model_norm;
mod nextest;
pub(crate) mod paths;
mod run_trace;
mod runner_args;
#[cfg(test)]
mod runner_args_test;
#[cfg(test)]
mod rust_coverage_missing_test;
pub(crate) mod selection;

pub(crate) use model_norm::empty_test_run_model_for_exit_code;
pub use nextest::run_cargo_nextest;

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

struct RustCoverageContext {
    toolchain: String,
    enable_branch_coverage: bool,
    paths: crate::rust_coverage::RustCoveragePaths,
    llvm_profile_prefix: &'static str,
}

fn build_rust_coverage_context_if_enabled(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    llvm_profile_prefix: &'static str,
) -> Result<Option<RustCoverageContext>, RunError> {
    if !crate::rust_coverage::should_collect_rust_coverage(args) {
        return Ok(None);
    }
    let (toolchain, enable_branch_coverage) =
        crate::rust_coverage::choose_llvm_tools_toolchain(repo_root);
    crate::rust_coverage::ensure_llvm_tools_available(repo_root, toolchain.as_str())?;
    let paths = crate::rust_coverage::rust_coverage_paths(args.keep_artifacts, repo_root, session);
    let _ = std::fs::create_dir_all(&paths.profraw_dir);
    crate::rust_coverage::purge_profile_artifacts(&paths.profraw_dir);
    crate::rust_coverage::purge_profile_artifacts(
        paths.profdata_path.parent().unwrap_or(repo_root),
    );
    Ok(Some(RustCoverageContext {
        toolchain: toolchain.to_string(),
        enable_branch_coverage,
        paths,
        llvm_profile_prefix,
    }))
}

fn build_instrumented_objects_for_rust_coverage(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    extra_cargo_args: &[String],
    enable_branch_coverage: bool,
    profraw_dir: &std::path::Path,
    llvm_profile_prefix: &str,
) -> Result<Vec<std::path::PathBuf>, RunError> {
    let cargo_target_dir = crate::cargo::paths::headlamp_cargo_target_dir_for_duct(
        args.keep_artifacts,
        repo_root,
        session,
    );
    let rustflags =
        crate::rust_coverage::coverage_rustflags_with_branch_coverage(enable_branch_coverage);
    let build_profile_prefix = format!("{llvm_profile_prefix}-build");
    let build_profile_file =
        crate::rust_coverage::llvm_profile_file_pattern(profraw_dir, build_profile_prefix.as_str());
    let built =
        crate::rust_runner::cargo_build::build_test_binaries_via_cargo_no_run_with_overrides(
            repo_root,
            args,
            session,
            extra_cargo_args,
            &cargo_target_dir,
            &rustflags,
            Some(build_profile_file.as_os_str()),
        )?;
    crate::rust_coverage::purge_profile_artifacts(profraw_dir);
    Ok(built.into_iter().map(|b| b.executable).collect::<Vec<_>>())
}

fn export_rust_coverage_reports(
    repo_root: &Path,
    ctx: &RustCoverageContext,
    objects: &[std::path::PathBuf],
) -> Result<(), RunError> {
    crate::rust_coverage::merge_profraw_dir_to_profdata(
        repo_root,
        ctx.toolchain.as_str(),
        &ctx.paths.profraw_dir,
        &ctx.paths.profdata_path,
    )?;
    crate::rust_coverage::export_llvm_cov_reports(
        repo_root,
        ctx.toolchain.as_str(),
        &ctx.paths.profdata_path,
        objects,
        &ctx.paths.lcov_path,
        &ctx.paths.llvm_cov_json_path,
    )
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
    let coverage_ctx =
        build_rust_coverage_context_if_enabled(repo_root, args, session, "cargo-test")?;
    let objects = coverage_ctx
        .as_ref()
        .map(|ctx| {
            build_instrumented_objects_for_rust_coverage(
                repo_root,
                args,
                session,
                &selection.extra_cargo_args,
                ctx.enable_branch_coverage,
                &ctx.paths.profraw_dir,
                ctx.llvm_profile_prefix,
            )
        })
        .transpose()?
        .unwrap_or_default();

    let run = run_cargo_test_streaming(
        repo_root,
        args,
        session,
        &selection.extra_cargo_args,
        coverage_ctx
            .as_ref()
            .map(|ctx| (&ctx.paths, ctx.llvm_profile_prefix)),
    )?;
    print_runner_tail_if_failed_without_tests(run.exit_code, &run.model, &run.tail);
    maybe_print_rendered_model(repo_root, args, run.exit_code, &run.model);
    if should_abort_coverage_after_run(args, &run.model) {
        return Ok(run_trace::normalize_and_trace_cargo_test_coverage_abort(
            repo_root,
            args,
            started_at,
            changed.len(),
            &selection,
            run.exit_code,
        ));
    }
    if let Some(ctx) = coverage_ctx.as_ref() {
        export_rust_coverage_reports(repo_root, ctx, &objects)?;
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
    coverage: Option<(&crate::rust_coverage::RustCoveragePaths, &'static str)>,
) -> Result<CargoTestRunOutput, RunError> {
    let mode = live_progress_mode(
        headlamp_core::format::terminal::is_output_terminal(),
        args.ci,
        args.quiet,
    );
    let live_progress = LiveProgress::start(1, mode);
    let run_start = Instant::now();
    let cmd = build_cargo_test_command(repo_root, args, session, extra_cargo_args, coverage);
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
        run_streaming_capture_tail_merged(cmd, &live_progress, &mut adapter, 1024 * 1024)?;
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
    coverage: Option<(&crate::rust_coverage::RustCoveragePaths, &'static str)>,
) -> std::process::Command {
    let mut cmd = std::process::Command::new("cargo");
    let use_nightly_rustc = crate::cargo::paths::nightly_rustc_exists(repo_root);
    if use_nightly_rustc {
        cmd.arg("+nightly");
    }
    cmd.args(runner_args::build_cargo_test_args(
        None,
        args,
        extra_cargo_args,
    ));
    cmd.current_dir(repo_root);
    paths::apply_headlamp_cargo_target_dir(&mut cmd, args.keep_artifacts, repo_root, session);
    cmd.env("RUST_BACKTRACE", "1");
    cmd.env("RUST_LIB_BACKTRACE", "1");
    if let Some((paths, prefix)) = coverage {
        let _ = std::fs::create_dir_all(&paths.profraw_dir);
        let llvm_profile =
            crate::rust_coverage::llvm_profile_file_pattern(&paths.profraw_dir, prefix);
        cmd.env("LLVM_PROFILE_FILE", llvm_profile);
        let existing = std::env::var("RUSTFLAGS").unwrap_or_default();
        let enable_branch_coverage = use_nightly_rustc;
        let rustflags = crate::rust_coverage::append_rustflags(
            &existing,
            &crate::rust_coverage::coverage_rustflags_with_branch_coverage(enable_branch_coverage),
        );
        cmd.env("RUSTFLAGS", rustflags);
        cmd.env("CARGO_INCREMENTAL", "0");
    }
    cmd
}

fn cargo_model_has_failed_tests(model: &TestRunModel) -> bool {
    model.aggregated.num_failed_tests > 0 || model.aggregated.num_failed_test_suites > 0
}

pub(crate) fn should_abort_coverage_after_run(args: &ParsedArgs, model: &TestRunModel) -> bool {
    args.coverage_abort_on_failure && cargo_model_has_failed_tests(model)
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
    let coverage_requested = args.collect_coverage && args.coverage_ui != CoverageUi::Jest;
    let thresholds_failed = if coverage_requested {
        coverage::print_lcov(repo_root, args, session)
    } else {
        false
    };
    let normalized_exit_code = normalize_runner_exit_code(exit_code);
    if coverage_requested && !thresholds_failed {
        // `print_lcov` returns false both for "no thresholds failed" and for "could not print"
        // (e.g., lcov missing). If coverage was explicitly requested, never fail silently:
        // if we printed nothing due to missing coverage artifacts, fail with an actionable hint.
        //
        // Heuristic: if lcov is missing/unreadable, `print_lcov` returns false and there will be no
        // thresholds output either. In that case, force a non-zero exit so users notice.
        let expected_lcov_path = if args.keep_artifacts {
            repo_root.join("coverage").join("lcov.info")
        } else {
            session.subdir("coverage").join("rust").join("lcov.info")
        };
        if !expected_lcov_path.exists() {
            eprintln!(
                "headlamp: coverage was requested but Rust lcov was not generated (missing {}). \
Install `llvm-tools-preview` (rustup) and re-run.",
                expected_lcov_path.to_string_lossy()
            );
            return 1;
        }
    }
    if normalized_exit_code == 0 && thresholds_failed {
        1
    } else {
        normalized_exit_code
    }
}
