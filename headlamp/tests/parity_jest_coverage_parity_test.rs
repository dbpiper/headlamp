mod parity_support;

use parity_support::{
    mk_repo, normalize, parity_binaries, run_parity_fixture_with_args, write_file,
};

#[test]
fn parity_jest_coverage_basic_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-coverage-basic", &binaries.node_modules);
    write_file(&repo.join("src/sum.js"), "exports.sum = (a,b) => a + b;\n");
    write_file(&repo.join("src/unused.js"), "exports.unused = () => 123;\n");
    write_file(
        &repo.join("tests/sum.test.js"),
        "const { sum } = require('../src/sum');\n\ntest('sum', () => { expect(sum(1,2)).toBe(3); });\n",
    );
    write_file(
        &repo.join("jest.config.js"),
        "module.exports = { testMatch: ['**/tests/**/*.test.js'], collectCoverage: true, collectCoverageFrom: ['src/**/*.js'] };\n",
    );

    let (code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        &["--coverage"],
        &["--coverage"],
    );
    assert_eq!(code_ts, code_rs);

    let n_ts = normalize(out_ts, &repo);
    let n_rs = normalize(out_rs, &repo);
    assert_eq!(n_ts, n_rs);
}

#[test]
fn parity_jest_coverage_max_hotspots_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-coverage-max-hotspots", &binaries.node_modules);
    write_file(
        &repo.join("src/many.js"),
        "exports.run = () => {\n  const a = 1;\n  if (a === 2) { console.log('never'); }\n  return a;\n};\n\nexports.uncoveredA = () => {\n  const x = 10;\n  const y = 20;\n  return x + y;\n};\n\nexports.uncoveredB = () => {\n  const m = 3;\n  const n = 4;\n  return m * n;\n};\n",
    );
    write_file(
        &repo.join("tests/many.test.js"),
        "const { run } = require('../src/many');\n\ntest('run', () => { expect(run()).toBe(1); });\n",
    );
    write_file(
        &repo.join("jest.config.js"),
        "module.exports = { testMatch: ['**/tests/**/*.test.js'], collectCoverage: true, collectCoverageFrom: ['src/**/*.js'] };\n",
    );

    let (code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        &["--coverage", "--coverage.maxHotspots=1"],
        &["--coverage", "--coverage.maxHotspots=1"],
    );
    assert_eq!(code_ts, code_rs);

    let n_ts = normalize(out_ts, &repo);
    let n_rs = normalize(out_rs, &repo);
    assert_eq!(n_ts, n_rs);
    assert_eq!(n_ts.matches("│Hotspot │").count(), 1, "{n_ts}");
}

#[test]
fn parity_jest_coverage_selection_order_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-coverage-selection-order", &binaries.node_modules);
    write_file(&repo.join("src/a.js"), "exports.a = () => 1;\n");
    write_file(&repo.join("src/b.js"), "exports.b = () => 2;\n");
    write_file(
        &repo.join("tests/a.test.js"),
        "const { a } = require('../src/a');\n\ntest('a', () => { expect(a()).toBe(1); });\n",
    );
    write_file(
        &repo.join("jest.config.js"),
        "module.exports = { testMatch: ['**/tests/**/*.test.js'], collectCoverage: true, collectCoverageFrom: ['src/**/*.js'] };\n",
    );

    let (code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        &["--coverage", "src/a.js"],
        &["--coverage", "src/a.js"],
    );
    assert_eq!(code_ts, code_rs);

    let n_ts = normalize(out_ts, &repo);
    let n_rs = normalize(out_rs, &repo);
    assert_eq!(n_ts, n_rs);
}
