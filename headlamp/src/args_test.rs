use crate::args::derive_args;

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
