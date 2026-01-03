use std::path::{Path, PathBuf};
use std::time::Instant;

use headlamp_core::args::ParsedArgs;
use headlamp_core::config::CoverageUi;

use crate::run::RunError;

use super::index::TestBinary;

pub(super) fn run_headlamp_rust_with_coverage(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
) -> Result<i32, RunError> {
    let started_at = Instant::now();
    super::run_optional_bootstrap(repo_root, args)?;

    let selection = derive_rust_coverage_selection(repo_root, args)?;
    let profraw_dir = create_profraw_dir(args.keep_artifacts, repo_root, session)?;
    let instrumented_binaries = build_instrumented_test_binaries(
        repo_root,
        args,
        session,
        &selection.extra_cargo_args,
        &profraw_dir,
    )?;
    if instrumented_binaries.is_empty() {
        return Ok(0);
    }

    let (run_model, exit_code) = run_instrumented_binaries_and_render_run_model(
        repo_root,
        args,
        &instrumented_binaries,
        &profraw_dir,
        started_at,
    )?;

    if crate::rust_coverage::should_abort_coverage_after_run(args, &run_model) {
        return Ok(1);
    }

    export_coverage_reports(repo_root, args, session, &instrumented_binaries)?;
    Ok(finalize_exit_code_with_thresholds(
        repo_root, args, session, exit_code,
    ))
}

fn normalize_runner_exit_code(exit_code: i32) -> i32 {
    if exit_code == 0 { 0 } else { 1 }
}

fn derive_rust_coverage_selection(
    repo_root: &Path,
    args: &ParsedArgs,
) -> Result<crate::cargo::selection::CargoSelection, RunError> {
    let changed_files = super::changed_files_for_args(repo_root, args)?;
    Ok(crate::cargo::selection::derive_cargo_selection(
        repo_root,
        args,
        &changed_files,
    ))
}

fn create_profraw_dir(
    keep_artifacts: bool,
    repo_root: &Path,
    session: &crate::session::RunSession,
) -> Result<PathBuf, RunError> {
    let profraw_dir = profraw_dir_for_session(keep_artifacts, repo_root, session);
    std::fs::create_dir_all(&profraw_dir).map_err(RunError::Io)?;
    crate::rust_coverage::purge_profile_artifacts(&profraw_dir);
    Ok(profraw_dir)
}

fn run_instrumented_binaries_and_render_run_model(
    repo_root: &Path,
    args: &ParsedArgs,
    instrumented_binaries: &[TestBinary],
    profraw_dir: &Path,
    started_at: Instant,
) -> Result<(crate::test_model::TestRunModel, i32), RunError> {
    let libtest_filter = super::derive_libtest_filter(repo_root, args);
    let live_progress = super::start_live_progress(args, instrumented_binaries.len());
    let (suite_models, exit_code) = run_instrumented_binaries(
        repo_root,
        args,
        live_progress,
        instrumented_binaries,
        libtest_filter.as_deref(),
        profraw_dir,
    )?;

    let run_time_ms = started_at.elapsed().as_millis() as u64;
    let run_model =
        super::render_and_print_run_model(repo_root, args, suite_models, run_time_ms, exit_code);
    Ok((run_model, exit_code))
}

fn export_coverage_reports(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    instrumented_binaries: &[TestBinary],
) -> Result<(), RunError> {
    let coverage_paths =
        crate::rust_coverage::rust_coverage_paths(args.keep_artifacts, repo_root, session);
    let (toolchain, _enable_branch_coverage) =
        crate::rust_coverage::choose_llvm_tools_toolchain(repo_root);

    crate::rust_coverage::ensure_llvm_tools_available(repo_root, toolchain.as_str())?;
    crate::rust_coverage::merge_profraw_dir_to_profdata(
        repo_root,
        toolchain.as_str(),
        &coverage_paths.profraw_dir,
        &coverage_paths.profdata_path,
    )?;

    let objects = instrumented_binaries
        .iter()
        .map(|binary| binary.executable.clone())
        .collect::<Vec<_>>();
    crate::rust_coverage::export_llvm_cov_reports(
        repo_root,
        toolchain.as_str(),
        &coverage_paths.profdata_path,
        &objects,
        &coverage_paths.lcov_path,
        &coverage_paths.llvm_cov_json_path,
    )
}

