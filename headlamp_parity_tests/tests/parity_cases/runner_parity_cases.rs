use crate::parity_support::runner_parity::{
    RunnerId, assert_runner_parity_tty_snapshot_all_four_env, lease_real_runner_worktree,
    shared_threshold_real_runner_repo,
};
use std::path::Path;
use std::process::Command;

mod changed_and_failure;

fn run_all_four_snapshot(repo: &std::path::Path, case: &str, runner_args: &[(&str, &[&str])]) {
    let headlamp_bin = crate::parity_support::runner_parity::runner_parity_headlamp_bin();
    let mapped = runner_args
        .iter()
        .map(|(runner, args)| {
            let id = match *runner {
                "jest" => RunnerId::Jest,
                "cargo-test" => RunnerId::CargoTest,
                "cargo-nextest" => RunnerId::CargoNextest,
                "pytest" => RunnerId::Pytest,
                other => panic!("unknown runner {other}"),
            };
            (id, *args)
        })
        .collect::<Vec<_>>();
    assert_runner_parity_tty_snapshot_all_four_env(repo, &headlamp_bin, case, &mapped, &[]);
}

fn lease_repo_for_case(
    name: &str,
) -> crate::parity_support::runner_parity::RealRunnerWorktreeLease {
    lease_real_runner_worktree(name)
}

fn append_file(path: &Path, suffix: &str) {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let next = format!("{existing}{suffix}");
    std::fs::write(path, next).unwrap();
}

fn run_git(repo: &Path, args: &[&str]) {
    let status = Command::new("git").current_dir(repo).args(args).status();
    assert!(status.is_ok_and(|s| s.success()));
}

fn strip_ansi_like(text: &str) -> String {
    let re = regex::Regex::new(r"\x1b\[[^m]*m").unwrap();
    re.replace_all(text, "").to_string()
}

fn extract_first_message_block_lines(normalized_output: &str) -> Vec<String> {
    let lines = normalized_output
        .lines()
        .map(strip_ansi_like)
        .collect::<Vec<_>>();
    let start = lines
        .iter()
        .position(|ln| ln.trim() == "Message:" || ln.trim_end().ends_with("Message:"))
        .unwrap_or(usize::MAX);
    if start == usize::MAX {
        return vec![];
    }
    lines
        .iter()
        .skip(start + 1)
        .map(|ln| ln.trim().to_string())
        .take_while(|ln| {
            let t = ln.trim();
            !(t.is_empty()
                || t.starts_with("Expected")
                || t.starts_with("Received")
                || t.starts_with("Stack")
                || t.starts_with("Assertion")
                || t.starts_with("Console"))
        })
        .filter(|ln| !ln.trim().is_empty())
        .collect::<Vec<_>>()
}

#[test]
fn parity_runner_name_pattern_only_all_four() {
    let lease = lease_repo_for_case("name-pattern-only");
    let repo = lease.path();
    let jest_args = ["--", "-t", "test_sum_passes"];
    let cargo_args = ["--", "test_sum_passes"];
    let pytest_args = ["--", "-k", "test_sum_passes"];
    run_all_four_snapshot(
        repo,
        "name pattern only all four",
        &[
            ("jest", &jest_args),
            ("cargo-test", &cargo_args),
            ("cargo-nextest", &cargo_args),
            ("pytest", &pytest_args),
        ],
    );
}

#[test]
fn parity_runner_selection_prod_file_all_four() {
    let lease = lease_repo_for_case("selection-prod-file");
    let repo = lease.path();
    let jest_args = ["src/sum.js"];
    let cargo_args = ["src/sum.rs"];
    let pytest_args = ["src/sum.py"];
    run_all_four_snapshot(
        repo,
        "selection prod file all four",
        &[
            ("jest", &jest_args),
            ("cargo-test", &cargo_args),
            ("cargo-nextest", &cargo_args),
            ("pytest", &pytest_args),
        ],
    );
}

#[test]
fn parity_runner_selection_test_file_all_four() {
    let lease = lease_repo_for_case("selection-test-file");
    let repo = lease.path();
    let jest_args = ["tests/a_test.js"];
    let cargo_args = ["tests/a_test.rs"];
    let pytest_args = ["tests/a_test.py"];
    run_all_four_snapshot(
        repo,
        "selection test file all four",
        &[
            ("jest", &jest_args),
            ("cargo-test", &cargo_args),
            ("cargo-nextest", &cargo_args),
            ("pytest", &pytest_args),
        ],
    );
}

