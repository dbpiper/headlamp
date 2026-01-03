use std::path::{Path, PathBuf};
use std::process::Command;

use headlamp_core::config::ChangedMode;

use crate::git::changed_files;

fn git_executable() -> std::path::PathBuf {
    // Some tests temporarily mutate PATH, and Rust tests run in parallel by default. Avoid relying
    // on PATH lookups here by preferring common absolute git locations.
    [
        "/usr/bin/git",
        "/opt/homebrew/bin/git",
        "/usr/local/bin/git",
        "/bin/git",
    ]
    .into_iter()
    .map(std::path::PathBuf::from)
    .find(|p| p.exists())
    .unwrap_or_else(|| std::path::PathBuf::from("git"))
}

fn run_git(repo: &Path, args: &[&str]) {
    let status = Command::new(git_executable())
        .current_dir(repo)
        .args(args)
        .status();
    assert!(status.is_ok_and(|s| s.success()));
}

fn write_file(path: &Path, contents: &str) {
    let parent = path.parent().unwrap();
    std::fs::create_dir_all(parent).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn rel_paths(repo: &Path, paths: Vec<PathBuf>) -> Vec<String> {
    let repo_real = repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf());
    let mut out = paths
        .into_iter()
        .map(|p| p.canonicalize().unwrap_or(p))
        .filter_map(|p| {
            p.strip_prefix(&repo_real)
                .ok()
                .map(|r| r.to_string_lossy().to_string())
        })
        .collect::<Vec<_>>();
    out.sort();
    out
}

fn init_repo_with_two_commits(repo: &Path) {
    run_git(repo, &["init", "-q"]);
    run_git(repo, &["config", "user.email", "headlamp@example.com"]);
    run_git(repo, &["config", "user.name", "Headlamp"]);

    write_file(&repo.join("committed.txt"), "v1\n");
    run_git(repo, &["add", "-A"]);
    run_git(repo, &["commit", "-q", "-m", "init"]);

    write_file(&repo.join("committed.txt"), "v2\n");
    run_git(repo, &["add", "-A"]);
    run_git(repo, &["commit", "-q", "-m", "second"]);
}

#[test]
fn changed_last_commit_also_includes_uncommitted_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo_with_two_commits(repo);

    write_file(&repo.join("unstaged.txt"), "u\n");
    write_file(&repo.join("staged.txt"), "s\n");
    run_git(repo, &["add", "staged.txt"]);

    let rel = rel_paths(repo, changed_files(repo, ChangedMode::LastCommit).unwrap());
    assert!(rel.contains(&"committed.txt".to_string()), "{rel:?}");
    assert!(rel.contains(&"staged.txt".to_string()), "{rel:?}");
    assert!(rel.contains(&"unstaged.txt".to_string()), "{rel:?}");
}

#[test]
fn changed_staged_also_includes_unstaged_when_any_uncommitted_exists() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    init_repo_with_two_commits(repo);

    write_file(&repo.join("unstaged.txt"), "u\n");
    write_file(&repo.join("staged.txt"), "s\n");
    run_git(repo, &["add", "staged.txt"]);

    let rel = rel_paths(repo, changed_files(repo, ChangedMode::Staged).unwrap());
    assert!(rel.contains(&"staged.txt".to_string()));
    assert!(rel.contains(&"unstaged.txt".to_string()));
}
