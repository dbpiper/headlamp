use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use path_slash::PathExt;

use crate::selection::dependency_language::{
    DependencyLanguageId, DependencyResolveCache, build_seed_terms, extract_import_specs,
    resolve_import_with_root_cached,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaxDepth(pub u32);

#[derive(Debug, Default)]
struct ResolutionCache {
    memo: HashMap<(PathBuf, String), Option<PathBuf>>,
    dependency_cache: DependencyResolveCache,
}

#[derive(Debug)]
struct MatchTransitivelyCtx<'a> {
    repo_root: &'a Path,
    language: DependencyLanguageId,
    seed_terms: &'a [String],
    max_depth: MaxDepth,
    body_cache: &'a mut HashMap<PathBuf, String>,
    spec_cache: &'a mut HashMap<PathBuf, Vec<String>>,
    resolution_cache: &'a mut ResolutionCache,
    visit_guard: &'a mut HashSet<(PathBuf, u32)>,
}

pub fn filter_tests_by_transitive_seed(
    repo_root: &Path,
    language: DependencyLanguageId,
    candidate_test_paths_abs: &[String],
    production_selection_paths_abs: &[String],
    max_depth: MaxDepth,
) -> Vec<String> {
    let seed_terms = build_seed_terms(language, repo_root, production_selection_paths_abs);
    if seed_terms.is_empty() {
        return vec![];
    }

    let mut body_cache: HashMap<PathBuf, String> = HashMap::new();
    let mut spec_cache: HashMap<PathBuf, Vec<String>> = HashMap::new();
    let mut resolution_cache = ResolutionCache::default();
    let mut visit_guard: HashSet<(PathBuf, u32)> = HashSet::new();

    let mut ctx = MatchTransitivelyCtx {
        repo_root,
        language,
        seed_terms: &seed_terms,
        max_depth,
        body_cache: &mut body_cache,
        spec_cache: &mut spec_cache,
        resolution_cache: &mut resolution_cache,
        visit_guard: &mut visit_guard,
    };

    candidate_test_paths_abs
        .iter()
        .filter_map(|abs| {
            let abs_path = PathBuf::from(abs);
            matches_transitively(&mut ctx, &abs_path, 0).then_some(abs_path)
        })
        .filter_map(|abs| abs.to_str().map(|s| s.replace('\\', "/")))
        .collect::<Vec<_>>()
}

fn matches_transitively(ctx: &mut MatchTransitivelyCtx<'_>, abs_path: &Path, depth: u32) -> bool {
    if depth > ctx.max_depth.0 {
        return false;
    }
    if !ctx.visit_guard.insert((abs_path.to_path_buf(), depth)) {
        return false;
    }

    let body = read_file_cached(abs_path, ctx.body_cache);
    if ctx.seed_terms.iter().any(|seed| body.contains(seed)) {
        return true;
    }

    let specs = import_specs_cached(abs_path, ctx.language, ctx.spec_cache);
    specs.into_iter().any(|spec| {
        let Some(target) = resolve_spec_cached(
            abs_path,
            &spec,
            ctx.repo_root,
            ctx.language,
            ctx.resolution_cache,
        ) else {
            return false;
        };
        let target_body = read_file_cached(&target, ctx.body_cache);
        if ctx.seed_terms.iter().any(|seed| target_body.contains(seed)) {
            return true;
        }
        matches_transitively(ctx, &target, depth.saturating_add(1))
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

fn import_specs_cached(
    abs_path: &Path,
    language: DependencyLanguageId,
    cache: &mut HashMap<PathBuf, Vec<String>>,
) -> Vec<String> {
    if let Some(cached) = cache.get(abs_path) {
        return cached.clone();
    }
    let specs = extract_import_specs(language, abs_path);
    cache.insert(abs_path.to_path_buf(), specs.clone());
    specs
}

fn resolve_spec_cached(
    from_file: &Path,
    spec: &str,
    repo_root: &Path,
    language: DependencyLanguageId,
    cache: &mut ResolutionCache,
) -> Option<PathBuf> {
    let key = (from_file.to_path_buf(), spec.to_string());
    if let Some(cached) = cache.memo.get(&key) {
        return cached.clone();
    }
    let resolved = resolve_import_with_root_cached(
        language,
        from_file,
        spec,
        repo_root,
        &mut cache.dependency_cache,
    );
    cache.memo.insert(key, resolved.clone());
    resolved
}

pub fn max_depth_from_args(changed_depth: Option<u32>) -> MaxDepth {
    MaxDepth(changed_depth.filter(|d| *d > 0).unwrap_or(5))
}

pub fn normalize_abs_posix(path: &Path) -> String {
    path.to_slash_lossy().to_string()
}
