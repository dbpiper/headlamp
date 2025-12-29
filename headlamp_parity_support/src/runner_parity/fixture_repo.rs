use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::fs::write_file;
use crate::git::{git_commit_all, git_init};

use super::jest_bin::ensure_repo_local_jest_bin;

pub fn shared_real_runner_repo() -> PathBuf {
    static REPO: OnceLock<PathBuf> = OnceLock::new();
    REPO.get_or_init(|| {
        let process_id = std::process::id();
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("parity-fixtures")
            .join(format!("real-runner-repo-{process_id}"));
        if !repo.exists() {
            std::fs::create_dir_all(&repo).unwrap();
        }
        write_real_runner_repo(&repo);
        repo
    })
    .clone()
}

pub(crate) fn shared_real_runner_repo_for_worktrees() -> PathBuf {
    shared_real_runner_repo()
}

pub fn shared_threshold_real_runner_repo() -> PathBuf {
    static REPO: OnceLock<PathBuf> = OnceLock::new();
    REPO.get_or_init(|| {
        let process_id = std::process::id();
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("parity-fixtures")
            .join(format!("real-runner-repo-thresholds-{process_id}"));
        if !repo.exists() {
            std::fs::create_dir_all(&repo).unwrap();
        }
        write_real_runner_repo(&repo);

        // Make all tests pass so the only failure signal is coverage thresholds.
        write_file(
            &repo.join("tests/sum_fail_test.js"),
            r#"const { sum } = require('../src/sum');

test('test_sum_fails', () => {
  console.log('log-pass');
  console.error('err-fail');
  expect(sum(1, 2)).toBe(3);
});
"#,
        );
        write_file(
            &repo.join("tests/sum_fail_test.rs"),
            "\
use parity_real::sum;\n\
\n\
#[test]\n\
fn test_sum_fails() {\n\
    println!(\"log-pass\");\n\
    eprintln!(\"err-fail\");\n\
    assert_eq!(sum(1, 2), 3);\n\
}\n\
",
        );
        write_file(
            &repo.join("tests/sum_fail_test.py"),
            r#"import sys
from sum import sum_two

def test_sum_fails() -> None:
    print("log-pass")
    sys.stderr.write("err-fail\n")
    assert sum_two(1, 2) == 3
"#,
        );
        write_file(
            &repo.join("headlamp.config.json5"),
            "\
{\n\
  coverage: {\n\
    thresholds: {\n\
      lines: 101,\n\
      functions: 101,\n\
      branches: 101,\n\
    },\n\
  },\n\
}\n\
",
        );
        git_commit_all(&repo, "coverage thresholds");
        repo
    })
    .clone()
}

pub fn write_real_runner_repo(repo: &Path) {
    // Make the repo deterministic and tiny; do NOT delete the dir because we want to reuse
    // compiled artifacts for speed across many parity cases.
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::create_dir_all(repo.join("tests")).unwrap();
    git_init(repo);

    // Remove older layout artifacts (python used to live in pkg/).
    let _ = std::fs::remove_dir_all(repo.join("pkg"));
    let _ = std::fs::remove_dir_all(repo.join("tests/__pycache__"));
    let _ = std::fs::remove_dir_all(repo.join("tests/.pytest_cache"));

    write_js_runner_files(repo);
    write_rust_runner_files(repo);
    write_python_runner_files(repo);

    // Ensure the repo has a usable HEAD for changed-mode scenarios.
    git_commit_all(repo, "init");
}

