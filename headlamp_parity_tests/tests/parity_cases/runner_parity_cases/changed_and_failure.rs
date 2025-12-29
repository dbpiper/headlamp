use super::*;

fn assert_has_standard_failure_sections(runner: &str, normalized: &str) {
    let plain = strip_ansi_like(normalized);
    assert!(
        plain.contains("Expected") && plain.contains("Received"),
        "runner {runner} output missing Expected/Received.\n\n{plain}"
    );
    assert!(
        plain.contains("Message:") || plain.contains("Assertion:"),
        "runner {runner} output missing Message/Assertion section.\n\n{plain}"
    );
    let has_stack = plain.contains("Stack:")
        || plain.contains("stack backtrace:")
        || plain.contains("Traceback (most recent call last):")
        || plain.contains("File \"")
        || plain.contains("AssertionError");
    assert!(
        has_stack,
        "runner {runner} output missing Stack section.\n\n{plain}"
    );
    assert!(
        plain.contains("|") && plain.contains(">") && plain.contains("^"),
        "runner {runner} output missing code frame.\n\n{plain}"
    );
}

#[test]
fn parity_runner_failure_output_has_standard_sections_all_four() {
    let lease = lease_repo_for_case("failure-output-sections");
    let repo = lease.path();

    std::fs::write(
        repo.join("tests/sum_fail_test.js"),
        "test('test_sum_fails', () => { expect(1).toBe(2); });\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("tests/sum_fail_test.rs"),
        "#[test]\nfn test_sum_fails() {\n    assert_eq!(1, 2);\n}\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("tests/sum_fail_test.py"),
        "def test_sum_fails() -> None:\n    assert 1 == 2\n",
    )
    .unwrap();

    let headlamp_bin = crate::parity_support::runner_parity::runner_parity_headlamp_bin();
    let runners = [
        ("jest", &["tests/sum_fail_test.js"][..]),
        ("cargo-test", &["tests/sum_fail_test.rs"][..]),
        ("cargo-nextest", &["tests/sum_fail_test.rs"][..]),
        ("pytest", &["tests/sum_fail_test.py"][..]),
    ];

    runners.iter().for_each(|(runner, args)| {
        let (_spec, _code, raw) = crate::parity_support::parity_run::run_headlamp_with_args_tty(
            repo,
            &headlamp_bin,
            120,
            runner,
            args,
        );
        let normalized = crate::parity_support::normalize::normalize_tty_ui(raw, repo);
        assert_has_standard_failure_sections(runner, &normalized);
    });
}

#[test]
fn parity_runner_only_failures_all_four() {
    let lease = lease_repo_for_case("onlyFailures");
    let repo = lease.path();
    let args = ["--onlyFailures"];
    run_all_four_snapshot(
        repo,
        "onlyFailures all four",
        &[
            ("jest", &args),
            ("cargo-test", &args),
            ("cargo-nextest", &args),
            ("pytest", &args),
        ],
    );
}

#[test]
fn parity_runner_show_logs_all_four() {
    let lease = lease_repo_for_case("showLogs");
    let repo = lease.path();
    let args = ["--showLogs"];
    run_all_four_snapshot(
        repo,
        "showLogs all four",
        &[
            ("jest", &args),
            ("cargo-test", &args),
            ("cargo-nextest", &args),
            ("pytest", &args),
        ],
    );
}

#[test]
fn parity_runner_changed_staged_all_four() {
    let lease = lease_repo_for_case("changed-staged");
    let repo = lease.path();
    append_file(&repo.join("src/sum.js"), "\n");
    append_file(&repo.join("src/sum.rs"), "\n");
    append_file(&repo.join("src/sum.py"), "\n");
    run_git(repo, &["add", "-A"]);
    let args = ["--changed=staged"];
    run_all_four_snapshot(
        repo,
        "changed=staged all four",
        &[
            ("jest", &args),
            ("cargo-test", &args),
            ("cargo-nextest", &args),
            ("pytest", &args),
        ],
    );
}

#[test]
fn parity_runner_changed_unstaged_all_four() {
    let lease = lease_repo_for_case("changed-unstaged");
    let repo = lease.path();
    append_file(&repo.join("src/sum.js"), "\n");
    append_file(&repo.join("src/sum.rs"), "\n");
    append_file(&repo.join("src/sum.py"), "\n");
    let args = ["--changed=unstaged"];
    run_all_four_snapshot(
        repo,
        "changed=unstaged all four",
        &[
            ("jest", &args),
            ("cargo-test", &args),
            ("cargo-nextest", &args),
            ("pytest", &args),
        ],
    );
}

#[test]
fn parity_runner_changed_branch_all_four() {
    let lease = lease_repo_for_case("changed-branch");
    let repo = lease.path();
    append_file(&repo.join("src/sum.js"), "\n");
    append_file(&repo.join("src/sum.rs"), "\n");
    append_file(&repo.join("src/sum.py"), "\n");
    run_git(repo, &["add", "-A"]);
    run_git(repo, &["commit", "-q", "-m", "branch change"]);
    let args = ["--changed=branch"];
    run_all_four_snapshot(
        repo,
        "changed=branch all four",
        &[
            ("jest", &args),
            ("cargo-test", &args),
            ("cargo-nextest", &args),
            ("pytest", &args),
        ],
    );
}

#[test]
fn parity_runner_changed_last_commit_all_four() {
    let lease = lease_repo_for_case("changed-last-commit");
    let repo = lease.path();
    append_file(&repo.join("src/sum.js"), "\n");
    append_file(&repo.join("src/sum.rs"), "\n");
    append_file(&repo.join("src/sum.py"), "\n");
    run_git(repo, &["add", "-A"]);
    run_git(repo, &["commit", "-q", "-m", "last commit change"]);
    let args = ["--changed=lastCommit"];
    run_all_four_snapshot(
        repo,
        "changed=lastCommit all four",
        &[
            ("jest", &args),
            ("cargo-test", &args),
            ("cargo-nextest", &args),
            ("pytest", &args),
        ],
    );
}
