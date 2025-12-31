use crate::args::ParsedArgs;
use crate::config::{CoverageMode, CoverageUi};
use crate::pytest::build_pytest_cmd_args;
use crate::session::RunSession;

fn base_args() -> ParsedArgs {
    ParsedArgs {
        runner_args: vec![],
        selection_paths: vec![],
        selection_specified: false,
        keep_artifacts: false,
        watch: false,
        ci: false,
        verbose: false,
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
fn pytest_cov_report_lcov_is_rewritten_into_session_when_keep_artifacts_false() {
    let session = RunSession::new(false).unwrap();
    let mut args = base_args();
    args.runner_args = vec![
        "--cov=.".to_string(),
        "--cov-report=lcov:coverage/lcov.info".to_string(),
    ];
    let cmd_args = build_pytest_cmd_args(&args, &session, &[]);
    assert!(cmd_args.iter().any(|t| t == "-p"));
    assert!(cmd_args.iter().any(|t| t == "no:cacheprovider"));
    assert!(
        cmd_args
            .iter()
            .filter(|t| t.starts_with("--cov-report=lcov:"))
            .all(|t| !t.contains("coverage/lcov.info"))
    );
}
