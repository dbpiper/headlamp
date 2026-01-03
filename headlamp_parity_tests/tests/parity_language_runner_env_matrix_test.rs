use std::time::Duration;

use headlamp_parity_support::runner_parity::{
    ParityExecEnv, RunnerId, run_env_matrix_no_hang, runner_parity_headlamp_bin,
    shared_real_runner_repo,
};

#[derive(Debug, Clone, Copy)]
enum ParityLanguage {
    Js,
    Python,
    Rust,
}

#[test]
fn parity_env_matrix_tty_non_tty_language_runner_no_hang() {
    let repo = shared_real_runner_repo();
    let headlamp_bin = runner_parity_headlamp_bin();
    let environments = [ParityExecEnv::NonTty, ParityExecEnv::Tty { columns: 120 }];
    let timeout = Duration::from_secs(15);

    // This explicitly encodes the (language, runner, args) matrix we care about.
    // We do NOT run invalid combinations.
    let matrix: &[(ParityLanguage, RunnerId, &[&str])] = &[
        (
            ParityLanguage::Js,
            RunnerId::Jest,
            &["tests/sum_pass_test.js"],
        ),
        (
            ParityLanguage::Python,
            RunnerId::Pytest,
            &["tests/sum_pass_test.py"],
        ),
        (
            ParityLanguage::Rust,
            RunnerId::Headlamp,
            &["tests/sum_pass_test.rs"],
        ),
        (
            ParityLanguage::Rust,
            RunnerId::CargoTest,
            &["tests/sum_pass_test.rs"],
        ),
        (
            ParityLanguage::Rust,
            RunnerId::CargoNextest,
            &["tests/sum_pass_test.rs"],
        ),
    ];

    let runners = matrix
        .iter()
        .map(|(_lang, runner, args)| (*runner, *args))
        .collect::<Vec<_>>();
    let results = run_env_matrix_no_hang(
        &repo,
        &headlamp_bin,
        "env_matrix_language_runner_pass",
        &runners,
        &environments,
        timeout,
        Some("env_matrix_language_runner_pass"),
    );
    results.iter().for_each(|r| {
        assert_ne!(
            r.exit, 124,
            "unexpected timeout: env={:?} runner={:?} spec={:?}",
            r.env, r.runner, r.spec
        );
        assert_eq!(
            r.exit, 0,
            "expected pass: env={:?} runner={:?} output:\n{}",
            r.env, r.runner, r.output
        );
    });
}
