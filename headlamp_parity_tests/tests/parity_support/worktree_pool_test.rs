use std::process::Command;

use super::runner_parity;

fn assert_git_head_exists(repo: &std::path::Path) {
    let out = Command::new("git")
        .current_dir(repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(out.status.success(), "git rev-parse HEAD failed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.trim().is_empty(), "expected HEAD sha");
}

#[test]
fn real_runner_worktree_has_expected_files_and_git() {
    let repo = runner_parity::real_runner_worktree("worktree_pool_smoke");
    assert!(repo.join("src/sum.js").exists());
    assert!(repo.join("src/sum.rs").exists());
    assert!(repo.join("src/sum.py").exists());
    assert!(repo.join("node_modules").exists());
    assert!(repo.join("node_modules/.bin").exists());
    assert_git_head_exists(&repo);
}
