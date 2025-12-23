mod parity_support;

use parity_support::{assert_parity, mk_repo, parity_binaries, write_file, write_jest_config};

#[test]
fn parity_jest_discovery_error_from_invalid_config_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };
    let repo = mk_repo(
        "parity_jest_discovery_error_from_invalid_config_fixture",
        &binaries.node_modules,
    );

    write_jest_config(&repo, "**/*.test.js");
    write_file(
        &repo.join("jest.config.js"),
        "module.exports = { preset: 'does-not-exist', testMatch: ['**/*.test.js'] };\n",
    );
    write_file(
        &repo.join("tests/a.test.js"),
        "test('a', () => { expect(1).toBe(1); });\n",
    );

    assert_parity(&repo, &binaries);
}
