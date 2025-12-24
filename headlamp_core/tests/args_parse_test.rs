use headlamp_core::args::{CoverageDetail, config_tokens, derive_args};
use headlamp_core::config::{CoverageUi, HeadlampConfig};

#[test]
fn derives_basic_flags_and_selection() {
    let cfg = HeadlampConfig::default();
    let argv = vec![
        "--coverage".to_string(),
        "--onlyFailures".to_string(),
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
        "--showLogs".to_string(),
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
fn derive_args_changed_optional_value_does_not_consume_next_flag() {
    let cfg = HeadlampConfig::default();
    let argv = vec![
        "--changed".to_string(),
        "--showLogs".to_string(),
        "-t".to_string(),
        "UserCard".to_string(),
    ];
    let cfg_tokens = config_tokens(&cfg, &argv);
    let parsed = derive_args(&cfg_tokens, &argv, true);
    assert_eq!(
        parsed.changed,
        Some(headlamp_core::config::ChangedMode::All)
    );
    assert!(parsed.show_logs);
    assert!(parsed.runner_args.iter().any(|t| t == "-t"));
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
        headlamp_core::config::CoverageMode::Compact
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
        "--coverage.abortOnFailure".to_string(),
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
