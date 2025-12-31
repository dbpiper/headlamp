use crate::args::derive_args;
use crate::args::split_headlamp_tokens;

#[test]
fn derive_args_does_not_include_double_dash_in_runner_args() {
    let cfg_tokens: Vec<String> = vec![];
    let argv = vec![
        "--".to_string(),
        "-t".to_string(),
        "test_sum_passes".to_string(),
    ];

    let parsed = derive_args(&cfg_tokens, &argv, true);
    assert_eq!(
        parsed.runner_args,
        vec!["-t".to_string(), "test_sum_passes".to_string()]
    );
    assert!(parsed.selection_paths.is_empty());
    assert!(parsed.selection_specified);
}

#[test]
fn split_headlamp_tokens_treats_canonical_coverage_abort_on_failure_as_boolean_value_taker() {
    let tokens = vec![
        "--coverage-abort-on-failure".to_string(),
        "false".to_string(),
        "-t".to_string(),
        "UserCard".to_string(),
    ];

    let (headlamp_tokens, passthrough_tokens) = split_headlamp_tokens(&tokens);
    assert_eq!(
        headlamp_tokens,
        vec![
            "--coverage-abort-on-failure".to_string(),
            "false".to_string()
        ]
    );
    assert_eq!(
        passthrough_tokens,
        vec!["-t".to_string(), "UserCard".to_string()]
    );
}
