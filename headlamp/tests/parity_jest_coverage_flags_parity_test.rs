mod parity_support;

use parity_support::{
    assert_parity_non_tty_with_diagnostics, assert_parity_normalized_outputs,
    assert_parity_tty_ui_with_diagnostics, mk_repo, normalize, normalize_tty_ui, parity_binaries,
    run_parity_fixture_with_args, run_parity_fixture_with_args_tty, write_file, write_jest_config,
};

fn write_multi_file_coverage_repo(repo: &std::path::Path, file_count: usize) {
    (0..file_count).for_each(|index| {
        let file_name = format!("src/file_{index}.js");
        let body = format!(
            "exports.used = () => {index};\n\nexports.uncovered_{index} = () => {{\n  const a = 1;\n  const b = 2;\n  return a + b;\n}};\n"
        );
        write_file(&repo.join(file_name), &body);
    });

    write_file(
        &repo.join("tests/coverage.test.js"),
        "const { used } = require('../src/file_0');\n\ntest('used', () => { expect(used()).toBe(0); });\n",
    );

    write_file(
        &repo.join("jest.config.js"),
        "module.exports = { testMatch: ['**/tests/**/*.test.js'], collectCoverage: true, collectCoverageFrom: ['src/**/*.js'] };\n",
    );
}

#[test]
fn parity_jest_coverage_page_fit_tty_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-coverage-page-fit", &binaries.node_modules);
    write_multi_file_coverage_repo(&repo, 8);

    let (_spec, code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args_tty(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        120,
        &["--coverage", "--coverage.pageFit=true"],
        &["--coverage", "--coverage.pageFit=true"],
    );

    let n_ts = normalize_tty_ui(out_ts, &repo);
    let n_rs = normalize_tty_ui(out_rs, &repo);
    assert_parity_normalized_outputs(&repo, "coverage_page_fit", code_ts, &n_ts, code_rs, &n_rs);

    let stripped = headlamp_core::format::stacks::strip_ansi_simple(&n_ts);
    assert!(
        stripped.contains("│File") && stripped.contains("│Section"),
        "expected per-file table header to be present:\n{n_ts}"
    );
}

#[test]
fn parity_jest_coverage_max_files_tty_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-coverage-max-files", &binaries.node_modules);
    write_multi_file_coverage_repo(&repo, 10);

    let (_spec, code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args_tty(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        120,
        &["--coverage", "--coverage.maxFiles=3"],
        &["--coverage", "--coverage.maxFiles=3"],
    );

    let n_ts = normalize_tty_ui(out_ts.clone(), &repo);
    let _n_rs = normalize_tty_ui(out_rs.clone(), &repo);
    assert_parity_tty_ui_with_diagnostics(
        &repo,
        "coverage_max_files",
        code_ts,
        out_ts,
        code_rs,
        out_rs,
        Some(&_spec),
    );

    assert!(
        headlamp_core::format::stacks::strip_ansi_simple(&n_ts).contains("src/file_9.js"),
        "expected pretty output to still include many files (TS parity):\n{n_ts}"
    );
}

#[test]
fn parity_jest_coverage_include_exclude_tty_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-coverage-include-exclude", &binaries.node_modules);
    write_multi_file_coverage_repo(&repo, 4);

    let (_spec, code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args_tty(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        120,
        &[
            "--coverage",
            "--coverage.include=src/file_0.js,src/file_1.js",
            "--coverage.exclude=src/file_1.js",
        ],
        &[
            "--coverage",
            "--coverage.include=src/file_0.js,src/file_1.js",
            "--coverage.exclude=src/file_1.js",
        ],
    );

    let n_ts = normalize_tty_ui(out_ts.clone(), &repo);
    let _n_rs = normalize_tty_ui(out_rs.clone(), &repo);
    assert_parity_tty_ui_with_diagnostics(
        &repo,
        "coverage_include_exclude",
        code_ts,
        out_ts,
        code_rs,
        out_rs,
        Some(&_spec),
    );

    assert!(
        n_ts.contains("file_0.js"),
        "expected included file_0.js to appear:\n{n_ts}"
    );
    assert!(
        !n_ts.contains("file_2.js") && !n_ts.contains("file_3.js"),
        "expected non-included files to be filtered out:\n{n_ts}"
    );
    assert!(
        !n_ts.contains("file_1.js"),
        "expected excluded file_1.js to be filtered out:\n{n_ts}"
    );
}

