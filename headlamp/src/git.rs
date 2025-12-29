use std::path::{Path, PathBuf};

use git2::{DiffDelta, DiffOptions, Repository, Status, StatusOptions};
use indexmap::IndexMap;
use once_cell::sync::Lazy;
use path_slash::PathExt;
use regex::Regex;
use semver::Version;

use headlamp_core::config::ChangedMode;

use crate::run::RunError;

static SEMVER_IN_TAG_NAME: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?:^|[^0-9A-Za-z])v?(?P<ver>\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?)",
    )
    .unwrap()
});

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
        ChangedMode::LastRelease => {
            let Some(base_tag_name) = last_release_baseline_tag_name(&repo) else {
                // fallback-all: return empty seed list (so the runner will run everything)
                // and do not include uncommitted changes (avoid narrowing selection).
                return Ok(vec![]);
            };
            let base_ref = format!("refs/tags/{base_tag_name}");
            out.extend(list_diff_commits(&repo, &workdir, &base_ref, "HEAD")?);
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

fn last_release_baseline_tag_name(repo: &Repository) -> Option<String> {
    let head_oid = repo.head().ok()?.peel_to_commit().ok()?.id();
    let mut stable_reachable: Vec<(String, Version, git2::Oid)> = repo
        .tag_names(None)
        .ok()?
        .iter()
        .filter_map(|t| t.map(|s| s.to_string()))
        .filter_map(|tag_name| {
            let version = stable_semver_from_tag_name(&tag_name)?;
            let commit_oid = tag_commit_oid(repo, &tag_name)?;
            let reachable = commit_oid == head_oid
                || repo
                    .graph_descendant_of(head_oid, commit_oid)
                    .ok()
                    .unwrap_or(false);
            reachable.then_some((tag_name, version, commit_oid))
        })
        .collect();
    if stable_reachable.is_empty() {
        return None;
    }

    stable_reachable.sort_by(|(_, a, _), (_, b, _)| a.cmp(b));
    let current_on_head = stable_reachable
        .iter()
        .filter(|(_, _, oid)| *oid == head_oid)
        .map(|(_, v, _)| v)
        .max()
        .cloned();

    match current_on_head {
        Some(current) => stable_reachable
            .into_iter()
            .filter(|(_, v, _)| *v < current)
            .max_by(|(_, a, _), (_, b, _)| a.cmp(b))
            .map(|(name, _, _)| name),
        None => stable_reachable
            .into_iter()
            .max_by(|(_, a, _), (_, b, _)| a.cmp(b))
            .map(|(name, _, _)| name),
    }
}

fn stable_semver_from_tag_name(tag_name: &str) -> Option<Version> {
    let caps = SEMVER_IN_TAG_NAME.captures(tag_name)?;
    let raw = caps.name("ver")?.as_str();
    let version = Version::parse(raw).ok()?;
    version.pre.is_empty().then_some(version)
}

fn tag_commit_oid(repo: &Repository, tag_name: &str) -> Option<git2::Oid> {
    let tag_ref = format!("refs/tags/{tag_name}");
    repo.revparse_single(&tag_ref)
        .ok()?
        .peel_to_commit()
        .ok()
        .map(|c| c.id())
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
