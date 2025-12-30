use std::path::{Path, PathBuf};
use std::process::Command;

use headlamp::config::ChangedMode;
use headlamp::git::changed_files;

fn temp_repo_dir() -> tempfile::TempDir {
    let base = std::env::current_dir()
        .unwrap()
        .join("target")
        .join("tmp")
        .join("headlamp-tests");
    std::fs::create_dir_all(&base).unwrap();
    tempfile::Builder::new()
        .prefix("changed-last-release-")
        .tempdir_in(base)
        .unwrap()
}

fn run_git(repo: &Path, args: &[&str]) {
    let status = Command::new("git").current_dir(repo).args(args).status();
    assert!(status.is_ok_and(|s| s.success()), "git {:?} failed", args);
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

fn init_repo(repo: &Path) {
    run_git(repo, &["init", "-q"]);
    run_git(repo, &["config", "user.email", "headlamp@example.com"]);
    run_git(repo, &["config", "user.name", "Headlamp"]);
}

fn commit_file(repo: &Path, rel: &str, contents: &str, message: &str) {
    write_file(&repo.join(rel), contents);
    run_git(repo, &["add", "-A"]);
    run_git(repo, &["commit", "-q", "-m", message]);
}

#[test]
fn changed_last_release_uses_previous_tag_when_head_is_tagged() {
    let tmp = temp_repo_dir();
    let repo = tmp.path();
    init_repo(repo);

    commit_file(repo, "a.txt", "a1\n", "a1");
    commit_file(repo, "b.txt", "b1\n", "b1");
    run_git(repo, &["tag", "v0.1.37"]);

    commit_file(repo, "c.txt", "c1\n", "c1");
    run_git(repo, &["tag", "v0.1.38"]);

    let rel = rel_paths(repo, changed_files(repo, ChangedMode::LastRelease).unwrap());
    assert!(rel.contains(&"c.txt".to_string()), "{rel:?}");
    assert!(!rel.contains(&"b.txt".to_string()), "{rel:?}");
}

#[test]
fn changed_last_release_uses_latest_reachable_release_when_head_is_not_tagged() {
    let tmp = temp_repo_dir();
    let repo = tmp.path();
    init_repo(repo);

    commit_file(repo, "a.txt", "a1\n", "a1");
    run_git(repo, &["tag", "v0.1.0"]);

    commit_file(repo, "b.txt", "b1\n", "b1");
    run_git(repo, &["tag", "release/v0.2.0"]);

    commit_file(repo, "c.txt", "c1\n", "c1");

    let rel = rel_paths(repo, changed_files(repo, ChangedMode::LastRelease).unwrap());
    assert!(rel.contains(&"c.txt".to_string()), "{rel:?}");
    assert!(!rel.contains(&"b.txt".to_string()), "{rel:?}");
}

#[test]
fn changed_last_release_ignores_prerelease_tags() {
    let tmp = temp_repo_dir();
    let repo = tmp.path();
    init_repo(repo);

    commit_file(repo, "a.txt", "a1\n", "a1");
    run_git(repo, &["tag", "v0.1.37"]);

    commit_file(repo, "b.txt", "b1\n", "b1");
    run_git(repo, &["tag", "v0.1.37-rc.1"]);

    commit_file(repo, "c.txt", "c1\n", "c1");

    let rel = rel_paths(repo, changed_files(repo, ChangedMode::LastRelease).unwrap());
    assert!(rel.contains(&"b.txt".to_string()), "{rel:?}");
    assert!(rel.contains(&"c.txt".to_string()), "{rel:?}");
}

#[test]
fn changed_last_release_falls_back_to_all_when_no_stable_release_tags_exist() {
    let tmp = temp_repo_dir();
    let repo = tmp.path();
    init_repo(repo);

    commit_file(repo, "a.txt", "a1\n", "a1");
    run_git(repo, &["tag", "v0.1.0-rc.1"]);

    commit_file(repo, "b.txt", "b1\n", "b1");

    let rel = rel_paths(repo, changed_files(repo, ChangedMode::LastRelease).unwrap());
    assert!(rel.is_empty(), "{rel:?}");
}
