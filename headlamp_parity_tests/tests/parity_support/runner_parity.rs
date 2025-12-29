use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Condvar, Mutex, OnceLock};

use sha1::Digest;

use super::{
    ParityRunGroup, ParityRunSpec, assert_parity_with_diagnostics, git_commit_all, git_init,
    parity_meta, run_headlamp_with_args_tty_env, timing, write_file,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunnerId {
    Jest,
    CargoTest,
    CargoNextest,
    Pytest,
}

impl RunnerId {
    pub fn as_runner_flag_value(self) -> &'static str {
        match self {
            RunnerId::Jest => "jest",
            RunnerId::CargoTest => "cargo-test",
            RunnerId::CargoNextest => "cargo-nextest",
            RunnerId::Pytest => "pytest",
        }
    }

    pub fn as_runner_label(self) -> &'static str {
        match self {
            RunnerId::Jest => "jest",
            RunnerId::CargoTest => "cargo_test",
            RunnerId::CargoNextest => "cargo_nextest",
            RunnerId::Pytest => "pytest",
        }
    }
}

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

fn shared_real_runner_repo_for_worktrees() -> PathBuf {
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

    // Ensure the repo has a usable HEAD for changed-mode scenarios.
    git_commit_all(repo, "init");
}

fn ensure_repo_local_jest_bin(repo: &Path) {
    // Jest runner requires repo-local node_modules/.bin/jest.
    let js_deps_bin = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("headlamp_tests")
        .join("tests")
        .join("js_deps")
        .join("node_modules")
        .join(".bin");
    let jest_src = js_deps_bin.join(if cfg!(windows) { "jest.cmd" } else { "jest" });
    let jest_dst = repo
        .join("node_modules")
        .join(".bin")
        .join(if cfg!(windows) { "jest.cmd" } else { "jest" });
    if !jest_src.exists() {
        return;
    }
    let Some(jest_dst_parent) = jest_dst.parent() else {
        panic_jest_like_setup_failure(
            repo,
            "jest",
            format!(
                "failed to compute jest bin parent for {}",
                jest_dst.display()
            ),
        );
    };
    if let Err(error) = std::fs::create_dir_all(jest_dst_parent) {
        panic_jest_like_setup_failure(
            repo,
            "jest",
            format!("failed to create {} ({})", jest_dst_parent.display(), error),
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let already_correct = std::fs::read_link(&jest_dst)
            .ok()
            .is_some_and(|target| target == jest_src);
        if !already_correct {
            let _ = std::fs::remove_file(&jest_dst);
            let _ = std::fs::remove_dir_all(&jest_dst);
            if let Err(error) = symlink(&jest_src, &jest_dst)
                && error.kind() != std::io::ErrorKind::AlreadyExists
            {
                panic_jest_like_setup_failure(
                    repo,
                    "jest",
                    format!(
                        "failed to symlink {} -> {} ({})",
                        jest_dst.display(),
                        jest_src.display(),
                        error
                    ),
                );
            }
        }
    }
    #[cfg(windows)]
    {
        let _ = std::fs::copy(&jest_src, &jest_dst);
    }
}

fn panic_jest_like_setup_failure(repo: &Path, runner: &str, message: String) -> ! {
    let ctx = headlamp::format::ctx::make_ctx(repo, Some(120), true, true, None);
    let suite_path = format!("headlamp_parity_tests/setup/{runner}");
    let model = headlamp::format::infra_failure::build_infra_failure_test_run_model(
        suite_path.as_str(),
        "Test suite failed to run",
        &message,
    );
    let rendered = headlamp::format::vitest::render_vitest_from_test_model(&model, &ctx, true);
    panic!("{rendered}");
}

fn safe_dir_component(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c.to_ascii_lowercase(),
            _ => '_',
        })
        .collect()
}

fn worktrees_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("parity-fixtures")
        .join("worktrees")
}

fn worktrees_root_for_process() -> PathBuf {
    worktrees_root().join(format!("{}", std::process::id()))
}

fn reset_worktrees_root() {}

#[derive(Debug)]
pub struct RealRunnerWorktreeLease {
    worktree_path: PathBuf,
    pool: Arc<RealRunnerWorktreePool>,
}

impl RealRunnerWorktreeLease {
    pub fn path(&self) -> &Path {
        &self.worktree_path
    }
}

impl Drop for RealRunnerWorktreeLease {
    fn drop(&mut self) {
        self.pool.release(self.worktree_path.clone());
    }
}

#[derive(Debug)]
struct RealRunnerWorktreePool {
    base_repo: PathBuf,
    base_head: String,
    available_worktrees: Mutex<Vec<PathBuf>>,
    available_worktrees_cv: Condvar,
}

fn parse_usize_env(var_name: &str) -> Option<usize> {
    std::env::var(var_name)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|&n| n > 0)
}