fn finalize_exit_code_with_thresholds(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    exit_code: i32,
) -> i32 {
    if args.coverage_ui == CoverageUi::Jest {
        return normalize_runner_exit_code(exit_code);
    }

    let thresholds_failed = crate::cargo::coverage::print_lcov(repo_root, args, session);
    if normalize_runner_exit_code(exit_code) == 0 && thresholds_failed {
        1
    } else {
        normalize_runner_exit_code(exit_code)
    }
}

fn build_instrumented_test_binaries(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    extra_cargo_args: &[String],
    profraw_dir: &Path,
) -> Result<Vec<TestBinary>, RunError> {
    let use_nightly = crate::cargo::paths::nightly_rustc_exists(repo_root);
    let cargo_target_dir = instrumented_cargo_target_dir(args.keep_artifacts, repo_root, session);
    let rustflags = crate::rust_coverage::coverage_rustflags_with_branch_coverage(use_nightly);
    let build_profile_file =
        crate::rust_coverage::llvm_profile_file_pattern(profraw_dir, "headlamp-build");
    let built = super::cargo_build::build_test_binaries_via_cargo_no_run_with_overrides(
        repo_root,
        args,
        session,
        extra_cargo_args,
        &cargo_target_dir,
        &rustflags,
        Some(build_profile_file.as_os_str()),
    )?;
    crate::rust_coverage::purge_profile_artifacts(profraw_dir);
    Ok(built
        .into_iter()
        .map(|b| TestBinary {
            executable: b.executable,
            suite_source_path: b.suite_source_path,
        })
        .collect())
}

fn instrumented_cargo_target_dir(
    keep_artifacts: bool,
    repo_root: &Path,
    session: &crate::session::RunSession,
) -> PathBuf {
    if keep_artifacts {
        repo_root.join("target").join("headlamp-cargo-coverage")
    } else {
        session.subdir("cargo-target-coverage")
    }
}

fn run_instrumented_binaries(
    repo_root: &Path,
    args: &ParsedArgs,
    live_progress: crate::live_progress::LiveProgress,
    binaries: &[TestBinary],
    libtest_filter: Option<&str>,
    profraw_dir: &Path,
) -> Result<(Vec<crate::test_model::TestSuiteResult>, i32), RunError> {
    let use_libtest_json = crate::cargo::paths::nightly_rustc_exists(repo_root)
        && super::should_use_libtest_json_output(&args.runner_args);
    let test_binary_args = super::build_test_binary_args(args, libtest_filter, use_libtest_json);
    let mut suite_models: Vec<crate::test_model::TestSuiteResult> = vec![];
    let mut exit_code: i32 = 0;

    for binary in binaries {
        let llvm_profile_file =
            crate::rust_coverage::llvm_profile_file_pattern(profraw_dir, "headlamp");
        let (model, current_exit_code) = super::run_single_test_binary(
            repo_root,
            args,
            &live_progress,
            binary,
            &test_binary_args,
            Some(llvm_profile_file.as_os_str()),
            use_libtest_json,
        )?;
        if current_exit_code != 0 {
            exit_code = 1;
        }
        if let Some(model) = model {
            suite_models.extend(model.test_results);
        }
    }

    live_progress.finish();
    Ok((suite_models, exit_code))
}

fn profraw_dir_for_session(
    keep_artifacts: bool,
    repo_root: &Path,
    session: &crate::session::RunSession,
) -> PathBuf {
    if keep_artifacts {
        repo_root.join("coverage").join("rust").join("profraw")
    } else {
        session.subdir("coverage").join("rust").join("profraw")
    }
}