fn write_js_runner_files(repo: &Path) {
    // JS (Jest)
    write_file(&repo.join("src/sum.js"), "exports.sum = (a,b) => a + b;\n");
    let _ = std::fs::remove_file(repo.join("tests/sum_test.js"));
    let _ = std::fs::remove_file(repo.join("tests/sum_test_test.js"));
    write_file(
        &repo.join("legacy/sum_test_js.txt"),
        "\
const { sum } = require('../src/sum');\n\
\n\
test('sum_passes', () => { expect(sum(1, 2)).toBe(3); });\n\
\n\
test('sum_fails', () => {\n\
  console.log('log-pass');\n\
  console.error('err-fail');\n\
  expect(sum(1, 2)).toBe(4);\n\
});\n\
",
    );
    write_file(
        &repo.join("jest.config.js"),
        "module.exports = { testMatch: ['**/tests/**/*_test.js'] };\n",
    );
    write_file(
        &repo.join("tests/sum_pass_test.js"),
        r#"const { sum } = require('../src/sum');

test('test_sum_passes', () => { expect(sum(1, 2)).toBe(3); });
"#,
    );
    write_file(
        &repo.join("tests/sum_fail_test.js"),
        r#"const { sum } = require('../src/sum');

test('test_sum_fails', () => {
  console.log('log-pass');
  console.error('err-fail');
  expect(sum(1, 2)).toBe(4);
});
"#,
    );
    write_file(&repo.join("src/a.js"), "exports.a = () => 1;\n");
    write_file(&repo.join("src/b.js"), "exports.b = () => 2;\n");
    write_file(
        &repo.join("tests/a_test.js"),
        r#"const { a } = require('../src/a');

test('test_a', () => { expect(a()).toBe(1); });
"#,
    );
    write_file(
        &repo.join("tests/b_test.js"),
        r#"const { b } = require('../src/b');

test('test_b', () => { expect(b()).toBe(2); });
"#,
    );
    write_file(
        &repo.join("src/very_unique_name_for_parity_123.js"),
        "module.exports = () => 123;\n",
    );
    write_file(
        &repo.join("src/index.js"),
        "const impl = require('./very_unique_name_for_parity_123');\nmodule.exports = () => impl();\n",
    );
    write_file(
        &repo.join("tests/index_test.js"),
        r#"const run = require('../src/index');

test('test_indirect', () => { expect(run()).toBe(123); });
"#,
    );

    ensure_repo_local_jest_bin(repo);
}

fn write_rust_runner_files(repo: &Path) {
    // Rust (cargo test / cargo nextest / cargo llvm-cov)
    write_file(
        &repo.join("Cargo.toml"),
        "\
[package]\n\
name = \"parity_real\"\n\
version = \"0.1.0\"\n\
edition = \"2024\"\n\
\n\
[lib]\n\
path = \"src/lib.rs\"\n\
\n\
[workspace]\n\
",
    );
    write_file(
        &repo.join("src/lib.rs"),
        "\
pub mod sum;\n\
pub use sum::sum;\n\
\n\
pub mod a;\n\
pub mod b;\n\
pub mod very_unique_name_for_parity_123;\n\
pub mod index;\n\
",
    );
    write_file(
        &repo.join("src/sum.rs"),
        "#[inline(never)]\npub fn sum(a: i32, b: i32) -> i32 { a + b }\n",
    );
    write_file(
        &repo.join("src/a.rs"),
        "#[inline(never)]\npub fn a() -> i32 { 1 }\n",
    );
    write_file(
        &repo.join("src/b.rs"),
        "#[inline(never)]\npub fn b() -> i32 { 2 }\n",
    );
    write_file(
        &repo.join("src/very_unique_name_for_parity_123.rs"),
        "#[inline(never)]\npub fn impl_() -> i32 { 123 }\n",
    );
    write_file(
        &repo.join("src/index.rs"),
        "#[inline(never)]\npub fn run() -> i32 { crate::very_unique_name_for_parity_123::impl_() }\n",
    );
    write_file(
        &repo.join("legacy/sum_test_rs.txt"),
        "\
use parity_real::sum;\n\
\n\
#[test]\n\
fn sum_passes() {\n\
    assert_eq!(sum(1, 2), 3);\n\
}\n\
\n\
#[test]\n\
fn sum_fails() {\n\
    println!(\"log-pass\");\n\
    eprintln!(\"err-fail\");\n\
    assert_eq!(sum(1, 2), 4);\n\
}\n\
",
    );
    write_file(
        &repo.join("tests/sum_pass_test.rs"),
        "\
use parity_real::sum;\n\
\n\
#[test]\n\
fn test_sum_passes() {\n\
    assert_eq!(sum(1, 2), 3);\n\
}\n\
",
    );
    write_file(
        &repo.join("tests/sum_fail_test.rs"),
        "\
use parity_real::sum;\n\
\n\
#[test]\n\
fn test_sum_fails() {\n\
    println!(\"log-pass\");\n\
    eprintln!(\"err-fail\");\n\
    assert_eq!(sum(1, 2), 4);\n\
}\n\
",
    );
    write_file(
        &repo.join("tests/a_test.rs"),
        "\
use parity_real::a;\n\
\n\
#[test]\n\
fn test_a() {\n\
    assert_eq!(a::a(), 1);\n\
}\n\
",
    );
    write_file(
        &repo.join("tests/b_test.rs"),
        "\
use parity_real::b;\n\
\n\
#[test]\n\
fn test_b() {\n\
    assert_eq!(b::b(), 2);\n\
}\n\
",
    );
    write_file(
        &repo.join("tests/index_test.rs"),
        "\
use parity_real::index;\n\
use parity_real::very_unique_name_for_parity_123;\n\
\n\
#[test]\n\
fn test_indirect() {\n\
    assert_eq!(index::run(), 123);\n\
    assert_eq!(very_unique_name_for_parity_123::impl_(), 123);\n\
}\n\
",
    );
}

