use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

use super::fixture_repo::shared_real_runner_repo_for_worktrees;
use super::git_utils::{git_rev_parse_head, run_git_expect_success};
use super::jest_bin::ensure_repo_local_jest_bin;

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
    // Runner parity executes up to 4 runners concurrently (jest/cargo-test/cargo-nextest/pytest),
    // so each test process should have at least 4 worktrees available to avoid serializing.
    parse_usize_env("HEADLAMP_PARITY_WORKTREE_POOL_SIZE").unwrap_or(4)
}

impl RealRunnerWorktreePool {
    fn new(base_repo: PathBuf) -> Self {
        let process_id = std::process::id();
        let pool_root = worktrees_root_for_process();
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

        let _timing = crate::timing::TimingGuard::start(format!(
            "lease acquire reset+clean name={lease_name}"
        ));
        let base_head =
            git_rev_parse_head(&self.base_repo).expect("git rev-parse HEAD failed (pool acquire)");
        run_git_expect_success(
            &worktree_path,
            &["reset", "--hard", base_head.as_str(), "-q"],
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

fn pools_by_repo() -> &'static Mutex<HashMap<String, Arc<RealRunnerWorktreePool>>> {
    static POOLS: OnceLock<Mutex<HashMap<String, Arc<RealRunnerWorktreePool>>>> = OnceLock::new();
    POOLS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pool_for_repo(repo: &Path) -> Arc<RealRunnerWorktreePool> {
    let key = repo.to_string_lossy().to_string();
    let mut locked = pools_by_repo().lock().unwrap();
    locked
        .entry(key)
        .or_insert_with(|| Arc::new(RealRunnerWorktreePool::new(repo.to_path_buf())))
        .clone()
}

pub fn lease_worktree_for_repo(repo: &Path, name: &str) -> RealRunnerWorktreeLease {
    let pool = pool_for_repo(repo);
    let worktree_path = pool.acquire(name);
    RealRunnerWorktreeLease {
        worktree_path,
        pool,
    }
}

pub fn lease_real_runner_worktree(name: &str) -> RealRunnerWorktreeLease {
    let base_repo = shared_real_runner_repo_for_worktrees();
    lease_worktree_for_repo(&base_repo, name)
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
    worktrees_root().join("pool")
}

#[derive(Debug)]
struct WorktreeGitLockGuard {
    lock_dir: PathBuf,
}

impl Drop for WorktreeGitLockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.lock_dir);
    }
}

fn acquire_worktree_git_lock() -> WorktreeGitLockGuard {
    let lock_dir = worktrees_root().join("git-lock");
    let mut attempts: usize = 0;
    loop {
        match std::fs::create_dir(&lock_dir) {
            Ok(()) => return WorktreeGitLockGuard { lock_dir },
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                attempts = attempts.saturating_add(1);
                if attempts > 600 {
                    panic!(
                        "timed out waiting for worktree git lock {}",
                        lock_dir.display()
                    );
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(error) => panic!(
                "failed to acquire worktree git lock {} ({error})",
                lock_dir.display()
            ),
        }
    }
}

fn prune_stale_worktree_admin_dirs(base_repo: &Path, safe_worktree_name: &str) {
    run_git_expect_success(base_repo, &["worktree", "prune", "--expire", "now"]);
    let admin_root = base_repo.join(".git").join("worktrees");
    let Ok(entries) = std::fs::read_dir(&admin_root) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(|s| s.to_string()) else {
            continue;
        };
        if !name.starts_with(safe_worktree_name) {
            continue;
        }
        let _ = std::fs::remove_dir_all(admin_root.join(name));
    }
}

pub fn real_runner_worktree(name: &str) -> PathBuf {
    let _timing = crate::timing::TimingGuard::start(format!("worktree total name={name}"));
    let _lock = acquire_worktree_git_lock();
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
        let _timing = crate::timing::TimingGuard::start(format!("worktree remove name={name}"));
        let _ = Command::new("git")
            .current_dir(&base_repo)
            .args(["worktree", "remove", "--force"])
            .arg(&dir)
            .status();
    }
    let _ = std::fs::remove_dir_all(&dir);
    {
        let _timing = crate::timing::TimingGuard::start(format!("worktree add name={name}"));
        prune_stale_worktree_admin_dirs(&base_repo, safe.as_str());
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
        let _timing = crate::timing::TimingGuard::start(format!("worktree reset_hard name={name}"));
        run_git_expect_success(&dir, &["reset", "--hard", base_head.as_str(), "-q"]);
    }
    {
        let _timing = crate::timing::TimingGuard::start(format!("worktree clean name={name}"));
        run_git_expect_success(&dir, &["clean", "-fdx", "-q"]);
    }
    {
        let _timing =
            crate::timing::TimingGuard::start(format!("worktree checkout_branch name={name}"));
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
