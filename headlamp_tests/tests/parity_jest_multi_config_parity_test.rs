mod parity_support;

use parity_support::{
    assert_parity, assert_parity_with_args, mk_repo, parity_binaries, write_file,
};

#[test]
fn parity_jest_two_project_configs_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-two-configs", &binaries.node_modules);
    write_file(&repo.join("src/a.js"), "exports.a = () => 1;\n");
    write_file(&repo.join("src/b.js"), "exports.b = () => 2;\n");
    write_file(
        &repo.join("tests/a.test.js"),
        "const { a } = require('../src/a');\n\ntest('a', () => { expect(a()).toBe(1); });\n",
    );
    write_file(
        &repo.join("tests/b.test.js"),
        "const { b } = require('../src/b');\n\ntest('b', () => { expect(b()).toBe(2); });\n",
    );
    write_file(
        &repo.join("jest.config.js"),
        "module.exports = { testMatch: ['**/tests/a.test.js'] };\n",
    );
    write_file(
        &repo.join("jest.ts.config.js"),
        "module.exports = { testMatch: ['**/tests/b.test.js'] };\n",
    );

    assert_parity(&repo, &binaries);
}

#[test]
fn parity_jest_two_project_configs_select_one_test_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-two-configs-select-one", &binaries.node_modules);
    write_file(&repo.join("src/sum.js"), "exports.sum = (a,b) => a + b;\n");
    write_file(
        &repo.join("tests/a.test.js"),
        "const { sum } = require('../src/sum');\n\ntest('a', () => { expect(sum(1,2)).toBe(3); });\n",
    );
    write_file(
        &repo.join("tests/b.test.js"),
        "test('b', () => { expect(1).toBe(1); });\n",
    );
    write_file(
        &repo.join("jest.config.js"),
        "module.exports = { testMatch: ['**/tests/a.test.js'] };\n",
    );
    write_file(
        &repo.join("jest.ts.config.js"),
        "module.exports = { testMatch: ['**/tests/b.test.js'] };\n",
    );

    assert_parity_with_args(&repo, &binaries, &["tests/a.test.js"], &["tests/a.test.js"]);
}

#[test]
fn parity_jest_two_project_configs_shared_test_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-two-configs-shared-test", &binaries.node_modules);
    write_file(
        &repo.join("tests/shared.test.js"),
        "test('shared', () => { expect(1).toBe(1); });\n",
    );
    write_file(
        &repo.join("jest.config.js"),
        "module.exports = { testMatch: ['**/tests/shared.test.js'] };\n",
    );
    write_file(
        &repo.join("jest.ts.config.js"),
        "module.exports = { testMatch: ['**/tests/shared.test.js'] };\n",
    );

    assert_parity_with_args(
        &repo,
        &binaries,
        &["tests/shared.test.js"],
        &["tests/shared.test.js"],
    );
}