fn default_worktree_pool_size() -> usize {
    // nextest runs one Rust test per process (process-per-test), so each process only needs
    // one worktree lease at a time.
    parse_usize_env("HEADLAMP_PARITY_WORKTREE_POOL_SIZE").unwrap_or(1)
}

impl RealRunnerWorktreePool {
    fn new() -> Self {
        let base_repo = shared_real_runner_repo_for_worktrees();
        let base_head = git_rev_parse_head(&base_repo).expect("git rev-parse HEAD failed");
        let process_id = std::process::id();
        let pool_root = worktrees_root_for_process().join("pool");
        let _ = std::fs::create_dir_all(&pool_root);

        let pool_size = default_worktree_pool_size();

        let _ = std::fs::remove_file(base_repo.join(".git/index.lock"));
        let _ = std::fs::remove_file(base_repo.join(".git/config.lock"));

        let mut worktrees = (0..pool_size)
            .map(|index| pool_root.join(format!("wt-{process_id}-{index}")))
            .collect::<Vec<_>>();

        worktrees.iter().for_each(|dir| {
            if dir.exists() {
                return;
            }
            run_git_expect_success(
                &base_repo,
                &[
                    "worktree",
                    "add",
                    "--force",
                    "--detach",
                    dir.to_string_lossy().as_ref(),
                    "HEAD",
                ],
            );
        });

        worktrees.reverse();
        Self {
            base_repo,
            base_head,
            available_worktrees: Mutex::new(worktrees),
            available_worktrees_cv: Condvar::new(),
        }
    }

    fn acquire(&self, lease_name: &str) -> PathBuf {
        let mut available = self.available_worktrees.lock().unwrap();
        let worktree_path = loop {
            if let Some(path) = available.pop() {
                break path;
            }
            available = self.available_worktrees_cv.wait(available).unwrap();
        };

        let _timing =
            timing::TimingGuard::start(format!("lease acquire reset+clean name={lease_name}"));
        run_git_expect_success(
            &worktree_path,
            &["reset", "--hard", self.base_head.as_str(), "-q"],
        );
        run_git_expect_success(&worktree_path, &["clean", "-fdx", "-q"]);
        ensure_repo_local_jest_bin(&worktree_path);
        worktree_path
    }

    fn release(&self, worktree_path: PathBuf) {
        let mut available = self.available_worktrees.lock().unwrap();
        available.push(worktree_path);
        self.available_worktrees_cv.notify_one();
    }
}

pub fn lease_real_runner_worktree(name: &str) -> RealRunnerWorktreeLease {
    static POOL: OnceLock<Arc<RealRunnerWorktreePool>> = OnceLock::new();
    let pool = POOL.get_or_init(|| Arc::new(RealRunnerWorktreePool::new()));
    let worktree_path = pool.acquire(name);
    RealRunnerWorktreeLease {
        worktree_path,
        pool: pool.clone(),
    }
}

fn run_git_expect_success(repo: &Path, args: &[&str]) {
    let status = Command::new("git").current_dir(repo).args(args).status();
    assert!(status.is_ok_and(|s| s.success()), "git {:?} failed", args);
}

