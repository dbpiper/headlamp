mod parity_support;

use parity_support::{
    assert_parity_normalized_outputs, extract_coverage_ui_block, mk_repo, normalize_tty_ui,
    parity_binaries, run_parity_fixture_with_args_tty, write_file, write_jest_config,
};

#[test]
fn parity_jest_coverage_ui_tty_matches_ts_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-coverage-ui-tty", &binaries.node_modules);
    write_file(
        &repo.join("utils/logging/eventPresentation.js"),
        "\
exports.presentEvent = (input) => {
  if (input.kind === 'a') {
    if (input.value > 10) return 'A:big';
    return 'A:small';
  }
  if (input.value % 2 === 0) return 'B:even';
  return 'B:odd';
};
",
    );
    write_file(
        &repo.join("tests/eventPresentation.test.js"),
        "\
const { presentEvent } = require('../utils/logging/eventPresentation.js');

test('presentEvent a small', () => {
  expect(presentEvent({ kind: 'a', value: 1 })).toBe('A:small');
});
",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    let (_spec, code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args_tty(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        120,
        &["--coverage", "utils/logging/eventPresentation.js"],
        &["--coverage", "utils/logging/eventPresentation.js"],
        "jest",
    );

    let n_ts = extract_coverage_ui_block(&normalize_tty_ui(out_ts, &repo));
    let n_rs = extract_coverage_ui_block(&normalize_tty_ui(out_rs, &repo));
    assert_parity_normalized_outputs(&repo, "coverage_ui_tty", code_ts, &n_ts, code_rs, &n_rs);
}
