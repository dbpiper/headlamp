mod parity_support;

use parity_support::{
    extract_istanbul_text_table_block, mk_repo, normalize_tty_ui, parity_binaries,
    run_parity_fixture_with_args_tty, write_file, write_jest_config,
};

fn build_uncovered_lines_module(block_count: usize) -> String {
    let mut out = String::from("exports.used = () => 1;\n\n");
    for index in 0..block_count {
        out.push_str("if (false) {\n");
        out.push_str(&format!("  console.log('block-{index}');\n"));
        out.push_str("}\n\n");
    }
    out
}

fn assert_istanbul_table_is_fixed_width(block: &str, max_columns: usize) {
    let stripped_lines = block
        .lines()
        .map(headlamp::format::stacks::strip_ansi_simple)
        .collect::<Vec<_>>();
    let max_width = stripped_lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    assert!(
        max_width <= max_columns,
        "expected table to fit within terminal width {max_columns}, got width {max_width}:\n{block}"
    );

    let dash_lines = stripped_lines
        .iter()
        .filter(|line| line.contains("|---------|") && line.chars().all(|c| c == '-' || c == '|'))
        .collect::<Vec<_>>();
    let dash_width = dash_lines
        .first()
        .map(|line| line.chars().count())
        .unwrap_or(max_width);
    assert!(
        dash_lines
            .iter()
            .all(|line| line.chars().count() == dash_width),
        "expected all dash lines to be fixed-width:\n{block}"
    );
}

#[test]
fn parity_jest_coverage_istanbul_text_table_truncation_does_not_overflow() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-coverage-istanbul-text-table", &binaries.node_modules);
    write_file(
        &repo.join("src/uncoveredLines.js"),
        &build_uncovered_lines_module(140),
    );
    write_file(
        &repo.join("tests/uncoveredLines.test.js"),
        "const { used } = require('../src/uncoveredLines');\n\ntest('used', () => { expect(used()).toBe(1); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    let (_spec, code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args_tty(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        95,
        &["--coverage", "src/uncoveredLines.js"],
        &["--coverage", "src/uncoveredLines.js"],
        "jest",
    );

    assert_eq!(code_ts, 0);
    assert_eq!(code_rs, 0);

    let n_ts = extract_istanbul_text_table_block(&normalize_tty_ui(out_ts, &repo));
    let n_rs = extract_istanbul_text_table_block(&normalize_tty_ui(out_rs, &repo));

    assert!(
        headlamp::format::stacks::strip_ansi_simple(&n_ts).contains("..."),
        "expected TS to truncate uncovered line list with ellipsis, but got:\n{n_ts}"
    );
    assert!(
        headlamp::format::stacks::strip_ansi_simple(&n_rs).contains("..."),
        "expected Rust to truncate uncovered line list with ellipsis, but got:\n{n_rs}"
    );

    assert_istanbul_table_is_fixed_width(&n_ts, 95);
    assert_istanbul_table_is_fixed_width(&n_rs, 95);
}
