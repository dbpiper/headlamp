use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use indexmap::IndexSet;
use path_slash::PathExt;
use sha1::{Digest, Sha1};
use tempfile::NamedTempFile;
use which::which;

use crate::process::CapturedProcessOutput;
use crate::process::run_command_capture_with_timeout;
use crate::run::RunError;

pub const DEFAULT_TEST_GLOBS: [&str; 2] = [
    "**/*.{test,spec}.{ts,tsx,js,jsx}",
    "tests/**/*.{ts,tsx,js,jsx}",
];

pub const FAST_RELATED_TIMEOUT: Duration = Duration::from_millis(1500);

fn rg_program() -> Option<PathBuf> {
    which("rg").ok()
}

fn rg_related_args(
    repo_root: &Path,
    test_globs: &[&str],
    exclude_globs: &[String],
    seed_terms: &[String],
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--no-messages".to_string(),
        "--line-number".to_string(),
        "--color".to_string(),
        "never".to_string(),
        "--files-with-matches".to_string(),
        "-S".to_string(),
        "-F".to_string(),
        "--no-ignore".to_string(),
    ];

    test_globs.iter().for_each(|glob| {
        args.push("-g".to_string());
        args.push((*glob).to_string());
    });
    exclude_globs.iter().for_each(|exclude| {
        args.push("-g".to_string());
        args.push(format!("!{exclude}"));
    });
    seed_terms.iter().for_each(|seed| {
        args.push("-e".to_string());
        args.push(seed.clone());
    });
    args.push(repo_root.to_string_lossy().to_string());
    args
}

fn display_command(program: &Path, args: &[String]) -> String {
    format!(
        "{} {}",
        program.to_string_lossy(),
        args.iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn run_rg_related(
    rg: &Path,
    repo_root: &Path,
    args: &[String],
    timeout: Duration,
) -> Result<Option<CapturedProcessOutput>, RunError> {
    let display = display_command(rg, args);
    let mut command = Command::new(rg);
    command
        .args(args)
        .current_dir(repo_root)
        .env("CI", "1")
        .env("NODE_ENV", "test");
    match run_command_capture_with_timeout(command, display, timeout) {
        Ok(v) => Ok(Some(v)),
        Err(RunError::TimedOut { .. }) => Ok(None),
        Err(e) => Err(RunError::Io(std::io::Error::other(e.to_string()))),
    }
}

fn abs_posix_existing_paths_from_rg_stdout(repo_root: &Path, stdout: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(stdout);
    let uniq = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| abs_posix_existing_path(repo_root, line))
        .fold(IndexSet::<String>::new(), |mut acc, path| {
            acc.insert(path);
            acc
        });
    let mut out = uniq.into_iter().collect::<Vec<_>>();
    out.sort();
    out
}

fn abs_posix_existing_path(repo_root: &Path, line: &str) -> Option<String> {
    let path = Path::new(line);
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo_root.join(path)
    };
    abs.exists().then_some(abs.to_slash_lossy().to_string())
}

pub fn find_related_tests_fast(
    repo_root: &Path,
    seeds: &[String],
    test_globs: &[&str],
    exclude_globs: &[String],
    timeout: Duration,
) -> Result<Vec<String>, RunError> {
    if seeds.is_empty() {
        return Ok(vec![]);
    }

    let Some(rg) = rg_program() else {
        return Ok(vec![]);
    };

    let seed_terms = build_seed_terms_ts_like(repo_root, seeds);
    if seed_terms.is_empty() {
        return Ok(vec![]);
    }

    let args = rg_related_args(repo_root, test_globs, exclude_globs, &seed_terms);
    let Some(out) = run_rg_related(&rg, repo_root, &args, timeout)? else {
        return Ok(vec![]);
    };
    if !out.status.success() && out.status.code() != Some(1) {
        return Ok(vec![]);
    }
    Ok(abs_posix_existing_paths_from_rg_stdout(
        repo_root,
        &out.stdout,
    ))
}

pub fn cached_related(
    repo_root: &Path,
    selection_key: &str,
    no_cache: bool,
    compute: impl FnOnce() -> Result<Vec<String>, RunError>,
) -> Result<Vec<String>, RunError> {
    if no_cache {
        let computed = compute()?;
        let mut computed_dedup = {
            let mut uniq = IndexSet::<String>::new();
            for p in computed {
                uniq.insert(p);
            }
            uniq.into_iter().collect::<Vec<_>>()
        };
        sort_paths_for_ts_parity(&mut computed_dedup);
        computed_dedup.dedup();
        return Ok(computed_dedup);
    }
    let cache_root = default_cache_root();
    let repo_key = stable_repo_key_hash_12(repo_root);
    let dir = cache_root.join(repo_key);
    let file = dir.join("relevant-tests.json");

    let head = git_short_head(repo_root).unwrap_or_else(|| "nohead".to_string());
    let key = format!("{head}::{selection_key}");

    let mut bag: std::collections::BTreeMap<String, Vec<String>> =
        read_json_map(&file).unwrap_or_default();
    if let Some(hit) = bag.get(&key)
        && hit.iter().all(|p| Path::new(p).exists())
    {
        let mut cached = hit.clone();
        sort_paths_for_ts_parity(&mut cached);
        cached.dedup();
        return Ok(cached);
    };

    let computed = compute()?;
    let mut computed_dedup = {
        let mut uniq = IndexSet::<String>::new();
        for p in computed {
            uniq.insert(p);
        }
        uniq.into_iter().collect::<Vec<_>>()
    };
    sort_paths_for_ts_parity(&mut computed_dedup);
    computed_dedup.dedup();
    bag.insert(key, computed_dedup.clone());
    if std::fs::create_dir_all(&dir).is_ok() {
        let _ = std::fs::remove_file(&file);
        if let Ok(mut tmp) = NamedTempFile::new_in(&dir) {
            use std::io::Write;
            let _ = serde_json::to_writer(&mut tmp, &bag);
            let _ = tmp.flush();
            let _ = tmp.persist(&file);
        }
    }
    Ok(computed_dedup)
}

