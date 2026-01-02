use std::path::Path;
use std::time::Instant;

use duct::cmd as duct_cmd;

use headlamp_core::args::ParsedArgs;
use headlamp_core::config::CoverageUi;
use headlamp_core::format::ctx::make_ctx;
use headlamp_core::format::vitest::render_vitest_from_test_model;

use crate::live_progress::{LiveProgress, live_progress_mode};
use crate::profile;
use crate::run::RunError;
use crate::streaming::run_streaming_capture_tail_merged;

use super::adapters::{CargoTestAdapter, NextestAdapter};
use super::coverage;
use super::model_norm::empty_test_run_model_for_exit_code;
use super::paths::{
    apply_headlamp_cargo_target_dir, build_cargo_llvm_cov_command_args, can_use_nightly,
    headlamp_cargo_target_dir_for_duct,
};
use super::runner_args::{build_llvm_cov_nextest_run_args, build_llvm_cov_test_run_args};

#[derive(Debug)]
pub(super) struct LlvmCovRunOutput {
    pub(super) exit_code: i32,
    pub(super) model: headlamp_core::test_model::TestRunModel,
}

pub(super) fn should_reuse_instrumented_build(ci: bool) -> bool {
    !ci || std::env::var_os("HEADLAMP_PARITY_REUSE_INSTRUMENTED_BUILD").is_some()
}

fn lcov_output_path_for_session(
    keep_artifacts: bool,
    repo_root: &Path,
    session: &crate::session::RunSession,
) -> std::path::PathBuf {
    if keep_artifacts {
        repo_root.join("coverage").join("lcov.info")
    } else {
        session.subdir("coverage").join("rust").join("lcov.info")
    }
}

fn should_run_post_run_llvm_cov_reports(
    ci: bool,
    reuse_instrumented_build: bool,
    lcov_path_exists: bool,
) -> bool {
    // If the instrumented run already wrote the lcov report to disk, do not run an additional
    // `cargo llvm-cov report`: in reuse mode this can be flaky because cargo-llvm-cov may clean up
    // intermediate profile artifacts after it produces the report.
    if lcov_path_exists {
        return false;
    }
    // Otherwise, generate lcov (and optional json) via a post-run report step.
    // This is needed for the non-reuse path which uses `--no-report` during the instrumented run.
    !(ci && reuse_instrumented_build)
}

