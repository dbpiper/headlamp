use crate::args::ParsedArgs;
use crate::config::{CoverageMode, CoverageUi};
use crate::session::RunSession;

fn base_args_with_coverage() -> ParsedArgs {
    ParsedArgs {
        runner_args: vec![],
        selection_paths: vec![],
        selection_specified: false,
        keep_artifacts: false,
        watch: false,
        ci: false,
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
        sequential: false,
        bootstrap_command: None,
        changed: None,
        changed_depth: None,
        dependency_language: None,
    }
}

#[test]
fn coverage_requested_errors_when_rust_lcov_is_missing_in_non_llvm_cov_path() {
    let repo_root = tempfile::tempdir().unwrap();
    let session = RunSession::new(false).unwrap();
    let args = base_args_with_coverage();

    // No lcov exists under the session dir. This should not be silently ignored when coverage was requested.
    let exit_code = super::maybe_print_lcov_and_adjust_exit(repo_root.path(), &args, &session, 0);
    assert_eq!(exit_code, 1);
}