#[test]
fn parity_runner_selection_indirect_import_all_four() {
    let lease = lease_repo_for_case("selection-indirect-import");
    let repo = lease.path();
    let jest_args = ["src/very_unique_name_for_parity_123.js"];
    let cargo_args = ["src/very_unique_name_for_parity_123.rs"];
    let pytest_args = ["src/very_unique_name_for_parity_123.py"];
    run_all_four_snapshot(
        repo,
        "selection indirect import all four",
        &[
            ("jest", &jest_args),
            ("cargo-test", &cargo_args),
            ("cargo-nextest", &cargo_args),
            ("pytest", &pytest_args),
        ],
    );
}

#[test]
fn parity_runner_coverage_ui_both_all_four() {
    let lease = lease_repo_for_case("coverage-ui-both");
    let repo = lease.path();
    let args = ["--coverage", "--coverage-ui=both"];
    run_all_four_snapshot(
        repo,
        "coverage-ui=both all four",
        &[
            ("jest", &args),
            ("cargo-test", &args),
            ("cargo-nextest", &args),
            ("pytest", &args),
        ],
    );
}

#[test]
fn parity_runner_coverage_thresholds_not_met_all_four() {
    let repo = shared_threshold_real_runner_repo();
    let args = ["--coverage", "--coverage-ui=both"];
    run_all_four_snapshot(
        &repo,
        "coverage thresholds not met all four",
        &[
            ("jest", &args),
            ("cargo-test", &args),
            ("cargo-nextest", &args),
            ("pytest", &args),
        ],
    );
}

#[test]
fn parity_runner_coverage_ui_jest_suppresses_coverage_all_four() {
    let lease = lease_repo_for_case("coverage-ui-jest");
    let repo = lease.path();
    let args = ["--coverage", "--coverage-ui=jest"];
    run_all_four_snapshot(
        repo,
        "coverage-ui=jest suppresses coverage all four",
        &[
            ("jest", &args),
            ("cargo-test", &args),
            ("cargo-nextest", &args),
            ("pytest", &args),
        ],
    );
}

#[test]
fn parity_runner_bootstrap_command_all_four() {
    let lease = lease_repo_for_case("bootstrapCommand");
    let repo = lease.path();
    let file_name = "bootstrap.txt";
    let bootstrap_arg = format!("--bootstrapCommand=echo bootstrap > {file_name}");
    let jest_args = [bootstrap_arg.as_str(), "tests/sum_pass_test.js"];
    let cargo_args = [bootstrap_arg.as_str(), "tests/sum_pass_test.rs"];
    let pytest_args = [bootstrap_arg.as_str(), "tests/sum_pass_test.py"];
    run_all_four_snapshot(
        repo,
        "bootstrapCommand all four",
        &[
            ("jest", &jest_args),
            ("cargo-test", &cargo_args),
            ("cargo-nextest", &cargo_args),
            ("pytest", &pytest_args),
        ],
    );
    assert!(repo.join(file_name).exists());
}

#[test]
fn parity_runner_changed_all_selects_multiple_tests_all_four() {
    let lease = lease_repo_for_case("changed-all");
    let repo = lease.path();
    // Create real changed files in all three languages so each runner selects the same
    // pass+fail test pair through transitive dependency resolution.
    append_file(&repo.join("src/sum.js"), "\n");
    append_file(&repo.join("src/sum.rs"), "\n");
    append_file(&repo.join("src/sum.py"), "\n");
    let args = ["--changed=all"];
    run_all_four_snapshot(
        repo,
        "changed=all selects tests all four",
        &[
            ("jest", &args),
            ("cargo-test", &args),
            ("cargo-nextest", &args),
            ("pytest", &args),
        ],
    );
}

