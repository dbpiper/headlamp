mod parity_support;

use parity_support::{assert_parity, mk_repo, parity_binaries, write_file, write_jest_config};

#[test]
fn parity_jest_pass_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-pass", &binaries.node_modules);
    write_file(&repo.join("src/sum.js"), "exports.sum = (a,b) => a + b;\n");
    write_file(
        &repo.join("tests/sum.test.js"),
        "const { sum } = require('../src/sum');\n\ntest('sum', () => { expect(sum(1,2)).toBe(3); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    assert_parity(&repo, &binaries);
}

#[test]
fn parity_jest_fail_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-fail", &binaries.node_modules);
    write_file(&repo.join("src/sum.js"), "exports.sum = (a,b) => a + b;\n");
    write_file(
        &repo.join("tests/sum.test.js"),
        "const { sum } = require('../src/sum');\n\ntest('sum', () => { expect(sum(1,2)).toBe(4); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    assert_parity(&repo, &binaries);
}
