use std::path::Path;
use std::process::Command;

pub fn git_init(repo: &Path) {
    let _ = std::fs::remove_file(repo.join(".git/index.lock"));
    let _ = std::fs::remove_file(repo.join(".git/config.lock"));
    if !repo.join(".git").exists() {
        let _ = Command::new("git")
            .current_dir(repo)
            .args(["init", "-q"])
            .status();
    }
    let _ = Command::new("git")
        .current_dir(repo)
        .args(["config", "user.email", "parity@example.com"])
        .status();
    let _ = Command::new("git")
        .current_dir(repo)
        .args(["config", "user.name", "Parity"])
        .status();
}

pub fn git_commit_all(repo: &Path, message: &str) {
    let _ = Command::new("git")
        .current_dir(repo)
        .args(["add", "-A"])
        .status();
    let _ = Command::new("git")
        .current_dir(repo)
        .args(["commit", "-q", "-m", message])
        .status();
}
