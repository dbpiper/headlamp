use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use path_slash::PathExt;

use crate::selection::import_extract::extract_import_specs;
use crate::selection::import_resolve::resolve_import_with_root;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaxDepth(pub u32);

pub fn filter_tests_by_transitive_seed(
    repo_root: &Path,
    candidate_test_paths_abs: &[String],
    production_selection_paths_abs: &[String],
    max_depth: MaxDepth,
) -> Vec<String> {
    let seed_terms = build_seed_terms(repo_root, production_selection_paths_abs);
    if seed_terms.is_empty() {
        return vec![];
    }

    let mut body_cache: HashMap<PathBuf, String> = HashMap::new();
    let mut spec_cache: HashMap<PathBuf, Vec<String>> = HashMap::new();
    let mut resolution_cache: HashMap<(PathBuf, String), Option<PathBuf>> = HashMap::new();
    let mut visit_guard: HashSet<(PathBuf, u32)> = HashSet::new();

    candidate_test_paths_abs
        .iter()
        .map(|abs| PathBuf::from(abs))
        .filter(|abs| {
            matches_transitively(
                abs,
                repo_root,
                &seed_terms,
                max_depth,
                0,
                &mut body_cache,
                &mut spec_cache,
                &mut resolution_cache,
                &mut visit_guard,
            )
        })
        .filter_map(|abs| abs.to_str().map(|s| s.replace('\\', "/")))
        .collect::<Vec<_>>()
}

fn build_seed_terms(repo_root: &Path, production_selection_paths_abs: &[String]) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    production_selection_paths_abs.iter().for_each(|abs| {
        let abs_path = PathBuf::from(abs);
        let Ok(rel) = abs_path.strip_prefix(repo_root) else {
            return;
        };
        let Some(rel_text) = rel.to_str().map(|s| s.replace('\\', "/")) else {
            return;
        };
        let without_ext = strip_ts_like_extension(&rel_text);
        if without_ext.is_empty() {
            return;
        }
        let base = Path::new(&without_ext)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let last_two = last_two_segments(&without_ext);
        [without_ext, base, last_two]
            .into_iter()
            .filter(|s| !s.is_empty())
            .for_each(|s| {
                out.insert(s);
            });
    });
    out.into_iter().collect()
}

fn strip_ts_like_extension(input: &str) -> String {
    let lowered = input.to_lowercase();
    for ext in [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".cts"] {
        if lowered.ends_with(ext) {
            return input[..input.len().saturating_sub(ext.len())].to_string();
        }
    }
    input.to_string()
}

fn last_two_segments(path_text: &str) -> String {
    let segs = path_text
        .split('/')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if segs.len() < 2 {
        return String::new();
    }
    format!("{}/{}", segs[segs.len() - 2], segs[segs.len() - 1])
}

fn matches_transitively(
    abs_path: &Path,
    repo_root: &Path,
    seed_terms: &[String],
    max_depth: MaxDepth,
    depth: u32,
    body_cache: &mut HashMap<PathBuf, String>,
    spec_cache: &mut HashMap<PathBuf, Vec<String>>,
    resolution_cache: &mut HashMap<(PathBuf, String), Option<PathBuf>>,
    visit_guard: &mut HashSet<(PathBuf, u32)>,
) -> bool {
    if depth > max_depth.0 {
        return false;
    }
    if !visit_guard.insert((abs_path.to_path_buf(), depth)) {
        return false;
    }

    let body = read_file_cached(abs_path, body_cache);
    if seed_terms.iter().any(|seed| body.contains(seed)) {
        return true;
    }

    let specs = import_specs_cached(abs_path, spec_cache);
    specs.into_iter().any(|spec| {
        let Some(target) = resolve_spec_cached(abs_path, &spec, repo_root, resolution_cache) else {
            return false;
        };
        let target_body = read_file_cached(&target, body_cache);
        if seed_terms.iter().any(|seed| target_body.contains(seed)) {
            return true;
        }
        matches_transitively(
            &target,
            repo_root,
            seed_terms,
            max_depth,
            depth.saturating_add(1),
            body_cache,
            spec_cache,
            resolution_cache,
            visit_guard,
        )
    })
}

fn read_file_cached(abs_path: &Path, cache: &mut HashMap<PathBuf, String>) -> String {
    if let Some(cached) = cache.get(abs_path) {
        return cached.clone();
    }
    let content = std::fs::read_to_string(abs_path).unwrap_or_default();
    cache.insert(abs_path.to_path_buf(), content.clone());
    content
}

fn import_specs_cached(abs_path: &Path, cache: &mut HashMap<PathBuf, Vec<String>>) -> Vec<String> {
    if let Some(cached) = cache.get(abs_path) {
        return cached.clone();
    }
    let specs = extract_import_specs(abs_path);
    cache.insert(abs_path.to_path_buf(), specs.clone());
    specs
}

fn resolve_spec_cached(
    from_file: &Path,
    spec: &str,
    repo_root: &Path,
    cache: &mut HashMap<(PathBuf, String), Option<PathBuf>>,
) -> Option<PathBuf> {
    let key = (from_file.to_path_buf(), spec.to_string());
    if let Some(cached) = cache.get(&key) {
        return cached.clone();
    }
    let resolved = resolve_import_with_root(from_file, spec, repo_root);
    cache.insert(key, resolved.clone());
    resolved
}

pub fn max_depth_from_args(changed_depth: Option<u32>) -> MaxDepth {
    MaxDepth(changed_depth.filter(|d| *d > 0).unwrap_or(5))
}

pub fn normalize_abs_posix(path: &Path) -> String {
    path.to_slash_lossy().to_string()
}
