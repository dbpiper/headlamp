use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use indexmap::IndexSet;
use path_slash::PathExt;
use sha1::{Digest, Sha1};
use tempfile::NamedTempFile;
use which::which;

use git2::Repository;

use crate::process::run_command_capture_with_timeout;
use crate::run::RunError;

pub const DEFAULT_TEST_GLOBS: [&str; 2] = [
    "**/*.{test,spec}.{ts,tsx,js,jsx}",
    "tests/**/*.{ts,tsx,js,jsx}",
];

pub const FAST_RELATED_TIMEOUT: Duration = Duration::from_millis(1500);

#[allow(dead_code)]
fn build_globset(globs: &[&str]) -> globset::GlobSet {
    let mut b = globset::GlobSetBuilder::new();
    for g in globs {
        if let Ok(glob) = globset::Glob::new(g) {
            b.add(glob);
        }
    }
    b.build().unwrap_or_else(|_| globset::GlobSet::empty())
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

    let Ok(rg) = which("rg") else {
        return Ok(vec![]);
    };

    let seed_terms = build_seed_terms_ts_like(repo_root, seeds);
    if seed_terms.is_empty() {
        return Ok(vec![]);
    }

    // Mirror headlamp-original: `rg --no-messages --line-number --color never --files-with-matches -S -F --no-ignore ...`
    let mut args: Vec<String> = vec![
        "--no-messages".to_string(),
        "--line-number".to_string(),
        "--color".to_string(),
        "never".to_string(),
        "--files-with-matches".to_string(),
        "-S".to_string(), // smart-case
        "-F".to_string(), // fixed-strings
        "--no-ignore".to_string(),
    ];

    for glob in test_globs {
        args.push("-g".to_string());
        args.push(glob.to_string());
    }
    for exclude in exclude_globs {
        args.push("-g".to_string());
        args.push(format!("!{exclude}"));
    }
    for seed in &seed_terms {
        args.push("-e".to_string());
        args.push(seed.clone());
    }
    args.push(repo_root.to_string_lossy().to_string());

    let display_command = format!(
        "{} {}",
        rg.to_string_lossy(),
        args.iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    );
    let mut command = Command::new(&rg);
    command
        .args(&args)
        .current_dir(repo_root)
        .env("CI", "1")
        .env("NODE_ENV", "test");
    let out = match run_command_capture_with_timeout(command, display_command, timeout) {
        Ok(v) => v,
        Err(RunError::TimedOut { .. }) => return Ok(vec![]),
        Err(e) => {
            return Err(RunError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            )));
        }
    };
    if !out.status.success() && out.status.code() != Some(1) {
        return Ok(vec![]);
    }

    let text = String::from_utf8_lossy(&out.stdout);
    let looks_like_test = |p: &str| {
        let lower = p.to_ascii_lowercase();
        lower.ends_with(".test.ts")
            || lower.ends_with(".test.tsx")
            || lower.ends_with(".test.js")
            || lower.ends_with(".test.jsx")
            || lower.ends_with(".spec.ts")
            || lower.ends_with(".spec.tsx")
            || lower.ends_with(".spec.js")
            || lower.ends_with(".spec.jsx")
            || lower.contains("/tests/")
            || lower.contains("/test/")
    };

    let mut uniq: IndexSet<String> = IndexSet::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let p = Path::new(line);
        let abs = if p.is_absolute() {
            p.to_path_buf()
        } else {
            repo_root.join(p)
        };
        let abs_posix = abs.to_slash_lossy().to_string();
        if looks_like_test(&abs_posix) && abs.exists() {
            uniq.insert(abs_posix);
        }
    }

    let mut out = uniq.into_iter().collect::<Vec<_>>();
    out.sort();
    Ok(out)
}

pub fn cached_related(
    repo_root: &Path,
    selection_key: &str,
    compute: impl FnOnce() -> Result<Vec<String>, RunError>,
) -> Result<Vec<String>, RunError> {
    let cache_root = default_cache_root();
    let repo_key = sha1_12(&repo_root.to_string_lossy());
    let dir = cache_root.join(repo_key);
    let file = dir.join("relevant-tests.json");

    let head = git_short_head(repo_root).unwrap_or_else(|| "nohead".to_string());
    let key = format!("{head}::{selection_key}");

    let mut bag: std::collections::BTreeMap<String, Vec<String>> =
        read_json_map(&file).unwrap_or_default();
    if let Some(hit) = bag.get(&key) {
        if hit.iter().all(|p| Path::new(p).exists()) {
            return Ok(hit.clone());
        }
    }

    let computed = compute()?;
    let mut computed_dedup = {
        let mut uniq = IndexSet::<String>::new();
        for p in computed {
            uniq.insert(p);
        }
        uniq.into_iter().collect::<Vec<_>>()
    };
    computed_dedup.sort();
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

pub fn git_short_head(repo_root: &Path) -> Option<String> {
    let repo = Repository::discover(repo_root).ok()?;
    let oid = repo.head().ok()?.peel_to_commit().ok()?.id();
    let full = oid.to_string();
    Some(full.chars().take(8).collect())
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
            .map(|p| to_posix(p))
            .unwrap_or_else(|| to_posix(&abs));
        let rel = rel.replace('\\', "/");
        let without_ext = strip_js_ts_ext_ts_like(&rel);
        let base = without_ext
            .split('/')
            .last()
            .map(|s| s.to_string())
            .unwrap_or_else(|| without_ext.clone());
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
