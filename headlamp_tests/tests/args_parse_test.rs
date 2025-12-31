use headlamp::args::{CoverageDetail, config_tokens, derive_args};
use headlamp::config::{CoverageUi, HeadlampConfig};

#[test]
fn derives_basic_flags_and_selection() {
    let cfg = HeadlampConfig::default();
    let argv = vec![
        "--coverage".to_string(),
        "--only-failures".to_string(),
        "src/foo.ts".to_string(),
        "-t".to_string(),
        "UserCard".to_string(),
    ];
    let cfg_tokens = config_tokens(&cfg, &argv);
    let parsed = derive_args(&cfg_tokens, &argv, true);
    assert!(parsed.collect_coverage);
    assert!(parsed.only_failures);
    assert!(parsed.selection_specified);
    assert!(
        parsed
            .selection_paths
            .iter()
            .any(|p| p.contains("src/foo.ts"))
    );
}

#[test]
fn parses_coverage_detail_values() {
    let cfg = HeadlampConfig::default();
    let argv = vec!["--coverage-detail=all".to_string()];
    let parsed = derive_args(&config_tokens(&cfg, &argv), &argv, true);
    assert_eq!(parsed.coverage_detail, Some(CoverageDetail::All));
}

#[test]
fn parses_coverage_detail_values_via_legacy_flag() {
    let cfg = HeadlampConfig::default();
    let argv = vec!["--coverage.detail=all".to_string()];
    let parsed = derive_args(&config_tokens(&cfg, &argv), &argv, true);
    assert_eq!(parsed.coverage_detail, Some(CoverageDetail::All));
}

#[test]
fn config_tokens_apply_coverage_ui_only_when_coverage_active() {
    let cfg = HeadlampConfig {
        coverage_ui: Some(CoverageUi::Jest),
        ..Default::default()
    };

    let argv = vec![];
    let cfg_tokens = config_tokens(&cfg, &argv);
    assert!(cfg_tokens.iter().all(|t| !t.starts_with("--coverage-ui=")));

    let argv2 = vec!["--coverage".to_string()];
    let cfg_tokens2 = config_tokens(&cfg, &argv2);
    assert!(cfg_tokens2.iter().any(|t| t == "--coverage-ui=jest"));
}

#[test]
fn derive_args_splits_intermixed_headlamp_flags_from_runner_args() {
    let cfg = HeadlampConfig::default();
    let argv = vec![
        "-t".to_string(),
        "UserCard".to_string(),
        "--coverage".to_string(),
        "src/foo.ts".to_string(),
        "--show-logs".to_string(),
    ];
    let cfg_tokens = config_tokens(&cfg, &argv);
    let parsed = derive_args(&cfg_tokens, &argv, true);
    assert!(parsed.collect_coverage);
    assert!(parsed.show_logs);
    assert!(parsed.runner_args.iter().any(|t| t == "-t"));
    assert!(parsed.runner_args.iter().any(|t| t == "UserCard"));
    assert!(parsed.selection_paths.iter().any(|p| p == "src/foo.ts"));
}

#[test]
fn artifacts_are_ephemeral_by_default() {
    let cfg = HeadlampConfig::default();
    let argv: Vec<String> = vec![];
    let parsed = derive_args(&config_tokens(&cfg, &argv), &argv, true);
    assert!(!parsed.keep_artifacts);
}

#[test]
fn keep_artifacts_can_be_enabled_by_cli_flag() {
    let cfg = HeadlampConfig::default();
    let argv = vec!["--keep-artifacts".to_string()];
    let parsed = derive_args(&config_tokens(&cfg, &argv), &argv, true);
    assert!(parsed.keep_artifacts);
}

#[test]
fn keep_artifacts_can_be_enabled_by_legacy_cli_flag() {
    let cfg = HeadlampConfig::default();
    let argv = vec!["--keepArtifacts".to_string()];
    let parsed = derive_args(&config_tokens(&cfg, &argv), &argv, true);
    assert!(parsed.keep_artifacts);
}

