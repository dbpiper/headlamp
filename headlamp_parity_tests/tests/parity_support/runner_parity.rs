#![allow(dead_code, unused_imports)]

use std::path::Path;

pub use headlamp_parity_support::runner_parity::{
    RealRunnerWorktreeLease, RunnerId, assert_runner_parity_tty_all_four,
    assert_runner_parity_tty_all_four_env, lease_real_runner_worktree, real_runner_worktree,
    runner_parity_headlamp_bin, shared_real_runner_repo, shared_threshold_real_runner_repo,
    write_real_runner_repo,
};

pub fn assert_runner_parity_tty_snapshot_all_four_env(
    repo: &Path,
    headlamp_bin: &Path,
    case: &str,
    runners: &[(RunnerId, &[&str])],
    extra_env: &[(&str, String)],
) {
    let canonical =
        headlamp_parity_support::runner_parity::runner_parity_tty_all_four_canonical_env(
            repo,
            headlamp_bin,
            case,
            runners,
            extra_env,
        );
    let snapshot_name = snapshot_name_from_case(case);
    let mut settings = insta::Settings::clone_current();
    // Keep existing snapshot location stable (it previously lived under this `tests/parity_support/` module).
    settings.set_snapshot_path(Path::new("tests/snapshots/runner_parity"));
    settings.bind(|| {
        insta::assert_snapshot!(snapshot_name, canonical);
    });
}

fn snapshot_name_from_case(case: &str) -> String {
    case.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' => c.to_ascii_lowercase(),
            _ => '_',
        })
        .collect::<String>()
}