#[test]
fn parity_runner_changed_depth_respected_all_four() {
    let lease = lease_repo_for_case("changed-depth");
    let repo = lease.path();
    append_file(&repo.join("src/index.js"), "\n");
    append_file(&repo.join("src/index.rs"), "\n");
    append_file(&repo.join("src/index.py"), "\n");
    let depth0_args = ["--changed=all", "--changed.depth=0"];
    let depth1_args = ["--changed=all", "--changed.depth=1"];
    let depth5_args = ["--changed=all", "--changed.depth=5"];
    run_all_four_snapshot(
        repo,
        "changed.depth=0 all four",
        &[
            ("jest", &depth0_args),
            ("cargo-test", &depth0_args),
            ("cargo-nextest", &depth0_args),
            ("pytest", &depth0_args),
        ],
    );
    run_all_four_snapshot(
        repo,
        "changed.depth=1 all four",
        &[
            ("jest", &depth1_args),
            ("cargo-test", &depth1_args),
            ("cargo-nextest", &depth1_args),
            ("pytest", &depth1_args),
        ],
    );
    run_all_four_snapshot(
        repo,
        "changed.depth=5 all four",
        &[
            ("jest", &depth5_args),
            ("cargo-test", &depth5_args),
            ("cargo-nextest", &depth5_args),
            ("pytest", &depth5_args),
        ],
    );
}

#[test]
fn parity_runner_basic_pass_all_four() {
    let lease = lease_repo_for_case("basic-pass");
    let repo = lease.path();
    let jest_args = ["tests/sum_pass_test.js"];
    let cargo_args = ["tests/sum_pass_test.rs"];
    let pytest_args = ["tests/sum_pass_test.py"];
    run_all_four_snapshot(
        repo,
        "basic pass all four",
        &[
            ("jest", &jest_args),
            ("cargo-test", &cargo_args),
            ("cargo-nextest", &cargo_args),
            ("pytest", &pytest_args),
        ],
    );
}

#[test]
fn parity_runner_basic_fail_all_four() {
    let lease = lease_repo_for_case("basic-fail");
    let repo = lease.path();
    let jest_args = ["tests/sum_fail_test.js"];
    let cargo_args = ["tests/sum_fail_test.rs"];
    let pytest_args = ["tests/sum_fail_test.py"];
    run_all_four_snapshot(
        repo,
        "basic fail all four",
        &[
            ("jest", &jest_args),
            ("cargo-test", &cargo_args),
            ("cargo-nextest", &cargo_args),
            ("pytest", &pytest_args),
        ],
    );
}

#[test]
fn parity_runner_failure_message_identical_all_four() {
    let lease = lease_repo_for_case("failure-message-identical");
    let repo = lease.path();

    let shared_message = "assertion `left == right` failed\n  left: 1\n right: 2\n";
    std::fs::write(
        repo.join("tests/sum_fail_test.js"),
        format!(
            "test('test_sum_fails', () => {{ throw new Error({:?}); }});\n",
            shared_message
        ),
    )
    .unwrap();
    std::fs::write(
        repo.join("tests/sum_fail_test.rs"),
        format!(
            "#[test]\nfn test_sum_fails() {{\n    panic!({:?});\n}}\n",
            shared_message
        ),
    )
    .unwrap();
    std::fs::write(
        repo.join("tests/sum_fail_test.py"),
        format!(
            "def test_sum_fails() -> None:\n    raise AssertionError({:?})\n",
            shared_message
        ),
    )
    .unwrap();

    let headlamp_bin = crate::parity_support::runner_parity::runner_parity_headlamp_bin();
    let runners = [
        ("jest", &["tests/sum_fail_test.js"][..]),
        ("cargo-test", &["tests/sum_fail_test.rs"][..]),
        ("cargo-nextest", &["tests/sum_fail_test.rs"][..]),
        ("pytest", &["tests/sum_fail_test.py"][..]),
    ];

    let message_blocks = runners
        .iter()
        .map(|(runner, args)| {
            let (_spec, _code, raw) = crate::parity_support::parity_run::run_headlamp_with_args_tty(
                repo,
                &headlamp_bin,
                120,
                runner,
                args,
            );
            let normalized = crate::parity_support::normalize::normalize_tty_ui(raw, repo);
            (
                runner.to_string(),
                extract_first_message_block_lines(&normalized),
            )
        })
        .collect::<Vec<_>>();

    let required = ["assertion `left == right` failed", "left:", "right:"];
    message_blocks.iter().for_each(|(runner, msg)| {
        let joined = msg.join("\n");
        required.iter().for_each(|needle| {
            assert!(
                joined.contains(needle),
                "runner {runner} message block missing '{needle}'.\n\nmessage:\n{joined}"
            );
        });
    });
}

// moved changed/* and failure output tests into `runner_parity_cases::changed_and_failure`