#[test]
fn keep_artifacts_can_be_enabled_by_config() {
    let cfg = HeadlampConfig {
        keep_artifacts: Some(true),
        ..Default::default()
    };
    let argv: Vec<String> = vec![];
    let cfg_tokens = config_tokens(&cfg, &argv);
    assert!(cfg_tokens.iter().any(|t| t == "--keep-artifacts"));
    let parsed = derive_args(&cfg_tokens, &argv, true);
    assert!(parsed.keep_artifacts);
}

#[test]
fn derive_args_changed_optional_value_does_not_consume_next_flag() {
    let cfg = HeadlampConfig::default();
    let argv = vec![
        "--changed".to_string(),
        "--show-logs".to_string(),
        "-t".to_string(),
        "UserCard".to_string(),
    ];
    let cfg_tokens = config_tokens(&cfg, &argv);
    let parsed = derive_args(&cfg_tokens, &argv, true);
    assert_eq!(parsed.changed, Some(headlamp::config::ChangedMode::All));
    assert!(parsed.show_logs);
    assert!(parsed.runner_args.iter().any(|t| t == "-t"));
}

#[test]
fn derive_args_does_not_treat_runner_flags_with_slashes_as_selection_paths() {
    let cfg = HeadlampConfig::default();
    let argv = vec![
        "--coverage".to_string(),
        "--".to_string(),
        "--cov=src/models".to_string(),
        "--cov-report=term-missing".to_string(),
        "--cov-report=lcov:coverage/lcov.info".to_string(),
        "-p".to_string(),
        "deal".to_string(),
    ];
    let parsed = derive_args(&config_tokens(&cfg, &argv), &argv, true);
    assert!(parsed.collect_coverage);
    assert!(parsed.selection_specified);
    assert!(
        !parsed
            .selection_paths
            .iter()
            .any(|p| p.starts_with("--cov"))
    );
    assert!(parsed.runner_args.iter().any(|t| t == "--cov=src/models"));
    assert!(
        parsed
            .runner_args
            .iter()
            .any(|t| t == "--cov-report=lcov:coverage/lcov.info")
    );
}

#[test]
fn windows_style_test_path_is_treated_as_selection() {
    let cfg = HeadlampConfig::default();
    let argv = vec![
        "--coverage".to_string(),
        "tests\\foo.test.ts".to_string(),
        "-t".to_string(),
        "UserCard".to_string(),
    ];
    let cfg_tokens = config_tokens(&cfg, &argv);
    let parsed = derive_args(&cfg_tokens, &argv, true);
    assert!(parsed.collect_coverage);
    assert!(parsed.selection_specified);
    assert!(
        parsed
            .selection_paths
            .iter()
            .any(|p| p == "tests\\foo.test.ts")
    );
}

#[test]
fn windows_style_src_path_is_treated_as_selection() {
    let cfg = HeadlampConfig::default();
    let argv = vec!["src\\foo.ts".to_string()];
    let cfg_tokens = config_tokens(&cfg, &argv);
    let parsed = derive_args(&cfg_tokens, &argv, true);
    assert!(parsed.selection_specified);
    assert!(parsed.selection_paths.iter().any(|p| p == "src\\foo.ts"));
    assert!(!parsed.include_globs.iter().any(|g| g == "src/**/*"));
}

#[test]
fn derive_args_sets_selection_specified_for_test_name_filters() {
    let cfg = HeadlampConfig::default();
    let argv = vec!["-t".to_string(), "UserCard".to_string()];
    let parsed = derive_args(&config_tokens(&cfg, &argv), &argv, true);
    assert!(parsed.selection_specified);
    assert!(parsed.selection_paths.is_empty());
    assert!(parsed.runner_args.iter().any(|t| t == "-t"));
    assert!(parsed.runner_args.iter().any(|t| t == "UserCard"));
    assert_eq!(parsed.coverage_detail, Some(CoverageDetail::Auto));
    assert_eq!(
        parsed.coverage_mode,
        headlamp::config::CoverageMode::Compact
    );
}

