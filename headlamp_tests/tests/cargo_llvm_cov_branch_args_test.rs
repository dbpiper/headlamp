use headlamp::cargo::build_cargo_llvm_cov_command_args;

#[test]
fn build_cargo_llvm_cov_command_args_includes_nightly_and_branch_when_enabled() {
    let args = build_cargo_llvm_cov_command_args(
        true,
        true,
        false,
        &["test".to_string(), "--no-report".to_string()],
    );
    assert_eq!(
        args,
        vec![
            "+nightly".to_string(),
            "llvm-cov".to_string(),
            "--branch".to_string(),
            "test".to_string(),
            "--no-report".to_string(),
        ]
    );
}

#[test]
fn build_cargo_llvm_cov_command_args_omits_branch_without_nightly() {
    let args = build_cargo_llvm_cov_command_args(
        true,
        false,
        false,
        &["test".to_string(), "--no-report".to_string()],
    );
    assert_eq!(
        args,
        vec![
            "llvm-cov".to_string(),
            "test".to_string(),
            "--no-report".to_string(),
        ]
    );
}

#[test]
fn build_cargo_llvm_cov_command_args_omits_branch_when_not_enabled() {
    let args = build_cargo_llvm_cov_command_args(
        false,
        true,
        false,
        &["test".to_string(), "--no-report".to_string()],
    );
    assert_eq!(
        args,
        vec![
            "llvm-cov".to_string(),
            "test".to_string(),
            "--no-report".to_string(),
        ]
    );
}
