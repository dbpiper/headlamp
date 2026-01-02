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

fn with_git_config_global_removed<T>(f: impl FnOnce() -> T) -> T {
    let previous = std::env::var_os("GIT_CONFIG_GLOBAL");
    unsafe { std::env::remove_var("GIT_CONFIG_GLOBAL") };
    let out = f();
    if let Some(value) = previous {
        unsafe { std::env::set_var("GIT_CONFIG_GLOBAL", value) };
    }
    out
}

#[test]
fn changed_all_includes_staged_unstaged_and_untracked() {
    let tmp = temp_repo_dir();
    let repo = tmp.path();
    init_repo(repo);

    commit_file(repo, "a.txt", "a1\n", "a1");
    write_file(&repo.join("b.txt"), "b1\n");
    run_git(repo, &["add", "b.txt"]);
    write_file(&repo.join("a.txt"), "a2\n");
    write_file(&repo.join("c.txt"), "c1\n");

    let rel = rel_paths(repo, changed_files(repo, ChangedMode::All).unwrap());
    assert!(rel.contains(&"a.txt".to_string()), "{rel:?}");
    assert!(rel.contains(&"b.txt".to_string()), "{rel:?}");
    assert!(rel.contains(&"c.txt".to_string()), "{rel:?}");
}

#[test]
fn changed_all_is_robust_to_git_diff_aliases_in_global_config() {
    with_git_config_global_removed(|| {
        let tmp = temp_repo_dir();
        let repo = tmp.path();
        init_repo(repo);

        commit_file(repo, "a.txt", "a1\n", "a1");
        write_file(&repo.join("a.txt"), "a2\n");

        let gitconfig = repo.join("global.gitconfig");
        std::fs::write(&gitconfig, "[alias]\ndiff = diff --no-index\n").unwrap();
        unsafe { std::env::set_var("GIT_CONFIG_GLOBAL", &gitconfig) };

        // If we accidentally respect the alias, `git diff --cached` will break because it turns into
        // `git diff --no-index --cached ...`. We force the built-in `diff` via `-c alias.diff=diff`.
        let rel = rel_paths(repo, changed_files(repo, ChangedMode::All).unwrap());
        assert!(rel.contains(&"a.txt".to_string()), "{rel:?}");
    })
}

#[test]
fn changed_last_release_is_robust_to_git_diff_aliases_in_global_config() {
    with_git_config_global_removed(|| {
        let tmp = temp_repo_dir();
        let repo = tmp.path();
        init_repo(repo);

        commit_file(repo, "a.txt", "a1\n", "a1");
        run_git(repo, &["tag", "v0.1.0"]);
        commit_file(repo, "b.txt", "b1\n", "b1");

        let gitconfig = repo.join("global.gitconfig");
        std::fs::write(&gitconfig, "[alias]\ndiff = diff --no-index\n").unwrap();
        unsafe { std::env::set_var("GIT_CONFIG_GLOBAL", &gitconfig) };

        let rel = rel_paths(repo, changed_files(repo, ChangedMode::LastRelease).unwrap());
        assert!(rel.contains(&"b.txt".to_string()), "{rel:?}");
    })
}

#[test]
fn changed_last_commit_includes_last_commit_and_uncommitted() {
    let tmp = temp_repo_dir();
    let repo = tmp.path();
    init_repo(repo);

    commit_file(repo, "a.txt", "a1\n", "a1");
    commit_file(repo, "b.txt", "b1\n", "b1");
    write_file(&repo.join("c.txt"), "c1\n");

    let rel = rel_paths(repo, changed_files(repo, ChangedMode::LastCommit).unwrap());
    assert!(rel.contains(&"b.txt".to_string()), "{rel:?}");
    assert!(rel.contains(&"c.txt".to_string()), "{rel:?}");
    assert!(!rel.contains(&"a.txt".to_string()), "{rel:?}");
}

#[test]
fn changed_last_release_uses_previous_tag_when_head_is_tagged() {
    let tmp = temp_repo_dir();
    let repo = tmp.path();
    init_repo(repo);

    commit_file(repo, "a.txt", "a1\n", "a1");
    commit_file(repo, "b.txt", "b1\n", "b1");
    run_git(repo, &["tag", "v0.1.0"]);

    commit_file(repo, "c.txt", "c1\n", "c1");
    run_git(repo, &["tag", "v0.2.0"]);

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
    run_git(repo, &["tag", "v0.1.0"]);

    commit_file(repo, "b.txt", "b1\n", "b1");
    run_git(repo, &["tag", "v0.2.0-rc.1"]);

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