#[test]
fn derive_args_does_not_treat_test_path_pattern_value_as_selection_path() {
    let cfg = HeadlampConfig::default();
    let argv = vec![
        "--testPathPattern".to_string(),
        "src/foo.ts".to_string(),
        "-t".to_string(),
        "UserCard".to_string(),
    ];
    let parsed = derive_args(&config_tokens(&cfg, &argv), &argv, true);
    assert!(parsed.selection_specified);
    assert!(parsed.selection_paths.is_empty());
    assert!(parsed.runner_args.iter().any(|t| t == "--testPathPattern"));
    assert!(parsed.runner_args.iter().any(|t| t == "src/foo.ts"));
    assert!(parsed.runner_args.iter().any(|t| t == "-t"));
    assert!(parsed.runner_args.iter().any(|t| t == "UserCard"));
}

#[test]
fn derive_args_boolean_flags_can_consume_false_as_lookahead() {
    let cfg = HeadlampConfig::default();
    let argv = vec![
        "--sequential".to_string(),
        "false".to_string(),
        "--coverage-abort-on-failure".to_string(),
        "false".to_string(),
        "-t".to_string(),
        "UserCard".to_string(),
    ];
    let parsed = derive_args(&config_tokens(&cfg, &argv), &argv, true);
    assert!(!parsed.sequential);
    assert!(!parsed.coverage_abort_on_failure);
    assert!(parsed.selection_specified);
    assert!(parsed.runner_args.iter().any(|t| t == "-t"));
    assert!(parsed.runner_args.iter().any(|t| t == "UserCard"));
    assert!(!parsed.runner_args.iter().any(|t| t == "false"));
}

#[test]
fn cli_overrides_config_for_boolean_flags() {
    let cfg = HeadlampConfig {
        sequential: Some(true),
        ..Default::default()
    };
    let argv = vec!["--sequential=false".to_string()];
    let parsed = derive_args(&config_tokens(&cfg, &argv), &argv, true);
    assert!(!parsed.sequential);
}

#[test]
fn derive_args_does_not_consume_selection_path_as_boolean_value() {
    let cfg = HeadlampConfig::default();
    let argv = vec!["--sequential".to_string(), "src/a.js".to_string()];
    let parsed = derive_args(&config_tokens(&cfg, &argv), &argv, true);
    assert!(parsed.sequential);
    assert!(parsed.selection_specified);
    assert!(parsed.selection_paths.iter().any(|p| p == "src/a.js"));
}

#[test]
fn derive_args_parses_runner_agnostic_flags_and_does_not_forward_them() {
    let cfg = HeadlampConfig::default();
    let argv = vec![
        "--watch".to_string(),
        "--ci".to_string(),
        "--verbose".to_string(),
        "--no-cache".to_string(),
        "-t".to_string(),
        "UserCard".to_string(),
    ];
    let parsed = derive_args(&config_tokens(&cfg, &argv), &argv, true);
    assert!(!parsed.watch);
    assert!(parsed.ci);
    assert!(parsed.verbose);
    assert!(parsed.no_cache);
    assert!(parsed.runner_args.iter().any(|t| t == "-t"));
    assert!(!parsed.runner_args.iter().any(|t| t == "--watch"));
    assert!(!parsed.runner_args.iter().any(|t| t == "--ci"));
    assert!(!parsed.runner_args.iter().any(|t| t == "--verbose"));
    assert!(!parsed.runner_args.iter().any(|t| t == "--no-cache"));
}

#[test]
fn config_tokens_apply_runner_agnostic_flags() {
    let cfg = HeadlampConfig {
        ci: Some(true),
        verbose: Some(true),
        no_cache: Some(true),
        ..Default::default()
    };
    let argv = vec![];
    let cfg_tokens = config_tokens(&cfg, &argv);
    assert!(cfg_tokens.iter().any(|t| t == "--ci"));
    assert!(cfg_tokens.iter().any(|t| t == "--verbose"));
    assert!(cfg_tokens.iter().any(|t| t == "--no-cache"));
}

#[test]
fn double_dash_separator_forces_passthrough_even_for_headlamp_named_flags() {
    let cfg = HeadlampConfig::default();
    let argv = vec![
        "--verbose".to_string(),
        "--".to_string(),
        "--verbose".to_string(),
    ];
    let parsed = derive_args(&config_tokens(&cfg, &argv), &argv, true);
    assert!(parsed.verbose);
    assert!(parsed.runner_args.iter().any(|t| t == "--verbose"));
    assert!(!parsed.runner_args.iter().any(|t| t == "--"));
}
