mod parity_support;

use parity_support::{
    assert_parity_with_args, mk_repo, parity_binaries, write_file, write_jest_config,
};

#[test]
fn parity_jest_select_by_production_file_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-select-prod", &binaries.node_modules);
    write_file(&repo.join("src/a.js"), "exports.a = () => 1;\n");
    write_file(
        &repo.join("tests/a.test.js"),
        "const { a } = require('../src/a');\n\ntest('a', () => { expect(a()).toBe(1); });\n",
    );
    write_file(
        &repo.join("tests/b.test.js"),
        "test('b', () => { expect(1).toBe(1); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    assert_parity_with_args(&repo, &binaries, &["src/a.js"], &["src/a.js"]);
}

#[test]
fn parity_jest_select_by_test_file_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-select-test-file", &binaries.node_modules);
    write_file(&repo.join("src/sum.js"), "exports.sum = (a,b) => a + b;\n");
    write_file(
        &repo.join("tests/a.test.js"),
        "const { sum } = require('../src/sum');\n\ntest('a', () => { expect(sum(1,2)).toBe(3); });\n",
    );
    write_file(
        &repo.join("tests/b.test.js"),
        "test('b', () => { expect(1).toBe(1); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    assert_parity_with_args(&repo, &binaries, &["tests/a.test.js"], &["tests/a.test.js"]);
}

#[test]
fn parity_jest_select_indirect_import_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-select-indirect-import", &binaries.node_modules);
    write_file(
        &repo.join("src/very_unique_name_for_parity_123.js"),
        "module.exports = () => 1;\n",
    );
    write_file(
        &repo.join("src/index.js"),
        "const impl = require('./very_unique_name_for_parity_123');\nmodule.exports = () => impl();\n",
    );
    write_file(
        &repo.join("tests/index.test.js"),
        "const run = require('../src/index');\n\ntest('indirect', () => { expect(run()).toBe(1); });\n",
    );
    write_file(
        &repo.join("tests/unrelated.test.js"),
        "test('unrelated', () => { expect(1).toBe(1); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    assert_parity_with_args(
        &repo,
        &binaries,
        &["src/very_unique_name_for_parity_123.js"],
        &["src/very_unique_name_for_parity_123.js"],
    );
}

#[test]
fn parity_jest_select_handles_pattern_discovery_arg_collision_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo(
        "jest-select-pattern-discovery-arg-collision",
        &binaries.node_modules,
    );
    write_file(&repo.join("src/prod.js"), "exports.value = 1;\n");
    write_file(
        &repo.join("--bad.test.js"),
        "const { value } = require('./src/prod');\n\ntest('bad', () => { expect(value).toBe(1); });\n",
    );
    write_jest_config(&repo, "**/*.test.js");

    assert_parity_with_args(&repo, &binaries, &["src/prod.js"], &["src/prod.js"]);
}
