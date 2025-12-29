use std::borrow::Cow;
use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};
use once_cell::sync::Lazy;

use crate::config::{ChangedMode, CoverageMode, CoverageUi};

use super::types::CoverageDetail;

static TEST_LIKE_GLOBSET: Lazy<GlobSet> = Lazy::new(|| {
    let mut b = GlobSetBuilder::new();
    ["**/tests/**", "**/*.{test,spec}.{ts,tsx,js,jsx}"]
        .into_iter()
        .filter_map(|g| Glob::new(g).ok())
        .for_each(|g| {
            b.add(g);
        });
    b.build().unwrap_or_else(|_| GlobSet::empty())
});

pub(super) fn parse_coverage_ui(raw: &str) -> CoverageUi {
    match raw.trim().to_ascii_lowercase().as_str() {
        "jest" => CoverageUi::Jest,
        _ => CoverageUi::Both,
    }
}

pub(super) fn parse_coverage_detail(raw: &str) -> Option<CoverageDetail> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "all" => Some(CoverageDetail::All),
        "auto" => Some(CoverageDetail::Auto),
        s => s.parse::<u32>().ok().map(CoverageDetail::Lines),
    }
}

pub(super) fn parse_coverage_mode(raw: &str) -> CoverageMode {
    match raw.trim().to_ascii_lowercase().as_str() {
        "compact" => CoverageMode::Compact,
        "auto" => CoverageMode::Auto,
        _ => CoverageMode::Full,
    }
}

pub(super) fn parse_changed_mode_string(raw: &str) -> Option<ChangedMode> {
    Some(match raw.trim().to_ascii_lowercase().as_str() {
        "staged" => ChangedMode::Staged,
        "unstaged" => ChangedMode::Unstaged,
        "branch" => ChangedMode::Branch,
        "lastcommit" | "last_commit" | "last-commit" => ChangedMode::LastCommit,
        "lastrelease" | "last_release" | "last-release" => ChangedMode::LastRelease,
        "all" | "" => ChangedMode::All,
        _ => return None,
    })
}

pub(super) fn changed_mode_to_string(mode: ChangedMode) -> &'static str {
    match mode {
        ChangedMode::All => "all",
        ChangedMode::Staged => "staged",
        ChangedMode::Unstaged => "unstaged",
        ChangedMode::Branch => "branch",
        ChangedMode::LastCommit => "lastCommit",
        ChangedMode::LastRelease => "lastRelease",
    }
}

pub(super) fn depth_for_mode(
    section: &crate::config::ChangedSection,
    mode: ChangedMode,
) -> Option<u32> {
    let key = changed_mode_to_string(mode);
    let v = section.per_mode.get(key)?;
    match v {
        serde_json::Value::Number(n) => n.as_u64().map(|u| u as u32),
        serde_json::Value::Object(map) => {
            map.get("depth").and_then(|d| d.as_u64()).map(|u| u as u32)
        }
        _ => None,
    }
}

pub(super) fn base_flag(t: &str) -> &str {
    t.split_once('=').map(|(k, _)| k).unwrap_or(t)
}

pub(super) fn is_test_like_token(candidate: &str) -> bool {
    let normalized = normalize_token_path_text(candidate);
    let lower = normalized.to_ascii_lowercase();
    TEST_LIKE_GLOBSET.is_match(Path::new(&lower))
}

pub(super) fn is_path_like(candidate: &str) -> bool {
    let normalized = normalize_token_path_text(candidate);
    let normalized = normalized.as_ref();
    let has_sep = normalized.contains('/');
    let ext = Path::new(normalized)
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("");
    has_sep || matches!(ext, "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs")
}

pub(super) fn infer_glob_from_selection_path(path_text: &str) -> String {
    let normalized = normalize_token_path_text(path_text);
    let path_text = normalized.as_ref();
    let p = Path::new(path_text);
    let is_dir = p.extension().is_none();
    if !is_dir {
        return path_text.to_string();
    }
    let base = path_text.trim_end_matches('/').to_string();
    if base.is_empty() {
        "**/*".to_string()
    } else {
        format!("{base}/**/*")
    }
}

pub(super) fn normalize_token_path_text(candidate: &str) -> Cow<'_, str> {
    if candidate.contains('\\') {
        Cow::Owned(candidate.replace('\\', "/"))
    } else {
        Cow::Borrowed(candidate)
    }
}
