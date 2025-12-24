mod parity_support;

use parity_support::{
    assert_parity_with_args, mk_repo, parity_binaries, write_file, write_jest_config,
};

#[test]
fn parity_jest_raw_output_formatter_fallback_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-raw-output-fallback", &binaries.node_modules);
    write_file(&repo.join("src/sum.js"), "exports.sum = (a,b) => a + b;\n");
    write_file(
        &repo.join("tests/sum.test.js"),
        "const { sum } = require('../src/sum');\n\ntest('sum', () => { expect(sum(1,2)).toBe(3); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    // Invalid Jest CLI arg forces Jest to exit early before reporter writes bridge JSON.
    // TS headlamp uses formatJestOutputVitest here; Rust should now match.
    assert_parity_with_args(
        &repo,
        &binaries,
        &["--thisFlagDoesNotExistForJest"],
        &["--thisFlagDoesNotExistForJest"],
    );
}
