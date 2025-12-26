mod parity_support;

use std::path::Path;

use parity_support::{
    assert_parity_with_args, git_commit_all, git_init, mk_repo, parity_binaries, write_file,
    write_jest_config,
};

fn write_two_prod_two_tests(repo: &Path) {
    write_file(&repo.join("src/a.js"), "exports.a = () => 'a';\n");
    write_file(&repo.join("src/b.js"), "exports.b = () => 'b';\n");
    write_file(
        &repo.join("tests/a.test.js"),
        "const { a } = require('../src/a');\n\ntest('a', () => { expect(a()).toBe('a'); });\n",
    );
    write_file(
        &repo.join("tests/b.test.js"),
        "const { b } = require('../src/b');\n\ntest('b', () => { expect(b()).toBe('b'); });\n",
    );
    write_jest_config(repo, "**/tests/**/*.test.js");
}

#[test]
fn parity_jest_changed_all_selects_tests_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-changed-all", &binaries.node_modules);
    write_two_prod_two_tests(&repo);
    git_init(&repo);
    git_commit_all(&repo, "baseline");

    write_file(
        &repo.join("src/a.js"),
        "exports.a = () => 'a'; // changed\n",
    );

    assert_parity_with_args(&repo, &binaries, &["--changed=all"], &["--changed=all"]);
}

#[test]
fn parity_jest_changed_last_commit_selects_tests_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-changed-last-commit", &binaries.node_modules);
    write_file(&repo.join("src/b.js"), "exports.b = () => 'b';\n");
    write_file(
        &repo.join("tests/b.test.js"),
        "const { b } = require('../src/b');\n\ntest('b', () => { expect(b()).toBe('b'); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");
    git_init(&repo);
    git_commit_all(&repo, "baseline");

    write_file(
        &repo.join("src/b.js"),
        "exports.b = () => 'b'; // changed\n",
    );
    git_commit_all(&repo, "change b");

    assert_parity_with_args(
        &repo,
        &binaries,
        &["--changed=lastCommit"],
        &["--changed=lastCommit"],
    );
}

fn write_transitive_refine_fixture(repo: &Path) {
    write_file(&repo.join("src/seed_target.js"), "exports.x = 1;\n");
    write_file(
        &repo.join("src/helper2.js"),
        "const { x } = require('./seed_target');\nexports.y = () => x;\n",
    );
    write_file(
        &repo.join("src/helper1.js"),
        "const { y } = require('./helper2');\nexports.z = () => y();\n",
    );
    write_file(
        &repo.join("tests/transitive.test.js"),
        "const { z } = require('../src/helper1');\n\ntest('transitive', () => { expect(z()).toBe(1); });\n",
    );
    write_jest_config(repo, "**/tests/**/*.test.js");
}

#[test]
fn parity_jest_changed_depth_transitive_refine_depth_1_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-changed-depth-1", &binaries.node_modules);
    write_transitive_refine_fixture(&repo);
    git_init(&repo);
    git_commit_all(&repo, "baseline");

    write_file(
        &repo.join("src/seed_target.js"),
        "exports.x = 1; // changed\n",
    );

    assert_parity_with_args(
        &repo,
        &binaries,
        &["--changed=all", "--changed.depth=1"],
        &["--changed=all", "--changed.depth=1"],
    );
}

#[test]
fn parity_jest_changed_depth_transitive_refine_depth_5_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-changed-depth-5", &binaries.node_modules);
    write_transitive_refine_fixture(&repo);
    git_init(&repo);
    git_commit_all(&repo, "baseline");

    write_file(
        &repo.join("src/seed_target.js"),
        "exports.x = 1; // changed\n",
    );

    assert_parity_with_args(
        &repo,
        &binaries,
        &["--changed=all", "--changed.depth=5"],
        &["--changed=all", "--changed.depth=5"],
    );
}