#[test]
fn parity_jest_coverage_detail_all_prints_hotspots_non_tty_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-coverage-detail-all", &binaries.node_modules);
    write_multi_file_coverage_repo(&repo, 3);

    let (_spec, code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        &[
            "--coverage",
            "--coverage.detail=all",
            "--coverage.mode=compact",
        ],
        &[
            "--coverage",
            "--coverage.detail=all",
            "--coverage.mode=compact",
        ],
    );

    assert_eq!(code_ts, code_rs);
    let n_ts = normalize(out_ts.clone(), &repo);
    let _n_rs = normalize(out_rs.clone(), &repo);
    assert_parity_non_tty_with_diagnostics(
        &repo,
        "coverage_detail_all_non_tty",
        code_ts,
        out_ts,
        code_rs,
        out_rs,
        Some(&_spec),
    );

    assert!(
        n_ts.contains("src/file_"),
        "expected compact mode file rows to appear:\n{n_ts}"
    );
}

#[test]
fn parity_jest_coverage_editor_links_tty_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-coverage-editor-links", &binaries.node_modules);
    write_multi_file_coverage_repo(&repo, 2);

    let (_spec, code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args_tty(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        120,
        &[
            "--coverage",
            "--coverage.detail=all",
            "--coverage.mode=compact",
            "--coverage.editor=vscode://file/{file}:{line}",
        ],
        &[
            "--coverage",
            "--coverage.detail=all",
            "--coverage.mode=compact",
            "--coverage.editor=vscode://file/{file}:{line}",
        ],
    );

    let out_ts_raw = out_ts.clone();
    let out_rs_raw = out_rs.clone();
    let n_ts = normalize_tty_ui(out_ts.clone(), &repo);
    let n_rs = normalize_tty_ui(out_rs.clone(), &repo);
    assert_parity_tty_ui_with_diagnostics(
        &repo,
        "coverage_editor_links_tty",
        code_ts,
        out_ts,
        code_rs,
        out_rs,
        Some(&_spec),
    );

    assert!(
        out_ts_raw.contains("\u{1b}]8;;"),
        "expected OSC8 links in TTY output when --coverage.editor is set:\n{n_ts}"
    );
    assert!(
        out_rs_raw.contains("\u{1b}]8;;"),
        "expected OSC8 links in TTY output when --coverage.editor is set:\n{n_rs}"
    );
}

#[test]
fn parity_jest_coverage_abort_on_failure_flag_parses_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-coverage-abort-on-failure", &binaries.node_modules);
    write_file(&repo.join("src/sum.js"), "exports.sum = (a,b) => a + b;\n");
    write_file(
        &repo.join("tests/sum.test.js"),
        "const { sum } = require('../src/sum');\n\ntest('sum', () => { expect(sum(1,2)).toBe(3); });\n",
    );
    write_file(
        &repo.join("jest.config.js"),
        "module.exports = { testMatch: ['**/tests/**/*.test.js'], collectCoverage: true, collectCoverageFrom: ['src/**/*.js'], coverageThreshold: { global: { lines: 101 } } };\n",
    );

    let (_spec, code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        &["--coverage", "--coverage.abortOnFailure=true"],
        &["--coverage", "--coverage.abortOnFailure=true"],
    );

    assert_eq!(code_ts, code_rs);
    assert_ne!(
        code_ts, 0,
        "expected coverage threshold to fail (lines=101)"
    );
    let n_ts = normalize(out_ts, &repo);
    let n_rs = normalize(out_rs, &repo);
    assert_eq!(n_ts, n_rs);
}

#[test]
fn parity_jest_coverage_ui_flag_accepts_kebab_and_camel_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-coverage-ui-flag-variants", &binaries.node_modules);
    write_file(
        &repo.join("tests/pass.test.js"),
        "test('pass', () => { expect(1).toBe(1); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    let (_spec_a, code_ts_a, out_ts_a, code_rs_a, out_rs_a) = run_parity_fixture_with_args(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        &["--coverage", "--coverage-ui=jest"],
        &["--coverage", "--coverage-ui=jest"],
    );
    assert_eq!(code_ts_a, code_rs_a);
    let n_ts_a = normalize(out_ts_a, &repo);
    let n_rs_a = normalize(out_rs_a, &repo);
    assert_eq!(n_ts_a, n_rs_a);

    let (_spec_b, code_ts_b, out_ts_b, code_rs_b, out_rs_b) = run_parity_fixture_with_args(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        &["--coverage", "--coverageUi=jest"],
        &["--coverage", "--coverageUi=jest"],
    );
    assert_eq!(code_ts_b, code_rs_b);
    let n_ts_b = normalize(out_ts_b, &repo);
    let n_rs_b = normalize(out_rs_b, &repo);
    assert_eq!(n_ts_b, n_rs_b);
}
