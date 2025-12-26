use std::path::Path;
use std::process::Command;

pub fn git_add(repo: &Path, rel: &Path) -> bool {
    Command::new("git")
        .current_dir(repo)
        .args(["add", "-A"])
        .arg(rel)
        .status()
        .ok()
        .is_some_and(|status| status.success())
}

pub fn git_reset(repo: &Path, rel: &Path) -> bool {
    Command::new("git")
        .current_dir(repo)
        .args(["reset", "-q", "HEAD", "--"])
        .arg(rel)
        .status()
        .ok()
        .is_some_and(|status| status.success())
}