fn write_python_runner_files(repo: &Path) {
    // Python (pytest / pytest-cov)
    write_file(&repo.join("pyproject.toml"), "[tool.pytest.ini_options]\n");
    write_file(
        &repo.join("src/sum.py"),
        "def sum_two(a: int, b: int) -> int:\n    return a + b\n",
    );
    write_file(&repo.join("src/a.py"), "def a() -> int:\n    return 1\n");
    write_file(&repo.join("src/b.py"), "def b() -> int:\n    return 2\n");
    write_file(
        &repo.join("src/very_unique_name_for_parity_123.py"),
        "def impl_() -> int:\n    return 123\n",
    );
    write_file(
        &repo.join("src/index.py"),
        "from very_unique_name_for_parity_123 import impl_\n\ndef run() -> int:\n    return impl_()\n",
    );
    write_file(
        &repo.join("legacy/sum_test_py.txt"),
        r#"import sys

def sum_two(a: int, b: int) -> int:
    return a + b

def test_sum_passes() -> None:
    assert sum_two(1, 2) == 3

def test_sum_fails() -> None:
    print("log-pass")
    sys.stderr.write("err-fail\n")
    assert sum_two(1, 2) == 4
"#,
    );
    write_file(
        &repo.join("tests/sum_pass_test.py"),
        r#"from sum import sum_two

def test_sum_passes() -> None:
    assert sum_two(1, 2) == 3
"#,
    );
    write_file(
        &repo.join("tests/sum_fail_test.py"),
        r#"import sys
from sum import sum_two

def test_sum_fails() -> None:
    print("log-pass")
    sys.stderr.write("err-fail\n")
    assert sum_two(1, 2) == 4
"#,
    );
    write_file(
        &repo.join("tests/a_test.py"),
        r#"from a import a

def test_a() -> None:
    assert a() == 1
"#,
    );
    write_file(
        &repo.join("tests/b_test.py"),
        r#"from b import b

def test_b() -> None:
    assert b() == 2
"#,
    );
    write_file(
        &repo.join("tests/index_test.py"),
        r#"from index import run
from very_unique_name_for_parity_123 import impl_

def test_indirect() -> None:
    assert run() == 123
    assert impl_() == 123
"#,
    );
}