fn sort_paths_for_ts_parity(paths: &mut [String]) {
    // headlamp-original preserves a stable, reverse-lexicographic ordering for related test paths,
    // which directly affects Jest execution order and therefore stdout ordering.
    paths.sort_by(|left, right| right.cmp(left));
}

pub fn default_cache_root() -> PathBuf {
    let env = std::env::var("HEADLAMP_CACHE_DIR").ok();
    env.map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("headlamp-cache"))
}

fn read_json_map(path: &Path) -> Option<std::collections::BTreeMap<String, Vec<String>>> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<std::collections::BTreeMap<String, Vec<String>>>(&raw).ok()
}

fn sha1_12(text: &str) -> String {
    let mut h = Sha1::new();
    h.update(text.as_bytes());
    let hex = hex::encode(h.finalize());
    hex.chars().take(12).collect()
}

fn stable_repo_key_input(repo_root: &Path) -> String {
    let git_path = repo_root.join(".git");
    let meta = std::fs::metadata(&git_path).ok();
    if meta.as_ref().is_some_and(|m| m.is_dir()) {
        return dunce::canonicalize(&git_path)
            .unwrap_or(git_path)
            .to_string_lossy()
            .to_string();
    }
    if meta.as_ref().is_some_and(|m| m.is_file()) {
        let raw = std::fs::read_to_string(&git_path).unwrap_or_default();
        let prefix = "gitdir:";
        let maybe_gitdir = raw
            .lines()
            .map(str::trim)
            .find(|line| line.to_ascii_lowercase().starts_with(prefix))
            .map(|line| line[prefix.len()..].trim().to_string());
        if let Some(gitdir_text) = maybe_gitdir
            && !gitdir_text.is_empty()
        {
            let gitdir_path = PathBuf::from(gitdir_text);
            let gitdir_abs = if gitdir_path.is_absolute() {
                gitdir_path
            } else {
                repo_root.join(gitdir_path)
            };
            let gitdir_abs = dunce::canonicalize(&gitdir_abs).unwrap_or(gitdir_abs);
            let common = gitdir_abs
                .parent()
                .and_then(|p| p.parent())
                .map(ToOwned::to_owned)
                .unwrap_or(gitdir_abs);
            return common.to_string_lossy().to_string();
        }
    }
    dunce::canonicalize(repo_root)
        .unwrap_or_else(|_| repo_root.to_path_buf())
        .to_string_lossy()
        .to_string()
}

pub fn stable_repo_key_hash_12(repo_root: &Path) -> String {
    sha1_12(&stable_repo_key_input(repo_root))
}

pub fn git_short_head(repo_root: &Path) -> Option<String> {
    let out = crate::git::git_command_in_repo(repo_root)
        .args(["rev-parse", "--short=8", "HEAD"])
        .output()
        .ok()?;
    out.status.success().then(|| {
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .chars()
            .take(8)
            .collect::<String>()
    })
}

pub fn build_seed_terms_ts_like(repo_root: &Path, seeds: &[String]) -> Vec<String> {
    let to_posix = |p: &Path| p.to_slash_lossy().to_string();
    let strip_js_ts_ext_ts_like = |rel: &str| -> String {
        let lower = rel.to_ascii_lowercase();
        let exts_to_strip = [".ts", ".tsx", ".js", ".jsx", ".mjs", ".mts", ".cjs", ".cts"];
        exts_to_strip
            .iter()
            .find(|ext| lower.ends_with(*ext))
            .map(|ext| rel[..rel.len().saturating_sub(ext.len())].to_string())
            .unwrap_or_else(|| rel.to_string())
    };

    let mut uniq: IndexSet<String> = IndexSet::new();
    for candidate in seeds {
        let abs = PathBuf::from(candidate);
        let rel = abs
            .strip_prefix(repo_root)
            .ok()
            .map(to_posix)
            .unwrap_or_else(|| to_posix(&abs));
        let rel = rel.replace('\\', "/");
        let without_ext = strip_js_ts_ext_ts_like(&rel);
        let base = without_ext
            .split('/')
            .next_back()
            .unwrap_or(without_ext.as_str())
            .to_string();
        let segs = without_ext.split('/').collect::<Vec<_>>();
        let tail2 = segs
            .into_iter()
            .rev()
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("/");
        for term in [without_ext, base, tail2] {
            if !term.trim().is_empty() {
                uniq.insert(term);
            }
        }
    }

    uniq.into_iter().collect::<Vec<_>>()
}
