mod parity_support;
use parity_support::parity_binaries;

mod parity_repo_e2e_support;
use parity_repo_e2e_support::{
    assert_parity, create_test_file, discover_repo, run_npm_test_dev_parity_normalized, setup_repo,
    touch_real_source_file_used_by_tests,
};

#[test]
fn parity_repo_test_dev_matches_ts() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let Some(repo) = discover_repo() else { return };
    let Some(e2e) = setup_repo(repo, &binaries) else {
        return;
    };

    let _touch = touch_real_source_file_used_by_tests(&e2e.repo).or_else(|| {
        create_test_file(
            &e2e.repo,
            "headlamp_parity_smoke.test.js",
            "test('headlamp parity smoke', () => { expect(1 + 1).toBe(2); });\n",
        )
    });

    let (code_ts, out_ts, code_rs, out_rs) = run_npm_test_dev_parity_normalized(&e2e, &[]);
    assert_parity(&e2e, "baseline", code_ts, &out_ts, code_rs, &out_rs);
}
