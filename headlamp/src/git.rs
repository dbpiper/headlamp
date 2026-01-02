use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use indexmap::IndexMap;
use path_slash::PathExt;
use regex::Regex;
use semver::Version;
use std::sync::LazyLock;

use headlamp_core::config::ChangedMode;

use crate::run::RunError;

static SEMVER_IN_TAG_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:^|[^0-9A-Za-z])v?(?P<ver>\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?)",
    )
    .unwrap()
});

const EMPTY_TREE_OID: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

pub(crate) fn git_command_in_repo(repo_root: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo_root);
    // Git >= 2.35 can reject repositories with "dubious ownership" (common in CI workspaces).
    // We only need read-only queries, so we allow this repo for this invocation without mutating
    // global config.
    cmd.args(["-c", "safe.directory=*"]);
    cmd
}

pub fn changed_files(repo_root: &Path, mode: ChangedMode) -> Result<Vec<PathBuf>, RunError> {
    let workdir = git_toplevel(repo_root);
    let mut out: Vec<PathBuf> = vec![];

    let mut uncommitted: Vec<PathBuf> = vec![];
    uncommitted.extend(list_staged(&workdir)?);
    uncommitted.extend(list_unstaged_and_untracked(&workdir)?);

    match mode {
        ChangedMode::Staged | ChangedMode::Unstaged | ChangedMode::All => {
            if !uncommitted.is_empty() {
                out.extend(uncommitted);
            }
        }
        ChangedMode::LastCommit => {
            out.extend(list_diff_commits(&workdir, "HEAD^", "HEAD")?);
            if !uncommitted.is_empty() {
                out.extend(uncommitted);
            }
        }
        ChangedMode::Branch => {
            if let Some(base_spec) = merge_base_with_default_branch(&workdir) {
                out.extend(list_diff_commits(&workdir, base_spec.as_str(), "HEAD")?);
            } else {
                out.extend(list_diff_commits(&workdir, "HEAD^", "HEAD")?);
            }
            if !uncommitted.is_empty() {
                out.extend(uncommitted);
            }
        }
        ChangedMode::LastRelease => {
            let Some(base_tag_name) = last_release_baseline_tag_name(&workdir)? else {
                return Ok(vec![]);
            };
            let base_ref = format!("refs/tags/{base_tag_name}");
            out.extend(list_diff_commits(&workdir, base_ref.as_str(), "HEAD")?);
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

fn last_release_baseline_tag_name(repo_root: &Path) -> Result<Option<String>, RunError> {
    let head_oid = git_stdout_trimmed(repo_root, &["rev-parse", "HEAD"])?;
    let tag_names = git_stdout_lines(repo_root, &["tag", "--list"])?;

    let mut stable_reachable: Vec<(String, Version, String)> = vec![];
    for tag_name in tag_names {
        let Some(version) = stable_semver_from_tag_name(&tag_name) else {
            continue;
        };
        let commit_oid =
            git_stdout_trimmed(repo_root, &["rev-list", "-n", "1", tag_name.as_str()])?;
        let reachable = commit_oid == head_oid
            || git_is_ancestor(repo_root, commit_oid.as_str(), head_oid.as_str());
        if reachable {
            stable_reachable.push((tag_name, version, commit_oid));
        }
    }

    if stable_reachable.is_empty() {
        return Ok(None);
    }

    stable_reachable.sort_by(|(_, a, _), (_, b, _)| a.cmp(b));
    let current_on_head = stable_reachable
        .iter()
        .filter(|(_, _, oid)| *oid == head_oid)
        .map(|(_, v, _)| v)
        .max()
        .cloned();

    let selected = match current_on_head {
        Some(current) => stable_reachable
            .into_iter()
            .filter(|(_, v, _)| *v < current)
            .max_by(|(_, a, _), (_, b, _)| a.cmp(b))
            .map(|(name, _, _)| name),
        None => stable_reachable
            .into_iter()
            .max_by(|(_, a, _), (_, b, _)| a.cmp(b))
            .map(|(name, _, _)| name),
    };
    Ok(selected)
}

fn stable_semver_from_tag_name(tag_name: &str) -> Option<Version> {
    let caps = SEMVER_IN_TAG_NAME.captures(tag_name)?;
    let raw = caps.name("ver")?.as_str();
    let version = Version::parse(raw).ok()?;
    version.pre.is_empty().then_some(version)
}

fn list_staged(repo_root: &Path) -> Result<Vec<PathBuf>, RunError> {
    let base = if git_has_head(repo_root) {
        "HEAD"
    } else {
        EMPTY_TREE_OID
    };
    git_stdout_lines(
        repo_root,
        &["diff-index", "--name-only", "--cached", base, "--"],
    )
    .map(|v| v.into_iter().map(|p| repo_root.join(p)).collect())
}

fn list_unstaged_and_untracked(repo_root: &Path) -> Result<Vec<PathBuf>, RunError> {
    let mut out: Vec<PathBuf> = vec![];
    out.extend(
        git_stdout_lines(repo_root, &["diff-files", "--name-only", "--"])?
            .into_iter()
            .map(|p| repo_root.join(p)),
    );
    out.extend(
        git_stdout_lines(repo_root, &["ls-files", "--others", "--exclude-standard"])?
            .into_iter()
            .map(|p| repo_root.join(p)),
    );
    Ok(out)
}

fn list_diff_commits(repo_root: &Path, left: &str, right: &str) -> Result<Vec<PathBuf>, RunError> {
    git_stdout_lines(repo_root, &["diff-tree", "--name-only", "-r", left, right])
        .map(|v| v.into_iter().map(|p| repo_root.join(p)).collect())
}

fn merge_base_with_default_branch(repo_root: &Path) -> Option<String> {
    ["origin/HEAD", "origin/main", "origin/master"]
        .into_iter()
        .find_map(|candidate| {
            git_stdout_trimmed(repo_root, &["merge-base", "HEAD", candidate]).ok()
        })
}

fn git_toplevel(start: &Path) -> PathBuf {
    let out = git_command_in_repo(start)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    out.unwrap_or_else(|| start.to_path_buf())
}

fn git_stdout_lines(repo_root: &Path, args: &[&str]) -> Result<Vec<String>, RunError> {
    let mut cmd = git_command_in_repo(repo_root);
    if args.first().is_some_and(|arg| *arg == "diff") {
        // Guard against user/global gitconfig aliases overriding the built-in `diff` subcommand.
        // In CI we observed failures where an alias expanded `diff` into `diff --no-index`,
        // which rejects flags like `--cached`.
        cmd.args(["-c", "alias.diff=diff"]);
    }
    cmd.args(args);
    let out = cmd.output().map_err(RunError::Io)?;
    if !out.status.success() {
        return Err(RunError::Io(std::io::Error::other(
            String::from_utf8_lossy(&out.stderr).to_string(),
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect())
}

fn git_stdout_trimmed(repo_root: &Path, args: &[&str]) -> Result<String, RunError> {
    git_stdout_lines(repo_root, args).map(|lines| lines.into_iter().next().unwrap_or_default())
}

fn git_has_head(repo_root: &Path) -> bool {
    git_command_in_repo(repo_root)
        .args(["rev-parse", "--verify", "HEAD"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()
        .is_some_and(|s| s.success())
}

fn git_is_ancestor(repo_root: &Path, ancestor: &str, descendant: &str) -> bool {
    git_command_in_repo(repo_root)
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()
        .is_some_and(|s| s.success())
}
