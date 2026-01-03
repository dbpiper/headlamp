use std::path::Path;
use std::time::Instant;

use headlamp_core::args::ParsedArgs;

use crate::live_progress::{LiveProgress, live_progress_mode};
use crate::run::RunError;
use crate::streaming::run_streaming_capture_tail_merged;

pub fn run_cargo_nextest(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
) -> Result<i32, RunError> {
    super::run_optional_bootstrap(repo_root, args)?;
    let changed = super::changed_files_for_args(repo_root, args)?;
    let selection = super::selection::derive_cargo_selection(repo_root, args, &changed);
    if let Some(exit_code) =
        super::early_exit_for_zero_changed_selection(repo_root, args, session, &selection)
    {
        return Ok(exit_code);
    }
    ensure_cargo_nextest_is_available(repo_root, args, session)?;
    let coverage_ctx =
        super::build_rust_coverage_context_if_enabled(repo_root, args, session, "cargo-nextest")?;
    let objects = coverage_ctx
        .as_ref()
        .map(|ctx| {
            super::build_instrumented_objects_for_rust_coverage(
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

    let run = run_nextest_streaming(
        repo_root,
        args,
        session,
        &selection.extra_cargo_args,
        coverage_ctx
            .as_ref()
            .map(|ctx| (&ctx.paths, ctx.llvm_profile_prefix)),
    )?;
    super::print_runner_tail_if_failed_without_tests(run.exit_code, &run.model, &run.tail);
    super::maybe_print_rendered_model(repo_root, args, run.exit_code, &run.model);
    if super::should_abort_coverage_after_run(args, &run.model) {
        return Ok(super::normalize_runner_exit_code(run.exit_code));
    }
    if let Some(ctx) = coverage_ctx.as_ref() {
        super::export_rust_coverage_reports(repo_root, ctx, &objects)?;
    }
    let final_exit =
        super::maybe_print_lcov_and_adjust_exit(repo_root, args, session, run.exit_code);
    Ok(final_exit)
}

fn ensure_cargo_nextest_is_available(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
) -> Result<(), RunError> {
    super::coverage::has_cargo_nextest(repo_root, args, session)
        .then_some(())
        .ok_or_else(|| RunError::MissingRunner {
            runner: "cargo-nextest".to_string(),
            hint: "expected `cargo nextest` to be installed and available".to_string(),
        })
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
    coverage: Option<(&crate::rust_coverage::RustCoveragePaths, &'static str)>,
) -> Result<NextestRunOutput, RunError> {
    let mode = live_progress_mode(
        headlamp_core::format::terminal::is_output_terminal(),
        args.ci,
        args.quiet,
    );
    let live_progress = LiveProgress::start(1, mode);
    let run_start = Instant::now();
    let cmd = build_nextest_command(repo_root, args, session, extra_cargo_args, coverage);
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
    let mut adapter = super::adapters::NextestAdapter::new(repo_root, args.only_failures);
    let (exit_code, tail) =
        run_streaming_capture_tail_merged(cmd, &live_progress, &mut adapter, 1024 * 1024)?;
    live_progress.increment_done(1);
    live_progress.finish();
    let super::adapters::NextestAdapter { parser, .. } = adapter;
    let model = parser
        .finalize()
        .unwrap_or_else(|| super::empty_test_run_model_for_exit_code(exit_code));
    let elapsed_ms = run_start.elapsed().as_millis() as u64;
    let model = super::apply_wall_clock_run_time_ms(model, elapsed_ms);
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
    coverage: Option<(&crate::rust_coverage::RustCoveragePaths, &'static str)>,
) -> std::process::Command {
    let mut cmd = std::process::Command::new("cargo");
    let use_nightly_rustc = super::paths::nightly_rustc_exists(repo_root);
    if use_nightly_rustc {
        cmd.arg("+nightly");
    }
    cmd.args(super::runner_args::build_nextest_run_args(
        None,
        args,
        extra_cargo_args,
    ));
    cmd.current_dir(repo_root);
    super::paths::apply_headlamp_cargo_target_dir(
        &mut cmd,
        args.keep_artifacts,
        repo_root,
        session,
    );
    cmd.env("NEXTEST_EXPERIMENTAL_LIBTEST_JSON", "1");
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
