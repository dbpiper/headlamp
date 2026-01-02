use crate::args::derive_args;

fn with_env_var_removed<T>(key: &str, f: impl FnOnce() -> T) -> T {
    let previous_value = std::env::var_os(key);
    unsafe { std::env::remove_var(key) };
    let result = f();
    if let Some(value) = previous_value {
        unsafe { std::env::set_var(key, value) };
    }
    result
}

#[test]
fn nextest_args_non_tty_non_ci_do_not_enable_cargo_quiet() {
    with_env_var_removed("CI", || {
        let parsed = derive_args(&[], &[], false);
        let cmd_args = super::runner_args::build_nextest_run_args(None, &parsed, &[]);
        assert!(!cmd_args.iter().any(|t| t == "--cargo-quiet"));
    });
}
