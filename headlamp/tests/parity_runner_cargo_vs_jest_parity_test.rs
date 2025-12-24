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

fn write_dual_language_bootstrap_repo(repo: &Path, jest_node_modules: &Path) {
    symlink_dir(jest_node_modules, &repo.join("node_modules"));

    write_file(
        &repo.join("jest.config.js"),
        "module.exports = { testMatch: ['**/tests/**/*_test.js'] };\n",
    );

    write_file(
        &repo.join("tests/bootstrap_test.js"),
        "\
const fs = require('node:fs');\n\
\n\
test('bootstrap_passes', () => {\n\
  const contents = fs.readFileSync('bootstrap.txt', 'utf8');\n\
  expect(contents.trim()).toBe('bootstrap');\n\
});\n",
    );

    write_file(
        &repo.join("Cargo.toml"),
        "\
[package]\n\
name = \"parity_bootstrap\"\n\
version = \"0.1.0\"\n\
edition = \"2024\"\n\
\n\
[lib]\n\
path = \"src/lib.rs\"\n\
",
    );
    write_file(&repo.join("src/lib.rs"), "pub fn noop() {}\n");
    write_file(
        &repo.join("tests/bootstrap_test.rs"),
        "\
use std::fs;\n\
\n\
#[test]\n\
fn bootstrap_passes() {\n\
    let contents = fs::read_to_string(\"bootstrap.txt\").expect(\"bootstrap.txt\");\n\
    assert_eq!(contents.trim(), \"bootstrap\");\n\
}\n\
",
    );
}

fn write_dual_language_show_logs_repo(repo: &Path, jest_node_modules: &Path) {
    symlink_dir(jest_node_modules, &repo.join("node_modules"));

    write_file(
        &repo.join("jest.config.js"),
        "module.exports = { testMatch: ['**/tests/**/*_test.js'] };\n",
    );
    write_file(
        &repo.join("tests/logs_test.js"),
        "\
test('pass', () => {\n\
  console.log('log-pass');\n\
  expect(1).toBe(1);\n\
});\n\
\n\
test('fail', () => {\n\
  console.error('err-fail');\n\
  expect(1).toBe(2);\n\
});\n\
",
    );

    write_file(
        &repo.join("Cargo.toml"),
        "\
[package]\n\
name = \"parity_logs\"\n\
version = \"0.1.0\"\n\
edition = \"2024\"\n\
\n\
[lib]\n\
path = \"src/lib.rs\"\n\
",
    );
    write_file(&repo.join("src/lib.rs"), "pub fn noop() {}\n");
    write_file(
        &repo.join("tests/logs_test.rs"),
        "\
#[test]\n\
fn pass() {\n\
    println!(\"log-pass\");\n\
    assert_eq!(1, 1);\n\
}\n\
\n\
#[test]\n\
fn fail() {\n\
    eprintln!(\"err-fail\");\n\
    assert_eq!(1, 2);\n\
}\n\
",
    );
}

fn write_dual_language_changed_selection_repo(repo: &Path, jest_node_modules: &Path) {
    symlink_dir(jest_node_modules, &repo.join("node_modules"));

    write_file(
        &repo.join("jest.config.js"),
        "module.exports = { testMatch: ['**/tests/**/*_test.js'] };\n",
    );
    write_file(&repo.join("src/a.js"), "exports.a = () => 'a';\n");
    write_file(&repo.join("src/b.js"), "exports.b = () => 'b';\n");
    write_file(
        &repo.join("tests/a_test.js"),
        "const { a } = require('../src/a');\n\ntest('a_passes', () => { expect(a()).toBe('a'); });\n",
    );
    write_file(
        &repo.join("tests/b_test.js"),
        "const { b } = require('../src/b');\n\ntest('b_passes', () => { expect(b()).toBe('b'); });\n",
    );

    write_file(
        &repo.join("Cargo.toml"),
        "\
[package]\n\
name = \"parity_changed\"\n\
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
pub mod a;\n\
pub mod b;\n\
",
    );
    write_file(
        &repo.join("src/a.rs"),
        "pub fn a() -> &'static str { \"a\" }\n",
    );
    write_file(
        &repo.join("src/b.rs"),
        "pub fn b() -> &'static str { \"b\" }\n",
    );
    write_file(
        &repo.join("tests/a_test.rs"),
        "\
use parity_changed::a;\n\
\n\
#[test]\n\
fn a_passes() {\n\
    assert_eq!(a::a(), \"a\");\n\
}\n\
",
    );
    write_file(
        &repo.join("tests/b_test.rs"),
        "\
use parity_changed::b;\n\
\n\
#[test]\n\
fn b_passes() {\n\
    assert_eq!(b::b(), \"b\");\n\
}\n\
",
    );
}