pub(super) fn purge_llvm_cov_profile_artifacts(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
) {
    let root = headlamp_cargo_target_dir_for_duct(args.keep_artifacts, repo_root, session)
        .join("llvm-cov-target");
    fn purge_dir(dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(ty) = entry.file_type() else {
                continue;
            };
            if ty.is_dir() {
                purge_dir(&path);
                continue;
            }
            if !ty.is_file() {
                continue;
            }
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            let file_name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            let is_profraw_list = file_name.ends_with("-profraw-list");
            if matches!(ext, "profraw" | "profdata") || is_profraw_list {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    purge_dir(&root);
}

pub(super) fn finish_coverage_after_test_run(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    exit_code: i32,
    _extra_cargo_args: &[String],
) -> Result<i32, RunError> {
    let reuse_instrumented_build = should_reuse_instrumented_build(args.ci);
    let lcov_path = lcov_output_path_for_session(args.keep_artifacts, repo_root, session);
    let lcov_path_exists = lcov_path.exists();
    if should_run_post_run_llvm_cov_reports(args.ci, reuse_instrumented_build, lcov_path_exists) {
        let _span = profile::span("cargo llvm-cov report (lcov+json)");
        let report_scope_args = extract_llvm_cov_report_scope_args(&args.runner_args);
        run_cargo_llvm_cov_report_lcov(repo_root, args, session, &report_scope_args)?;
        if std::env::var_os("HEADLAMP_PARITY_SKIP_LLVM_COV_JSON").is_none() {
            run_cargo_llvm_cov_report_json(repo_root, args, session, &report_scope_args)?;
        }
    }
    if args.coverage_ui != CoverageUi::Jest {
        let thresholds_failed = coverage::print_lcov(repo_root, args, session);
        if args.collect_coverage && !lcov_path.exists() {
            return Err(RunError::CommandFailed {
                message: format!(
                    "coverage requested but rust lcov file was not generated: {}",
                    lcov_path.display()
                ),
            });
        }
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

pub(super) fn run_cargo_llvm_cov_test_and_render(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    extra_cargo_args: &[String],
) -> Result<LlvmCovRunOutput, RunError> {
    let mode = live_progress_mode(
        headlamp_core::format::terminal::is_output_terminal(),
        args.ci,
        args.quiet,
    );
    let live_progress = LiveProgress::start(1, mode);
    let run_start = Instant::now();

    let use_nightly = can_use_nightly(repo_root);
    let enable_branch_coverage = use_nightly;
    let reuse_instrumented_build = should_reuse_instrumented_build(args.ci);
    if reuse_instrumented_build {
        // When reusing the instrumented target dir, cargo-llvm-cov can leave old profraw/profdata around.
        // Purge only those artifacts (fast) while keeping compiled instrumented objects.
        purge_llvm_cov_profile_artifacts(repo_root, args, session);
    }
    let mut cmd = std::process::Command::new("cargo");
    cmd.args(build_cargo_llvm_cov_command_args(
        enable_branch_coverage,
        use_nightly,
        reuse_instrumented_build,
        &build_llvm_cov_test_run_args(args, session, extra_cargo_args, reuse_instrumented_build),
    ));
    cmd.current_dir(repo_root);
    apply_headlamp_cargo_target_dir(&mut cmd, args.keep_artifacts, repo_root, session);
    cmd.env("RUST_BACKTRACE", "1");
    cmd.env("RUST_LIB_BACKTRACE", "1");

    let mut adapter = CargoTestAdapter::new(repo_root, args.only_failures);
    let (exit_code, _tail) = {
        let _span = profile::span("cargo llvm-cov test (instrumented run)");
        run_streaming_capture_tail_merged(cmd, &live_progress, &mut adapter, 1024 * 1024)?
    };
    live_progress.increment_done(1);
    live_progress.finish();

    let model = adapter
        .parser
        .finalize()
        .unwrap_or_else(|| empty_test_run_model_for_exit_code(exit_code));
    let elapsed_ms = run_start.elapsed().as_millis() as u64;
    let mut model = model;
    model.aggregated.run_time_ms = Some(elapsed_ms);
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
    Ok(LlvmCovRunOutput { exit_code, model })
}

pub(super) fn run_cargo_llvm_cov_nextest_and_render(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    extra_cargo_args: &[String],
) -> Result<LlvmCovRunOutput, RunError> {
    let mode = live_progress_mode(
        headlamp_core::format::terminal::is_output_terminal(),
        args.ci,
        args.quiet,
    );
    let live_progress = LiveProgress::start(1, mode);
    let run_start = Instant::now();

    let use_nightly = can_use_nightly(repo_root);
    let enable_branch_coverage = use_nightly;
    let reuse_instrumented_build = should_reuse_instrumented_build(args.ci);
    if reuse_instrumented_build {
        purge_llvm_cov_profile_artifacts(repo_root, args, session);
    }
    let mut cmd = std::process::Command::new("cargo");
    cmd.args(build_cargo_llvm_cov_command_args(
        enable_branch_coverage,
        use_nightly,
        reuse_instrumented_build,
        &build_llvm_cov_nextest_run_args(args, session, extra_cargo_args, reuse_instrumented_build),
    ));
    cmd.current_dir(repo_root);
    apply_headlamp_cargo_target_dir(&mut cmd, args.keep_artifacts, repo_root, session);
    cmd.env("NEXTEST_EXPERIMENTAL_LIBTEST_JSON", "1");
    cmd.env("RUST_BACKTRACE", "1");
    cmd.env("RUST_LIB_BACKTRACE", "1");

    let mut adapter = NextestAdapter::new(repo_root, args.only_failures);
    let (exit_code, _tail) = {
        let _span = profile::span("cargo llvm-cov nextest (instrumented run)");
        run_streaming_capture_tail_merged(cmd, &live_progress, &mut adapter, 1024 * 1024)?
    };
    live_progress.increment_done(1);
    live_progress.finish();

    let model = adapter
        .parser
        .clone()
        .finalize()
        .unwrap_or_else(|| empty_test_run_model_for_exit_code(exit_code));
    let elapsed_ms = run_start.elapsed().as_millis() as u64;
    let mut model = model;
    model.aggregated.run_time_ms = Some(elapsed_ms);
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
    Ok(LlvmCovRunOutput { exit_code, model })
}

fn extract_llvm_cov_report_scope_args(runner_args: &[String]) -> Vec<String> {
    fn is_value_flag_needing_arg(flag: &str) -> bool {
        matches!(
            flag,
            "-p" | "--package" | "--exclude" | "--features" | "--target" | "--manifest-path"
        )
    }

    let mut out: Vec<String> = vec![];
    let mut iter = runner_args.iter().peekable();
    while let Some(token) = iter.next() {
        if token == "--" {
            break;
        }
        if token == "--workspace"
            || token == "--all-features"
            || token == "--no-default-features"
            || token == "--release"
        {
            out.push(token.clone());
            continue;
        }
        if is_value_flag_needing_arg(token) {
            if let Some(value) = iter.next() {
                out.push(token.clone());
                out.push(value.clone());
            }
            continue;
        }
        if token.starts_with("--package=")
            || token.starts_with("--exclude=")
            || token.starts_with("--features=")
            || token.starts_with("--target=")
            || token.starts_with("--manifest-path=")
        {
            out.push(token.clone());
            continue;
        }
    }
    out
}

fn run_cargo_llvm_cov_report_lcov(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    report_scope_args: &[String],
) -> Result<(), RunError> {
    let cargo_target_dir =
        headlamp_cargo_target_dir_for_duct(args.keep_artifacts, repo_root, session);
    let use_nightly = can_use_nightly(repo_root);
    let enable_branch_coverage = use_nightly;
    let out_path = if args.keep_artifacts {
        repo_root.join("coverage").join("lcov.info")
    } else {
        session.subdir("coverage").join("rust").join("lcov.info")
    };
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(RunError::Io)?;
    }
    let mut subcommand_args: Vec<String> = vec![
        "report".to_string(),
        "--lcov".to_string(),
        "--output-path".to_string(),
        out_path.to_string_lossy().to_string(),
        "--color".to_string(),
        "never".to_string(),
    ];
    subcommand_args.extend(report_scope_args.iter().cloned());
    let args = build_cargo_llvm_cov_command_args(
        enable_branch_coverage,
        use_nightly,
        false,
        &subcommand_args,
    );
    let out = duct_cmd("cargo", args)
        .dir(repo_root)
        .env("CARGO_TARGET_DIR", cargo_target_dir)
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
                "cargo llvm-cov report failed (exit={}):\n{}",
                out.status.code().unwrap_or(1),
                String::from_utf8_lossy(&out.stdout)
            ),
        })
    }
}

