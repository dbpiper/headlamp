mod parity_support;

use parity_support::{mk_repo, parity_binaries, run_parity_fixture_with_args, write_file};

#[test]
fn parity_cache_dir_env_is_honored_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("parity-cache-dir-env-honored", &binaries.node_modules);
    write_file(
        &repo.join("tests/pass.test.js"),
        "test('pass', () => { expect(1).toBe(1); });\n",
    );
    write_file(
        &repo.join("jest.config.js"),
        "module.exports = { testMatch: ['**/tests/**/*.test.js'] };\n",
    );

    let (_spec, code_ts, _out_ts, code_rs, _out_rs) =
        run_parity_fixture_with_args(&repo, &binaries.ts_cli, &binaries.rust_bin, &[], &[]);
    assert_eq!(code_ts, code_rs);

    assert!(
        repo.join(".headlamp-cache-ts").exists(),
        "expected TS cache dir to exist"
    );
    assert!(
        repo.join(".headlamp-cache-rs").exists(),
        "expected Rust cache dir to exist"
    );
}
