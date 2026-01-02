use std::sync::{LazyLock, Mutex};

use crate::args::ParsedArgs;
use crate::config::{CoverageMode, CoverageUi};
use crate::session::RunSession;

static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn base_args() -> ParsedArgs {
    ParsedArgs {
        runner_args: vec![],
        selection_paths: vec![],
        selection_specified: false,
        keep_artifacts: false,
        watch: false,
        ci: true,
        verbose: false,
        quiet: false,
        no_cache: false,
        collect_coverage: true,
        coverage_ui: CoverageUi::Both,
        coverage_abort_on_failure: false,
        coverage_detail: None,
        coverage_show_code: false,
        coverage_mode: CoverageMode::Auto,
        coverage_max_files: None,
        coverage_max_hotspots: None,
        coverage_page_fit: false,
        coverage_thresholds: None,
        include_globs: vec![],
        exclude_globs: vec![],
        editor_cmd: None,
        workspace_root: None,
        only_failures: false,
        show_logs: false,
        sequential: true,
        bootstrap_command: None,
        changed: None,
        changed_depth: None,
        dependency_language: None,
    }
}

#[test]
fn parity_env_allows_reuse_instrumented_build_in_ci() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe {
        std::env::remove_var("HEADLAMP_PARITY_REUSE_INSTRUMENTED_BUILD");
    }
    assert!(!super::llvm_cov::should_reuse_instrumented_build(true));

    unsafe {
        std::env::set_var("HEADLAMP_PARITY_REUSE_INSTRUMENTED_BUILD", "1");
    }
    assert!(super::llvm_cov::should_reuse_instrumented_build(true));
    unsafe {
        std::env::remove_var("HEADLAMP_PARITY_REUSE_INSTRUMENTED_BUILD");
    }
}

#[test]
fn reuse_mode_skips_post_run_llvm_cov_reports() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe {
        std::env::remove_var("HEADLAMP_PARITY_REUSE_INSTRUMENTED_BUILD");
    }
    assert!(!super::llvm_cov::should_reuse_instrumented_build(true));
    unsafe { std::env::set_var("HEADLAMP_PARITY_REUSE_INSTRUMENTED_BUILD", "1") };
    assert!(super::llvm_cov::should_reuse_instrumented_build(true));
    unsafe { std::env::remove_var("HEADLAMP_PARITY_REUSE_INSTRUMENTED_BUILD") };
}

#[test]
fn purge_llvm_cov_profile_artifacts_removes_profraw_profdata_and_profraw_list() {
    let _guard = ENV_LOCK.lock().unwrap();

    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let llvm_cov_target = repo_root
        .join("target")
        .join("headlamp-cargo")
        .join("llvm-cov-target");
    std::fs::create_dir_all(&llvm_cov_target).unwrap();

    let profraw_path = llvm_cov_target.join("a.profraw");
    let profdata_path = llvm_cov_target.join("b.profdata");
    let profraw_list_path = llvm_cov_target.join("wt-123-0-profraw-list");
    let unrelated_path = llvm_cov_target.join("keep.txt");

    std::fs::write(&profraw_path, "x").unwrap();
    std::fs::write(&profdata_path, "x").unwrap();
    std::fs::write(&profraw_list_path, "x").unwrap();
    std::fs::write(&unrelated_path, "x").unwrap();

    let mut args = base_args();
    args.keep_artifacts = true;
    args.ci = false;
    let session = RunSession::new(true).unwrap();

    super::llvm_cov::purge_llvm_cov_profile_artifacts(repo_root, &args, &session);

    assert!(!profraw_path.exists());
    assert!(!profdata_path.exists());
    assert!(!profraw_list_path.exists());
    assert!(unrelated_path.exists());
}

#[test]
fn reuse_mode_skips_post_run_report_when_lcov_already_exists() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe {
        std::env::set_var("HEADLAMP_PARITY_REUSE_INSTRUMENTED_BUILD", "1");
    }

    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let session = RunSession::new(false).unwrap();
    let args = base_args();

    let lcov_path = session.subdir("coverage").join("rust").join("lcov.info");
    std::fs::create_dir_all(lcov_path.parent().unwrap()).unwrap();
    // Minimal valid lcov so `coverage::print_lcov` can parse it.
    std::fs::write(&lcov_path, "TN:\nSF:src/lib.rs\nDA:1,1\nend_of_record\n").unwrap();

    // If we incorrectly run a post-run `cargo llvm-cov report`, this will fail. If we correctly
    // skip it (because lcov already exists), this should succeed.
    let original_path = std::env::var("PATH").unwrap_or_default();
    unsafe {
        std::env::set_var("PATH", "");
    }
    let result =
        super::llvm_cov::finish_coverage_after_test_run(repo_root, &args, &session, 1, &[]);
    unsafe {
        std::env::set_var("PATH", original_path);
    }
    assert!(
        result.is_ok(),
        "expected ok result when lcov already exists"
    );

    unsafe {
        std::env::remove_var("HEADLAMP_PARITY_REUSE_INSTRUMENTED_BUILD");
    }
}

#[test]
fn coverage_requested_errors_when_lcov_is_missing_in_ci_reuse_mode() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe {
        std::env::set_var("HEADLAMP_PARITY_REUSE_INSTRUMENTED_BUILD", "1");
        std::env::set_var("PATH", "");
    }

    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();
    let session = RunSession::new(false).unwrap();
    let args = base_args();

    let result =
        super::llvm_cov::finish_coverage_after_test_run(repo_root, &args, &session, 1, &[]);
    assert!(
        result.is_err(),
        "expected error when coverage is requested but lcov is missing"
    );

    unsafe {
        std::env::remove_var("HEADLAMP_PARITY_REUSE_INSTRUMENTED_BUILD");
        std::env::remove_var("PATH");
    }
}