fn git_rev_parse_head(repo: &Path) -> Option<String> {
    let out = Command::new("git")
        .current_dir(repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn git_object_exists(repo: &Path, object: &str) -> bool {
    Command::new("git")
        .current_dir(repo)
        .args(["cat-file", "-t", object])
        .status()
        .ok()
        .is_some_and(|s| s.success())
}

fn sha1_12(text: &str) -> String {
    let mut h = sha1::Sha1::new();
    h.update(text.as_bytes());
    let hex = hex::encode(h.finalize());
    hex.chars().take(12).collect()
}

fn git_output_trimmed(repo: &Path, args: &[&str]) -> String {
    Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default()
}

fn repo_state_token(repo: &Path) -> String {
    let head = git_output_trimmed(repo, &["rev-parse", "HEAD"]);
    let staged = git_output_trimmed(repo, &["diff", "--cached", "--name-only"]);
    let unstaged = git_output_trimmed(repo, &["diff", "--name-only"]);
    let payload = format!("head={head}\nstaged={staged}\nunstaged={unstaged}\n");
    sha1_12(&payload)
}

fn worktree_git_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub fn real_runner_worktree(name: &str) -> PathBuf {
    let _timing = timing::TimingGuard::start(format!("worktree total name={name}"));
    let _guard = worktree_git_lock().lock().unwrap();
    let base_repo = shared_real_runner_repo_for_worktrees();
    let safe = safe_dir_component(name);
    let dir = worktrees_root_for_process().join(&safe);

    let _ = std::fs::create_dir_all(worktrees_root_for_process());

    // Defensive cleanup for interrupted previous runs.
    let _ = std::fs::remove_file(base_repo.join(".git/index.lock"));
    let _ = std::fs::remove_file(base_repo.join(".git/config.lock"));

    let base_head = git_rev_parse_head(&base_repo).expect("git rev-parse HEAD failed");

    // Always recreate the worktree to ensure it is attached to the current base repo.
    // This avoids stale `.git` pointers if the fixture repo was recreated between runs.
    {
        let _timing = timing::TimingGuard::start(format!("worktree remove name={name}"));
        let _ = Command::new("git")
            .current_dir(&base_repo)
            .args(["worktree", "remove", "--force"])
            .arg(&dir)
            .status();
    }
    let _ = std::fs::remove_dir_all(&dir);
    {
        let _timing = timing::TimingGuard::start(format!("worktree add name={name}"));
        run_git_expect_success(
            &base_repo,
            &[
                "worktree",
                "add",
                "--force",
                "--detach",
                dir.to_string_lossy().as_ref(),
                "HEAD",
            ],
        );
    }

    // Reset to the baseline commit and purge untracked files so each test starts clean.
    let worktree_branch = format!("parity-worktree-{safe}");
    {
        let _timing = timing::TimingGuard::start(format!("worktree reset_hard name={name}"));
        run_git_expect_success(&dir, &["reset", "--hard", base_head.as_str(), "-q"]);
    }
    {
        let _timing = timing::TimingGuard::start(format!("worktree clean name={name}"));
        run_git_expect_success(&dir, &["clean", "-fdx", "-q"]);
    }
    {
        let _timing = timing::TimingGuard::start(format!("worktree checkout_branch name={name}"));
        run_git_expect_success(
            &dir,
            &[
                "checkout",
                "-B",
                worktree_branch.as_str(),
                base_head.as_str(),
                "-q",
            ],
        );
    }
    ensure_repo_local_jest_bin(&dir);
    dir
}

#[derive(Debug, Clone)]
struct CachedRunnerParitySide {
    spec: ParityRunSpec,
    exit: i32,
    raw: String,
    normalized: String,
    meta: parity_meta::ParitySideMeta,
}

#[derive(Debug, Clone, Eq)]
struct RunnerParityCacheKey {
    repo: String,
    runner: RunnerId,
    columns: usize,
    args: Vec<String>,
    extra_env: Vec<(String, String)>,
}

impl PartialEq for RunnerParityCacheKey {
    fn eq(&self, other: &Self) -> bool {
        self.repo == other.repo
            && self.runner == other.runner
            && self.columns == other.columns
            && self.args == other.args
            && self.extra_env == other.extra_env
    }
}

impl Hash for RunnerParityCacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.repo.hash(state);
        self.runner.hash(state);
        self.columns.hash(state);
        self.args.hash(state);
        self.extra_env.hash(state);
    }
}

type RunnerParityRunCache =
    Mutex<HashMap<RunnerParityCacheKey, Arc<OnceLock<Arc<CachedRunnerParitySide>>>>>;

fn runner_parity_run_cache() -> &'static RunnerParityRunCache {
    static CACHE: OnceLock<RunnerParityRunCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn sorted_extra_env(extra_env: &[(&str, String)]) -> Vec<(String, String)> {
    let mut out = extra_env
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect::<Vec<_>>();
    out.sort_by(|(ka, va), (kb, vb)| ka.cmp(kb).then_with(|| va.cmp(vb)));
    out
}

fn mk_runner_parity_cache_key(
    repo_cache_key: &str,
    runner: RunnerId,
    columns: usize,
    args: &[&str],
    extra_env: &[(&str, String)],
) -> RunnerParityCacheKey {
    RunnerParityCacheKey {
        repo: repo_cache_key.to_string(),
        runner,
        columns,
        args: args.iter().map(|s| (*s).to_string()).collect(),
        extra_env: sorted_extra_env(extra_env),
    }
}

fn run_and_normalize_cached(
    repo: &Path,
    repo_cache_key: &str,
    headlamp_bin: &Path,
    columns: usize,
    runner: RunnerId,
    args: &[&str],
    extra_env: &[(&str, String)],
) -> Arc<CachedRunnerParitySide> {
    let key = mk_runner_parity_cache_key(repo_cache_key, runner, columns, args, extra_env);
    let cell = {
        let mut locked = runner_parity_run_cache().lock().unwrap();
        locked
            .entry(key)
            .or_insert_with(|| Arc::new(OnceLock::new()))
            .clone()
    };
    cell.get_or_init(|| {
        Arc::new(run_and_normalize(
            repo,
            headlamp_bin,
            columns,
            runner,
            args,
            extra_env,
        ))
    })
    .clone()
}

