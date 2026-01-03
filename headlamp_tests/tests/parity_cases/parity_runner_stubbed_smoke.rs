use crate::parity_support::mk_temp_dir;
use crate::parity_support::runner_parity::{
    RunnerId, RunnerParityScenario, assert_runner_parity_tty_all_runners, write_stubbed_runner_repo,
};

#[test]
fn parity_runner_stubbed_smoke_pass_all_runners() {
    let repo = mk_temp_dir("runner-parity-stubbed-smoke-pass");
    let scenario = RunnerParityScenario::basic_pass();
    write_stubbed_runner_repo(&repo, &scenario);

    let headlamp_bin = crate::parity_support::runner_parity::runner_parity_headlamp_bin();
    assert_runner_parity_tty_all_runners(
        &repo,
        &headlamp_bin,
        "stubbed smoke pass all runners",
        &[
            (RunnerId::Jest, &[]),
            (RunnerId::CargoTest, &[]),
            (RunnerId::CargoNextest, &[]),
            (RunnerId::Pytest, &[]),
        ],
    );
}
