mod parity_support;

use parity_support::{
    mk_repo, parity_binaries, run_rust_fixture_with_args_tty_stdout_piped, write_file,
    write_jest_config,
};

#[test]
fn parity_jest_render_ui_tty_stdout_piped_keeps_color_and_width() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("jest-render-ui-tty-stdout-piped", &binaries.node_modules);
    write_file(
        &repo.join("tests/long_names.test.js"),
        "test('a very long test name that should still render in narrow terminals', () => { expect(1).toBe(1); });\n",
    );
    write_jest_config(&repo, "**/tests/**/*.test.js");

    let (code_rs, out_rs) = run_rust_fixture_with_args_tty_stdout_piped(
        &repo,
        &binaries.rust_bin,
        120,
        &[],
    );

    assert_eq!(code_rs, 0);

    let has_truecolor = out_rs.contains("\u{1b}[38;2;") || out_rs.contains("\u{1b}[48;2;");
    assert!(
        has_truecolor,
        "expected ANSI color output even when stdout is piped (stderr TTY); got:\n{out_rs}"
    );

    let has_wide_rule = out_rs.lines().any(|line| {
        let stripped = headlamp_core::format::stacks::strip_ansi_simple(line);
        stripped.contains('─') && stripped.chars().count() >= 100
    });
    assert!(
        has_wide_rule,
        "expected wide-rule/table rendering based on TTY columns even when stdout is piped; got:\n{out_rs}"
    );
}


