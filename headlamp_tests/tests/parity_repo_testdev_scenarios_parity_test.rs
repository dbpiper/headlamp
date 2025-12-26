mod parity_support;
use parity_support::parity_binaries;

#[path = "parity_repo_e2e_support/common.rs"]
mod parity_repo_e2e_support_common;
use parity_repo_e2e_support_common::{
    assert_parity, create_test_file, discover_repo, run_npm_test_dev_parity_normalized, setup_repo,
};

#[path = "parity_repo_e2e_support/git.rs"]
mod parity_repo_e2e_support_git;
use parity_repo_e2e_support_git::{git_add, git_reset};

#[test]
fn parity_repo_test_dev_works_for_untracked_changed_mode() {
    let Some(binaries) = parity_binaries() else {
        return;
    };
    let Some(repo) = discover_repo() else { return };
    let Some(e2e) = setup_repo(repo, &binaries) else {
        return;
    };

    let _t = create_test_file(
        &e2e.repo,
        "headlamp_parity_untracked.test.js",
        "test('parity untracked', () => { expect(2 + 2).toBe(4); });\n",
    );

    let (code_ts, out_ts, code_rs, out_rs) = run_npm_test_dev_parity_normalized(&e2e, &[]);
    assert_parity(&e2e, "untracked", code_ts, &out_ts, code_rs, &out_rs);
}

#[test]
fn parity_repo_test_dev_works_for_staged_changed_mode() {
    let Some(binaries) = parity_binaries() else {
        return;
    };
    let Some(repo) = discover_repo() else { return };
    let Some(e2e) = setup_repo(repo, &binaries) else {
        return;
    };

    let file_name = "headlamp_parity_staged.test.js";
    let Some(t) = create_test_file(
        &e2e.repo,
        file_name,
        "test('parity staged', () => { expect(3 + 3).toBe(6); });\n",
    ) else {
        return;
    };
    let rel = t.path.strip_prefix(&e2e.repo).ok();
    let Some(rel) = rel else { return };
    let _ = git_add(&e2e.repo, rel);

    let (code_ts, out_ts, code_rs, out_rs) = run_npm_test_dev_parity_normalized(&e2e, &[]);
    let _ = git_reset(&e2e.repo, rel);
    assert_parity(&e2e, "staged", code_ts, &out_ts, code_rs, &out_rs);
}

#[test]
fn parity_repo_test_dev_selection_path_runs_single_test() {
    let Some(binaries) = parity_binaries() else {
        return;
    };
    let Some(repo) = discover_repo() else { return };
    let Some(e2e) = setup_repo(repo, &binaries) else {
        return;
    };

    let file_name = "headlamp_parity_select.test.js";
    let _t = create_test_file(
        &e2e.repo,
        file_name,
        "test('parity select path', () => { expect(true).toBe(true); });\n",
    );

    let selection = format!("tests/{file_name}");
    let (code_ts, out_ts, code_rs, out_rs) =
        run_npm_test_dev_parity_normalized(&e2e, &[selection.as_str()]);
    assert_parity(&e2e, "selection_path", code_ts, &out_ts, code_rs, &out_rs);
}

#[test]
fn parity_repo_test_dev_failing_test_with_show_logs() {
    let Some(binaries) = parity_binaries() else {
        return;
    };
    let Some(repo) = discover_repo() else { return };
    let Some(e2e) = setup_repo(repo, &binaries) else {
        return;
    };

    let _t = create_test_file(
        &e2e.repo,
        "headlamp_parity_failing.test.js",
        "test('parity fail', () => { console.log('hello'); expect(1).toBe(2); });\n",
    );

    let (code_ts, out_ts, code_rs, out_rs) =
        run_npm_test_dev_parity_normalized(&e2e, &["--showLogs"]);
    assert_parity(
        &e2e,
        "failing_show_logs",
        code_ts,
        &out_ts,
        code_rs,
        &out_rs,
    );
}

#[test]
fn parity_repo_test_dev_sequential_flag() {
    let Some(binaries) = parity_binaries() else {
        return;
    };
    let Some(repo) = discover_repo() else { return };
    let Some(e2e) = setup_repo(repo, &binaries) else {
        return;
    };

    let _t = create_test_file(
        &e2e.repo,
        "headlamp_parity_sequential.test.js",
        "test('parity sequential', () => { expect('a' + 'b').toBe('ab'); });\n",
    );

    let (code_ts, out_ts, code_rs, out_rs) =
        run_npm_test_dev_parity_normalized(&e2e, &["--sequential"]);
    assert_parity(&e2e, "sequential", code_ts, &out_ts, code_rs, &out_rs);
}
