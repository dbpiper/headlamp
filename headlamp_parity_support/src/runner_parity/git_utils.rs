use std::path::Path;
use std::process::Command;

use crate::hashing::sha1_12;

#[derive(Debug, Clone)]
pub struct WorkingTreeSnapshot {
    pub staged_patch: Vec<u8>,
    pub unstaged_patch: Vec<u8>,
}

pub fn run_git_expect_success(repo: &Path, args: &[&str]) {
    let status = Command::new("git").current_dir(repo).args(args).status();
    assert!(status.is_ok_and(|s| s.success()), "git {:?} failed", args);
}

pub fn git_rev_parse_head(repo: &Path) -> Option<String> {
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

fn git_output_bytes(repo: &Path, args: &[&str]) -> Vec<u8> {
    Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| out.stdout)
        .unwrap_or_default()
}

pub fn repo_state_token(repo: &Path) -> String {
    let head = git_output_trimmed(repo, &["rev-parse", "HEAD"]);
    let staged = git_output_trimmed(repo, &["diff", "--cached", "--name-only"]);
    let unstaged = git_output_trimmed(repo, &["diff", "--name-only"]);
    let payload = format!("head={head}\nstaged={staged}\nunstaged={unstaged}\n");
    sha1_12(&payload)
}

pub fn snapshot_working_tree(repo: &Path) -> WorkingTreeSnapshot {
    let staged_patch = git_output_bytes(repo, &["diff", "--cached"]);
    let unstaged_patch = git_output_bytes(repo, &["diff"]);
    WorkingTreeSnapshot {
        staged_patch,
        unstaged_patch,
    }
}

pub fn apply_working_tree_snapshot(repo: &Path, snapshot: &WorkingTreeSnapshot) {
    if !snapshot.staged_patch.is_empty() {
        // Avoid `git apply --index` strictness (it can fail in worktrees due to index hash
        // mismatches when helper artifacts exist). Apply to the working tree, then stage.
        apply_git_patch_bytes(repo, &["apply"], &snapshot.staged_patch);
        run_git_expect_success(repo, &["add", "-A"]);
    }
    if !snapshot.unstaged_patch.is_empty() {
        apply_git_patch_bytes(repo, &["apply"], &snapshot.unstaged_patch);
    }
}

fn apply_git_patch_bytes(repo: &Path, args: &[&str], patch: &[u8]) {
    let mut child = Command::new("git")
        .current_dir(repo)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn git apply");
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let _ = stdin.write_all(patch);
    }
    let out = child
        .wait_with_output()
        .expect("failed to wait for git apply");
    if !out.status.success() {
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        panic!(
            "git {:?} failed (exit={})\nstdout:\n{}\nstderr:\n{}",
            args,
            out.status.code().unwrap_or(1),
            stdout.trim(),
            stderr.trim()
        );
    }
}
