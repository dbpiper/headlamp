mod parity_support;

use parity_support::{
    assert_parity_normalized_outputs, mk_repo, normalize_tty_ui, parity_binaries,
    run_parity_fixture_with_args_tty, write_file,
};

#[test]
fn parity_cargo_render_ui_tty_fail_fixture() {
    let Some(binaries) = parity_binaries() else {
        return;
    };

    let repo = mk_repo("cargo-render-ui-tty-fail", &binaries.node_modules);
    write_file(
        &repo.join("Cargo.toml"),
        "\
[package]\n\
name = \"cargo_render_ui_tty_fail\"\n\
version = \"0.1.0\"\n\
edition = \"2024\"\n\
\n\
[lib]\n\
path = \"src/lib.rs\"\n\
",
    );
    write_file(
        &repo.join("src/lib.rs"),
        "\
pub fn sum(a: i32, b: i32) -> i32 {\n\
    a + b\n\
}\n\
",
    );
    write_file(
        &repo.join("tests/sum_test.rs"),
        "\
use cargo_render_ui_tty_fail::sum;\n\
\n\
#[test]\n\
fn sum_passes() {\n\
    assert_eq!(sum(1, 2), 3);\n\
}\n\
\n\
#[test]\n\
fn sum_fails() {\n\
    assert_eq!(sum(1, 2), 4);\n\
}\n\
",
    );

    let (_spec, code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args_tty(
        &repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        120,
        &[],
        &[],
        "cargo-test",
    );

    let n_ts = normalize_tty_ui(out_ts, &repo);
    let n_rs = normalize_tty_ui(out_rs, &repo);
    assert_parity_normalized_outputs(
        &repo,
        "cargo_render_ui_tty_fail",
        code_ts,
        &n_ts,
        code_rs,
        &n_rs,
    );
}
