use std::time::Duration;

use headlamp_parity_support::runner_parity::{
    ParityExecEnv, RunnerId, run_env_matrix_no_hang, runner_parity_headlamp_bin,
    shared_real_runner_repo,
};

#[test]
fn parity_env_matrix_all_four_no_hang_pass_and_fail() {
    let repo = shared_real_runner_repo();
    let headlamp_bin = runner_parity_headlamp_bin();

    let environments = [ParityExecEnv::NonTty, ParityExecEnv::Tty { columns: 120 }];
    let timeout = Duration::from_secs(10);

    // Passing cases (fast).
    let runners_pass: Vec<(RunnerId, &[&str])> = vec![
        (RunnerId::Jest, &["tests/sum_pass_test.js"]),
        (RunnerId::Pytest, &["tests/sum_pass_test.py"]),
        (RunnerId::CargoTest, &["tests/sum_pass_test.rs"]),
        (RunnerId::CargoNextest, &["tests/sum_pass_test.rs"]),
    ];
    let results_pass = run_env_matrix_no_hang(
        &repo,
        &headlamp_bin,
        "env_matrix_pass",
        &runners_pass,
        &environments,
        timeout,
        Some("env_matrix_pass"),
    );
    results_pass.iter().for_each(|r| {
        assert_ne!(
            r.exit, 124,
            "unexpected timeout: env={:?} runner={:?}",
            r.env, r.runner
        );
    });

    // Failing cases: ensure we don't hang on failure paths.
    let runners_fail: Vec<(RunnerId, &[&str])> = vec![
        (RunnerId::Jest, &["tests/sum_fail_test.js"]),
        (RunnerId::Pytest, &["tests/sum_fail_test.py"]),
        (RunnerId::CargoTest, &["tests/sum_fail_test.rs"]),
        (RunnerId::CargoNextest, &["tests/sum_fail_test.rs"]),
    ];
    let results_fail = run_env_matrix_no_hang(
        &repo,
        &headlamp_bin,
        "env_matrix_fail",
        &runners_fail,
        &environments,
        timeout,
        Some("env_matrix_fail"),
    );
    results_fail.iter().for_each(|r| {
        assert_ne!(
            r.exit, 124,
            "unexpected timeout: env={:?} runner={:?}",
            r.env, r.runner
        );
        assert_ne!(
            r.exit, 0,
            "expected failure: env={:?} runner={:?}",
            r.env, r.runner
        );
    });
}
