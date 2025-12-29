use std::path::{Path, PathBuf};

use git2::{DiffDelta, DiffOptions, Repository, Status, StatusOptions};
use indexmap::IndexMap;
use path_slash::PathExt;

use headlamp_core::config::ChangedMode;

use crate::run::RunError;

pub fn changed_files(repo_root: &Path, mode: ChangedMode) -> Result<Vec<PathBuf>, RunError> {
    let repo = Repository::discover(repo_root)
        .map_err(|e| RunError::Io(std::io::Error::other(e.to_string())))?;
    let workdir = repo.workdir().unwrap_or(repo_root).to_path_buf();

    let mut out: Vec<PathBuf> = vec![];

    let mut uncommitted: Vec<PathBuf> = vec![];
    uncommitted.extend(list_staged(&repo, &workdir)?);
    uncommitted.extend(list_unstaged_and_untracked(&repo, &workdir)?);

    match mode {
        ChangedMode::Staged | ChangedMode::Unstaged | ChangedMode::All => {
            // If there are any uncommitted changes, always include them (staged+unstaged+untracked).
            // If there are none, keep previous behavior: allow selection to be empty.
            if !uncommitted.is_empty() {
                out.extend(uncommitted);
            }
        }
        ChangedMode::LastCommit => {
            out.extend(list_diff_commits(&repo, &workdir, "HEAD^", "HEAD")?);
            if !uncommitted.is_empty() {
                out.extend(uncommitted);
            }
        }
        ChangedMode::Branch => {
            if let Some(base) = merge_base_with_default_branch(&repo) {
                let base_spec = base.to_string();
                out.extend(list_diff_commits(&repo, &workdir, &base_spec, "HEAD")?);
            } else {
                out.extend(list_diff_commits(&repo, &workdir, "HEAD^", "HEAD")?);
            }
            if !uncommitted.is_empty() {
                out.extend(uncommitted);
            }
        }
    }

    let mut kept: IndexMap<String, PathBuf> = IndexMap::new();
    out.into_iter().for_each(|abs| {
        let key = abs.to_slash_lossy().to_string();
        let is_noise = key.contains("/node_modules/")
            || key.contains("/coverage/")
            || key.contains("/.yalc/")
            || key.ends_with("/yalc.lock");
        if !is_noise {
            kept.entry(key).or_insert(abs);
        }
    });
    Ok(kept.into_values().collect())
}

fn list_staged(repo: &Repository, workdir: &Path) -> Result<Vec<PathBuf>, RunError> {
    let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
    let index = repo
        .index()
        .map_err(|e| RunError::Io(std::io::Error::other(e.to_string())))?;

    let mut opts = DiffOptions::new();
    let diff = repo
        .diff_tree_to_index(head_tree.as_ref(), Some(&index), Some(&mut opts))
        .map_err(|e| RunError::Io(std::io::Error::other(e.to_string())))?;
    Ok(diff_paths(&diff, workdir))
}

fn list_unstaged_and_untracked(
    repo: &Repository,
    workdir: &Path,
) -> Result<Vec<PathBuf>, RunError> {
    let mut out: Vec<PathBuf> = vec![];

    // tracked changes in working tree
    let index = repo
        .index()
        .map_err(|e| RunError::Io(std::io::Error::other(e.to_string())))?;
    let mut opts = DiffOptions::new();
    let diff = repo
        .diff_index_to_workdir(Some(&index), Some(&mut opts))
        .map_err(|e| RunError::Io(std::io::Error::other(e.to_string())))?;
    out.extend(diff_paths(&diff, workdir));

    // untracked
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let statuses = repo
        .statuses(Some(&mut opts))
        .map_err(|e| RunError::Io(std::io::Error::other(e.to_string())))?;
    for entry in statuses.iter() {
        if entry.status().contains(Status::WT_NEW)
            && let Some(p) = entry.path()
        {
            out.push(workdir.join(p));
        }
    }

    Ok(out)
}

fn list_diff_commits(
    repo: &Repository,
    workdir: &Path,
    left: &str,
    right: &str,
) -> Result<Vec<PathBuf>, RunError> {
    let left_obj = repo
        .revparse_single(left)
        .map_err(|e| RunError::Io(std::io::Error::other(e.to_string())))?;
    let right_obj = repo
        .revparse_single(right)
        .map_err(|e| RunError::Io(std::io::Error::other(e.to_string())))?;
    let left_tree = left_obj
        .peel_to_tree()
        .map_err(|e| RunError::Io(std::io::Error::other(e.to_string())))?;
    let right_tree = right_obj
        .peel_to_tree()
        .map_err(|e| RunError::Io(std::io::Error::other(e.to_string())))?;
    let mut opts = DiffOptions::new();
    let diff = repo
        .diff_tree_to_tree(Some(&left_tree), Some(&right_tree), Some(&mut opts))
        .map_err(|e| RunError::Io(std::io::Error::other(e.to_string())))?;
    Ok(diff_paths(&diff, workdir))
}

fn diff_paths(diff: &git2::Diff, workdir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = vec![];
    for delta in diff.deltas() {
        push_delta_path(&mut out, delta, workdir);
    }
    out
}

fn push_delta_path(out: &mut Vec<PathBuf>, delta: DiffDelta, workdir: &Path) {
    // Prefer new file path if present, else old
    let p = delta.new_file().path().or_else(|| delta.old_file().path());
    if let Some(p) = p {
        out.push(workdir.join(p));
    }
}

fn merge_base_with_default_branch(repo: &Repository) -> Option<git2::Oid> {
    let head = repo.head().ok()?.peel_to_commit().ok()?.id();
    let default_ref = repo.find_reference("refs/remotes/origin/HEAD").ok()?;
    let target = default_ref.symbolic_target().map(|s| s.to_string());
    let default_spec = target.as_deref().unwrap_or("refs/remotes/origin/HEAD");
    let default_commit = repo
        .revparse_single(default_spec)
        .ok()?
        .peel_to_commit()
        .ok()?
        .id();
    repo.merge_base(head, default_commit).ok()
}
