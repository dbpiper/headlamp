mod parity_support;

use parity_support::{
    assert_parity_with_args, mk_repo, parity_binaries, write_file, write_jest_config,
};

#[test]
fn parity_jest_name_pattern_only_skips_run_tests_by_path_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-name-pattern-only", &binaries.node_modules);
    write_file(
        &repo.join("tests/pattern.test.js"),
        "test('alpha', () => { expect(1).toBe(1); });\n\ntest('beta', () => { expect(1).toBe(1); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    assert_parity_with_args(&repo, &binaries, &["-t", "alpha"], &["-t", "alpha"]);
}