fn run_and_normalize(
    repo: &Path,
    headlamp_bin: &Path,
    columns: usize,
    runner: RunnerId,
    args: &[&str],
    extra_env: &[(&str, String)],
) -> CachedRunnerParitySide {
    let (spec, exit, raw) = {
        let _timing = timing::TimingGuard::start(format!(
            "runner exec case_repo={} runner={}",
            repo.to_string_lossy(),
            runner.as_runner_label()
        ));
        run_headlamp_with_args_tty_env(
            repo,
            headlamp_bin,
            columns,
            runner.as_runner_flag_value(),
            args,
            extra_env,
        )
    };
    let raw_bytes = raw.len();
    let raw_lines = raw.lines().count();
    let (normalized, normalization_meta) = {
        let _timing = timing::TimingGuard::start(format!(
            "runner normalize case_repo={} runner={}",
            repo.to_string_lossy(),
            runner.as_runner_label()
        ));
        super::normalize::normalize_tty_ui_runner_parity_with_meta(raw.clone(), repo)
    };
    let normalized_bytes = normalized.len();
    let normalized_lines = normalized.lines().count();
    CachedRunnerParitySide {
        spec,
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
    }
}

fn snapshot_name_from_case(case: &str) -> String {
    case.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' => c.to_ascii_lowercase(),
            _ => '_',
        })
        .collect::<String>()
}

pub fn assert_runner_parity_tty_snapshot_all_four_env(
    repo: &Path,
    headlamp_bin: &Path,
    case: &str,
    runners: &[(RunnerId, &[&str])],
    extra_env: &[(&str, String)],
) {
    let _timing = timing::TimingGuard::start(format!("case total case={case}"));
    let repo_cache_key = format!(
        "{}:{}",
        headlamp::fast_related::stable_repo_key_hash_12(repo),
        repo_state_token(repo)
    );
    let columns = 120;
    let sides = std::thread::scope(|scope| {
        runners
            .iter()
            .map(|(runner, args)| {
                let runner_id = *runner;
                let runner_args = *args;
                let repo_cache_key = repo_cache_key.clone();
                scope.spawn(move || {
                    run_and_normalize_cached(
                        repo,
                        repo_cache_key.as_str(),
                        headlamp_bin,
                        columns,
                        runner_id,
                        runner_args,
                        extra_env,
                    )
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });

    {
        let _timing = timing::TimingGuard::start(format!("case compare case={case}"));
        let compare = parity_meta::ParityCompareInput {
            sides: sides
                .iter()
                .map(|side| parity_meta::ParityCompareSideInput {
                    label: side.spec.side_label.clone(),
                    exit: side.exit,
                    raw: side.raw.clone(),
                    normalized: side.normalized.clone(),
                    meta: side.meta.clone(),
                })
                .collect(),
        };
        let run_group = ParityRunGroup {
            sides: sides.iter().map(|side| side.spec.clone()).collect(),
        };
        assert_parity_with_diagnostics(repo, case, &compare, Some(&run_group));
    }

    let canonical = sides
        .first()
        .map(|s| s.normalized.clone())
        .unwrap_or_default();
    let snapshot_name = snapshot_name_from_case(case);
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path(Path::new("tests/snapshots/runner_parity"));
    {
        let _timing = timing::TimingGuard::start(format!("case snapshot case={case}"));
        settings.bind(|| {
            insta::assert_snapshot!(snapshot_name, canonical);
        });
    }
}

pub fn assert_runner_parity_tty_all_four(
    repo: &Path,
    headlamp_bin: &Path,
    case: &str,
    runners: &[(RunnerId, &[&str])],
) {
    assert_runner_parity_tty_all_four_env(repo, headlamp_bin, case, runners, &[]);
}

pub fn assert_runner_parity_tty_all_four_env(
    repo: &Path,
    headlamp_bin: &Path,
    case: &str,
    runners: &[(RunnerId, &[&str])],
    extra_env: &[(&str, String)],
) {
    let columns = 120;
    let mut run_specs: Vec<ParityRunSpec> = vec![];
    let mut sides: Vec<parity_meta::ParityCompareSideInput> = vec![];
    runners.iter().for_each(|(runner, args)| {
        let (spec, exit, raw) = run_headlamp_with_args_tty_env(
            repo,
            headlamp_bin,
            columns,
            runner.as_runner_flag_value(),
            args,
            extra_env,
        );
        let raw_bytes = raw.len();
        let raw_lines = raw.lines().count();
        let (normalized, normalization_meta) =
            super::normalize::normalize_tty_ui_runner_parity_with_meta(raw.clone(), repo);
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

pub fn runner_parity_headlamp_bin() -> PathBuf {
    std::env::var("CARGO_BIN_EXE_headlamp")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .unwrap_or_else(super::ensure_headlamp_bin_from_target_dir)
}
