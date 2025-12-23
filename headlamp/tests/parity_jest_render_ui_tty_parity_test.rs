mod parity_support;

use parity_support::{
    assert_parity_normalized_outputs, mk_repo, normalize_tty_ui, parity_binaries,
    run_parity_fixture_with_args_tty, write_file, write_jest_config,
};

#[test]
fn parity_jest_render_ui_tty_pass_120_cols_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };
    let repo = mk_repo("jest-render-ui-tty-pass", &binaries.node_modules);
    write_file(
        &repo.join("src/sum.js"),
        "exports.sum = (a, b) => a + b;\n",
    );
    write_file(
        &repo.join("tests/sum.test.js"),
        "const { sum } = require('../src/sum');\n\ntest('sum works', () => { expect(sum(1,2)).toBe(3); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    let (code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args_tty(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        120,
        &[],
        &[],
    );

    let n_ts = normalize_tty_ui(out_ts, &repo);
    let n_rs = normalize_tty_ui(out_rs, &repo);
    assert_parity_normalized_outputs(&repo, "pass_120", code_ts, &n_ts, code_rs, &n_rs);
}

#[test]
fn parity_jest_render_ui_tty_fail_120_cols_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };
    let repo = mk_repo("jest-render-ui-tty-fail", &binaries.node_modules);
    write_file(
        &repo.join("src/sum.js"),
        "exports.sum = (a, b) => a + b;\n",
    );
    write_file(
        &repo.join("tests/sum.test.js"),
        "const { sum } = require('../src/sum');\n\ntest('sum fails', () => { expect(sum(1,2)).toBe(4); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    let (code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args_tty(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        120,
        &[],
        &[],
    );

    let n_ts = normalize_tty_ui(out_ts, &repo);
    let n_rs = normalize_tty_ui(out_rs, &repo);
    assert_parity_normalized_outputs(&repo, "fail_120", code_ts, &n_ts, code_rs, &n_rs);
}

#[test]
fn parity_jest_render_ui_tty_pass_60_cols_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };
    let repo = mk_repo("jest-render-ui-tty-pass-narrow", &binaries.node_modules);
    write_file(
        &repo.join("tests/long_names.test.js"),
        "test('a very long test name that should still render in narrow terminals', () => { expect(1).toBe(1); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    let (code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args_tty(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        60,
        &[],
        &[],
    );

    let n_ts = normalize_tty_ui(out_ts, &repo);
    let n_rs = normalize_tty_ui(out_rs, &repo);
    assert_parity_normalized_outputs(&repo, "pass_60", code_ts, &n_ts, code_rs, &n_rs);
}


