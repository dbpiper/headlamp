use std::path::Path;
use std::process::Command;

use crate::hashing::sha1_12;

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

pub fn repo_state_token(repo: &Path) -> String {
    let head = git_output_trimmed(repo, &["rev-parse", "HEAD"]);
    let staged = git_output_trimmed(repo, &["diff", "--cached", "--name-only"]);
    let unstaged = git_output_trimmed(repo, &["diff", "--name-only"]);
    let payload = format!("head={head}\nstaged={staged}\nunstaged={unstaged}\n");
    sha1_12(&payload)
}
