mod parity_support;

use std::path::Path;

use parity_support::{
    ParityRunGroup, assert_parity_with_diagnostics, mk_temp_dir, parity_meta,
    run_headlamp_with_args_tty, runner_parity_binaries, symlink_dir, write_file,
};

fn write_dual_language_sum_repo(repo: &Path, jest_node_modules: &Path, should_fail: bool) {
    symlink_dir(jest_node_modules, &repo.join("node_modules"));

    write_file(
        &repo.join("jest.config.js"),
        "module.exports = { testMatch: ['**/tests/**/*_test.js'] };\n",
    );
    write_file(&repo.join("src/sum.js"), "exports.sum = (a,b) => a + b;\n");
    write_file(
        &repo.join("tests/sum_test.js"),
        &format!(
            "const {{ sum }} = require('../src/sum');\n\ntest('sum_fails', () => {{ expect(sum(1,2)).toBe({}); }});\n\ntest('sum_passes', () => {{ expect(sum(1,2)).toBe(3); }});\n",
            if should_fail { 4 } else { 3 }
        ),
    );

    write_file(
        &repo.join("Cargo.toml"),
        "\
[package]\n\
name = \"parity_sum\"\n\
version = \"0.1.0\"\n\
edition = \"2024\"\n\
\n\
[lib]\n\
path = \"src/lib.rs\"\n\
",
    );
    write_file(
        &repo.join("src/lib.rs"),
        "pub fn sum(a: i32, b: i32) -> i32 { a + b }\n",
    );
    write_file(
        &repo.join("tests/sum_test.rs"),
        &format!(
            "\
use parity_sum::sum;\n\
\n\
#[test]\n\
fn sum_passes() {{\n\
    assert_eq!(sum(1, 2), 3);\n\
}}\n\
\n\
#[test]\n\
fn sum_fails() {{\n\
    assert_eq!(sum(1, 2), {});\n\
}}\n\
",
            if should_fail { 4 } else { 3 }
        ),
    );
}

fn assert_headlamp_runners_tty_parity(
    repo: &Path,
    headlamp_bin: &Path,
    runner_args: &[(&str, &[&str])],
    case: &str,
) {
    let columns = 120;
    let mut run_specs: Vec<parity_support::ParityRunSpec> = vec![];
    let mut sides: Vec<parity_meta::ParityCompareSideInput> = vec![];
    runner_args.iter().for_each(|(runner, args)| {
        let (spec, exit, raw) =
            run_headlamp_with_args_tty(repo, headlamp_bin, columns, runner, args);
        let raw_bytes = raw.as_bytes().len();
        let raw_lines = raw.lines().count();
        let (normalized, normalization_meta) =
            parity_support::normalize::normalize_tty_ui_runner_parity_with_meta(raw.clone(), repo);
        let normalized_bytes = normalized.as_bytes().len();
        let normalized_lines = normalized.lines().count();
        let side_label = spec.side_label.clone();
        run_specs.push(spec);
        sides.push(parity_meta::ParityCompareSideInput {
            label: side_label,
            exit,
            raw,
            normalized,
            meta: parity_meta::ParitySideMeta {
                raw_bytes,
                raw_lines,
                normalized_bytes,
                normalized_lines,
                normalization: normalization_meta,
            },
        });
    });

    let compare = parity_meta::ParityCompareInput { sides };
    let run_group = ParityRunGroup { sides: run_specs };
    assert_parity_with_diagnostics(repo, case, &compare, Some(&run_group));
}

#[test]
fn parity_runner_all_three_tty_pass() {
    let binaries = runner_parity_binaries();
    let repo = mk_temp_dir("runner-parity-all-three-pass");
    write_dual_language_sum_repo(&repo, &binaries.jest_node_modules, false);

    let runner_args: [(&str, &[&str]); 3] =
        [("jest", &[]), ("cargo-test", &[]), ("cargo-nextest", &[])];
    assert_headlamp_runners_tty_parity(
        &repo,
        &binaries.headlamp_bin,
        &runner_args,
        "all three tty pass",
    );
}

#[test]
fn parity_runner_all_three_tty_fail() {
    let binaries = runner_parity_binaries();
    let repo = mk_temp_dir("runner-parity-all-three-fail");
    write_dual_language_sum_repo(&repo, &binaries.jest_node_modules, true);

    let runner_args: [(&str, &[&str]); 3] =
        [("jest", &[]), ("cargo-test", &[]), ("cargo-nextest", &[])];
    assert_headlamp_runners_tty_parity(
        &repo,
        &binaries.headlamp_bin,
        &runner_args,
        "all three tty fail",
    );
}

#[test]
fn parity_runner_selection_prod_file() {
    let binaries = runner_parity_binaries();
    let repo = mk_temp_dir("runner-parity-selection-prod-file");
    write_dual_language_sum_repo(&repo, &binaries.jest_node_modules, false);

    let jest_args = ["src/sum.js"];
    let cargo_args = ["src/lib.rs"];
    let runner_args: [(&str, &[&str]); 3] = [
        ("jest", &jest_args),
        ("cargo-test", &cargo_args),
        ("cargo-nextest", &cargo_args),
    ];
    assert_headlamp_runners_tty_parity(
        &repo,
        &binaries.headlamp_bin,
        &runner_args,
        "selection prod file all three",
    );
}

#[test]
fn parity_runner_selection_test_file() {
    let binaries = runner_parity_binaries();
    let repo = mk_temp_dir("runner-parity-selection-test-file");
    write_dual_language_sum_repo(&repo, &binaries.jest_node_modules, false);

    let jest_args = ["tests/sum_test.js"];
    let cargo_args = ["tests/sum_test.rs"];
    let runner_args: [(&str, &[&str]); 3] = [
        ("jest", &jest_args),
        ("cargo-test", &cargo_args),
        ("cargo-nextest", &cargo_args),
    ];
    assert_headlamp_runners_tty_parity(
        &repo,
        &binaries.headlamp_bin,
        &runner_args,
        "selection test file all three",
    );
}

#[test]
fn parity_runner_name_pattern_only() {
    let binaries = runner_parity_binaries();
    let repo = mk_temp_dir("runner-parity-name-pattern-only");
    write_dual_language_sum_repo(&repo, &binaries.jest_node_modules, true);

    let jest_args = ["-t", "sum_passes"];
    let cargo_args = ["--", "sum_passes"];
    let runner_args: [(&str, &[&str]); 3] = [
        ("jest", &jest_args),
        ("cargo-test", &cargo_args),
        ("cargo-nextest", &cargo_args),
    ];
    assert_headlamp_runners_tty_parity(
        &repo,
        &binaries.headlamp_bin,
        &runner_args,
        "name pattern only all three",
    );
}

#[test]
fn parity_runner_coverage_ui_jest_suppresses_coverage_output() {
    let binaries = runner_parity_binaries();
    let repo = mk_temp_dir("runner-parity-coverage-ui-jest");
    write_dual_language_sum_repo(&repo, &binaries.jest_node_modules, false);

    let coverage_args = ["--coverage", "--coverage-ui=jest"];
    let runner_args: [(&str, &[&str]); 3] = [
        ("jest", &coverage_args),
        ("cargo-test", &coverage_args),
        ("cargo-nextest", &coverage_args),
    ];
    assert_headlamp_runners_tty_parity(
        &repo,
        &binaries.headlamp_bin,
        &runner_args,
        "coverage-ui=jest all three",
    );
}
