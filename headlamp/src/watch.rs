use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Duration;

use ignore::WalkBuilder;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchDecision {
    Rerun,
    Continue,
}

pub fn run_polling_watch_loop(
    repo_root: &Path,
    poll_interval: Duration,
    verbose: bool,
    mut run_once: impl FnMut() -> i32,
) -> i32 {
    let _initial_exit_code = run_once();
    let mut last_fingerprint = compute_repo_fingerprint(repo_root);
    loop {
        std::thread::sleep(poll_interval);
        match watch_decision(repo_root, &mut last_fingerprint) {
            WatchDecision::Continue => {}
            WatchDecision::Rerun => {
                if verbose {
                    eprintln!("headlamp: watch detected changes, re-running");
                }
                let _ = run_once();
            }
        }
    }
}

fn watch_decision(repo_root: &Path, last_fingerprint: &mut u64) -> WatchDecision {
    let next = compute_repo_fingerprint(repo_root);
    if next == *last_fingerprint {
        WatchDecision::Continue
    } else {
        *last_fingerprint = next;
        WatchDecision::Rerun
    }
}

fn compute_repo_fingerprint(repo_root: &Path) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    walk_watch_paths(repo_root).for_each(|candidate| {
        if let Ok(metadata) = std::fs::metadata(&candidate) {
            let rel = candidate
                .strip_prefix(repo_root)
                .unwrap_or(&candidate)
                .to_string_lossy()
                .to_string();
            rel.hash(&mut hasher);
            metadata.len().hash(&mut hasher);
            if let Ok(modified) = metadata.modified()
                && let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH)
            {
                duration.as_nanos().hash(&mut hasher);
            }
        }
    });
    hasher.finish()
}

fn walk_watch_paths(repo_root: &Path) -> impl Iterator<Item = PathBuf> {
    WalkBuilder::new(repo_root)
        .standard_filters(true)
        .hidden(false)
        .follow_links(false)
        .filter_entry(|entry| !is_ignored_path(entry.path()))
        .build()
        .filter_map(|result| result.ok())
        .map(|entry| entry.into_path())
        .filter(|path| path.is_file())
}

fn is_ignored_path(candidate: &Path) -> bool {
    let ignored_components = [
        ".git",
        ".yalc",
        "node_modules",
        "target",
        "dist",
        "build",
        "coverage",
    ];
    candidate
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .any(|segment| ignored_components.contains(&segment))
}