fn write_dual_language_changed_depth_repo(repo: &Path, jest_node_modules: &Path) {
    symlink_dir(jest_node_modules, &repo.join("node_modules"));

    write_file(
        &repo.join("jest.config.js"),
        "module.exports = { testMatch: ['**/tests/**/*_test.js'] };\n",
    );

    write_file(
        &repo.join("src/api/depth_anchor.js"),
        "exports.anchor = () => 'anchor';\n",
    );
    write_file(
        &repo.join("tests/depth_anchor_test.js"),
        "const { anchor } = require('../src/api/depth_anchor');\n\ntest('anchor_passes', () => { expect(anchor()).toBe('anchor'); });\n",
    );
    write_file(
        &repo.join("tests/api_test.js"),
        "test('api_marker', () => { expect('api').toBe('api'); });\n",
    );

    write_file(
        &repo.join("Cargo.toml"),
        "\
[package]\n\
name = \"parity_changed_depth\"\n\
version = \"0.1.0\"\n\
edition = \"2024\"\n\
\n\
[lib]\n\
path = \"src/lib.rs\"\n\
",
    );
    write_file(&repo.join("src/lib.rs"), "pub mod api;\n");
    write_file(&repo.join("src/api.rs"), "pub mod depth_anchor;\n");
    write_file(
        &repo.join("src/api/depth_anchor.rs"),
        "pub fn anchor() -> &'static str { \"anchor\" }\n",
    );
    write_file(
        &repo.join("tests/depth_anchor_test.rs"),
        "\
use parity_changed_depth::api::depth_anchor;\n\
\n\
#[test]\n\
fn anchor_passes() {\n\
    assert_eq!(depth_anchor::anchor(), \"anchor\");\n\
}\n\
",
    );
    write_file(
        &repo.join("tests/api_test.rs"),
        "\
#[test]\n\
fn api_marker() {\n\
    assert_eq!(\"api\", \"api\");\n\
}\n\
",
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
        let raw_bytes = raw.len();
        let raw_lines = raw.lines().count();
        let (normalized, normalization_meta) =
            parity_support::normalize::normalize_tty_ui_runner_parity_with_meta(raw.clone(), repo);
        let normalized_bytes = normalized.len();
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

#[test]
fn parity_runner_bootstrap_command_all_three() {
    let binaries = runner_parity_binaries();
    let repo = mk_temp_dir("runner-parity-bootstrap-command");
    write_dual_language_bootstrap_repo(&repo, &binaries.jest_node_modules);

    let bootstrap_args = ["--bootstrapCommand=echo bootstrap > bootstrap.txt"];
    let runner_args: [(&str, &[&str]); 3] = [
        ("jest", &bootstrap_args),
        ("cargo-test", &bootstrap_args),
        ("cargo-nextest", &bootstrap_args),
    ];
    assert_headlamp_runners_tty_parity(
        &repo,
        &binaries.headlamp_bin,
        &runner_args,
        "bootstrapCommand all three",
    );
}

#[test]
fn parity_runner_show_logs_and_only_failures_all_three() {
    let binaries = runner_parity_binaries();
    let repo = mk_temp_dir("runner-parity-show-logs-only-failures");
    write_dual_language_show_logs_repo(&repo, &binaries.jest_node_modules);

    let args = ["--showLogs", "--onlyFailures"];
    let runner_args: [(&str, &[&str]); 3] = [
        ("jest", &args),
        ("cargo-test", &args),
        ("cargo-nextest", &args),
    ];
    assert_headlamp_runners_tty_parity(
        &repo,
        &binaries.headlamp_bin,
        &runner_args,
        "showLogs+onlyFailures all three",
    );
}

#[test]
fn parity_runner_changed_all_selects_multiple_tests_all_three() {
    let binaries = runner_parity_binaries();
    let repo = mk_temp_dir("runner-parity-changed-all-multi");
    write_dual_language_changed_selection_repo(&repo, &binaries.jest_node_modules);

    parity_support::git_init(&repo);
    parity_support::git_commit_all(&repo, "baseline");

    write_file(
        &repo.join("src/a.js"),
        "exports.a = () => 'a'; // changed\n",
    );
    write_file(
        &repo.join("src/b.js"),
        "exports.b = () => 'b'; // changed\n",
    );
    write_file(
        &repo.join("src/a.rs"),
        "pub fn a() -> &'static str { \"a\" } // changed\n",
    );
    write_file(
        &repo.join("src/b.rs"),
        "pub fn b() -> &'static str { \"b\" } // changed\n",
    );

    let args = ["--changed=all"];
    let runner_args: [(&str, &[&str]); 3] = [
        ("jest", &args),
        ("cargo-test", &args),
        ("cargo-nextest", &args),
    ];
    assert_headlamp_runners_tty_parity(
        &repo,
        &binaries.headlamp_bin,
        &runner_args,
        "changed=all selects multiple tests all three",
    );
}

#[test]
fn parity_runner_coverage_respects_changed_selection_all_three() {
    let binaries = runner_parity_binaries();
    let repo = mk_temp_dir("runner-parity-coverage-changed-selection");
    write_dual_language_changed_selection_repo(&repo, &binaries.jest_node_modules);

    parity_support::git_init(&repo);
    parity_support::git_commit_all(&repo, "baseline");

    write_file(
        &repo.join("src/a.js"),
        "exports.a = () => 'a'; // changed\n",
    );
    write_file(
        &repo.join("src/a.rs"),
        "pub fn a() -> &'static str { \"a\" } // changed\n",
    );

    let args = ["--coverage", "--coverage-ui=jest", "--changed=all"];
    let runner_args: [(&str, &[&str]); 3] = [
        ("jest", &args),
        ("cargo-test", &args),
        ("cargo-nextest", &args),
    ];
    assert_headlamp_runners_tty_parity(
        &repo,
        &binaries.headlamp_bin,
        &runner_args,
        "coverage respects changed selection all three",
    );
}

#[test]
fn parity_runner_changed_depth_affects_jest_and_is_not_overapplied_by_cargo_runners() {
    let binaries = runner_parity_binaries();
    let repo = mk_temp_dir("runner-parity-changed-depth");
    write_dual_language_changed_depth_repo(&repo, &binaries.jest_node_modules);

    parity_support::git_init(&repo);
    parity_support::git_commit_all(&repo, "baseline");

    write_file(
        &repo.join("src/api/depth_anchor.js"),
        "exports.anchor = () => 'anchor'; // changed\n",
    );
    write_file(
        &repo.join("src/api/depth_anchor.rs"),
        "pub fn anchor() -> &'static str { \"anchor\" } // changed\n",
    );

    let depth0_args = ["--changed=all", "--changed.depth=0"];
    let depth1_args = ["--changed=all", "--changed.depth=1"];
    let runner_depth0: [(&str, &[&str]); 3] = [
        ("jest", &depth0_args),
        ("cargo-test", &depth0_args),
        ("cargo-nextest", &depth0_args),
    ];
    let runner_depth1: [(&str, &[&str]); 3] = [
        ("jest", &depth1_args),
        ("cargo-test", &depth1_args),
        ("cargo-nextest", &depth1_args),
    ];

    assert_headlamp_runners_tty_parity(
        &repo,
        &binaries.headlamp_bin,
        &runner_depth0,
        "changed.depth=0 all three",
    );
    assert_headlamp_runners_tty_parity(
        &repo,
        &binaries.headlamp_bin,
        &runner_depth1,
        "changed.depth=1 all three",
    );
}
