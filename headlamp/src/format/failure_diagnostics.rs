use std::path::{Path, PathBuf};
use std::sync::Arc;

use dashmap::DashMap;
use ignore::WalkBuilder;
use once_cell::sync::Lazy;
use path_slash::PathExt;
use regex::Regex;

static RESOLVED_PATH_CACHE: Lazy<DashMap<String, Option<Arc<str>>>> = Lazy::new(DashMap::new);
static RUST_PANICKED_AT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"panicked at (?:[^:]+: )?([^:\s]+):(\d+):(\d+):?$"#).unwrap());

pub fn resolve_existing_path_best_effort(cwd: &str, raw_file: &str) -> Option<String> {
    let key = format!("{cwd}\n{raw_file}");
    if let Some(hit) = RESOLVED_PATH_CACHE.get(&key) {
        return hit.value().as_ref().map(|s| s.to_string());
    }

    let resolved = resolve_existing_path_best_effort_uncached(cwd, raw_file);
    RESOLVED_PATH_CACHE.insert(key, resolved.as_deref().map(Arc::from));
    resolved
}

fn resolve_existing_path_best_effort_uncached(cwd: &str, raw_file: &str) -> Option<String> {
    if !Path::new(cwd).exists() {
        return None;
    }
    let normalized = normalize_raw_file(raw_file)?;
    let cwd_path = Path::new(cwd);

    let direct = if Path::new(&normalized).is_absolute() {
        PathBuf::from(&normalized)
    } else {
        cwd_path.join(&normalized)
    };
    if direct.exists() {
        return Some(direct.to_slash_lossy().to_string());
    }

    let filename = Path::new(&normalized)
        .file_name()?
        .to_string_lossy()
        .to_string();
    let suffix = format!("/{}", normalized.trim_start_matches('/'));
    let mut matches: Vec<String> = vec![];
    for entry in WalkBuilder::new(cwd_path)
        .standard_filters(true)
        .build()
        .flatten()
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy() == filename)
        {
            let path_s = path.to_slash_lossy().to_string();
            if path_s.ends_with(&suffix) {
                matches.push(path_s);
                if matches.len() > 1 {
                    break;
                }
            }
        }
    }

    (matches.len() == 1).then(|| matches[0].clone())
}

fn normalize_raw_file(raw_file: &str) -> Option<String> {
    let trimmed = raw_file.trim();
    if trimmed.is_empty() {
        return None;
    }
    let trimmed = trimmed.trim_matches(&['"', '\'', '`'][..]);
    let trimmed = trimmed.replace('\\', "/");
    let trimmed = trimmed.strip_prefix("./").unwrap_or(&trimmed);
    Some(trimmed.to_string())
}

pub fn parse_rust_panic_location(line: &str) -> Option<(String, i64, i64)> {
    let trimmed = line.trim();
    let caps = RUST_PANICKED_AT_RE.captures(trimmed)?;
    let file = caps.get(1)?.as_str().to_string();
    let line_number = caps.get(2)?.as_str().parse::<i64>().ok()?;
    let col_number = caps.get(3)?.as_str().parse::<i64>().ok()?;
    Some((file, line_number, col_number))
}