fn run_cargo_llvm_cov_report_json(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    report_scope_args: &[String],
) -> Result<(), RunError> {
    let cargo_target_dir =
        headlamp_cargo_target_dir_for_duct(args.keep_artifacts, repo_root, session);
    let use_nightly = can_use_nightly(repo_root);
    let enable_branch_coverage = use_nightly;
    let out_path = if args.keep_artifacts {
        repo_root.join("coverage").join("coverage.json")
    } else {
        session
            .subdir("coverage")
            .join("rust")
            .join("coverage.json")
    };
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(RunError::Io)?;
    }
    let mut subcommand_args: Vec<String> = vec![
        "report".to_string(),
        "--json".to_string(),
        "--output-path".to_string(),
        out_path.to_string_lossy().to_string(),
        "--color".to_string(),
        "never".to_string(),
    ];
    subcommand_args.extend(report_scope_args.iter().cloned());
    let args = build_cargo_llvm_cov_command_args(
        enable_branch_coverage,
        use_nightly,
        false,
        &subcommand_args,
    );
    let out = duct_cmd("cargo", args)
        .dir(repo_root)
        .env("CARGO_TARGET_DIR", cargo_target_dir)
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
                "cargo llvm-cov report --json failed (exit={}):\n{}",
                out.status.code().unwrap_or(1),
                String::from_utf8_lossy(&out.stdout)
            ),
        })
    }
}
