use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

use globset::GlobSet;
use path_slash::PathExt;

use crate::project::classify::{FileKind, ProjectClassifier};
use crate::selection::dependency_language::{
    DependencyLanguageId, DependencyResolveCache, extract_import_specs, looks_like_source_file,
    resolve_import_with_root_cached,
};
use crate::selection::relevance::augment_rank_with_priority_paths;
use crate::selection::route_index::{discover_tests_for_http_paths, get_route_index};

#[derive(Debug, Clone)]
pub struct RelatedTestSelection {
    pub selected_test_paths_abs: Vec<String>,
    pub rank_by_abs_path: BTreeMap<String, i64>,
}

pub fn select_related_tests(
    repo_root: &Path,
    language: DependencyLanguageId,
    production_selection_paths_abs: &[String],
    exclude_globs: &[String],
) -> RelatedTestSelection {
    let normalized_seeds = production_selection_paths_abs
        .iter()
        .map(|p| normalize_abs_posix(p))
        .collect::<Vec<_>>();

    if normalized_seeds.is_empty() {
        return RelatedTestSelection {
            selected_test_paths_abs: vec![],
            rank_by_abs_path: BTreeMap::new(),
        };
    }

    let graph = build_reverse_import_graph(repo_root, language, exclude_globs);
    let mut classifier = ProjectClassifier::for_path(language, repo_root);
    let (selected_tests, rank_by_abs_path) =
        bfs_related_tests(&graph, &normalized_seeds, &mut classifier);

    let route_augmented_tests =
        discover_route_augmented_tests(repo_root, &normalized_seeds, exclude_globs);
    let augmented_rank =
        augment_rank_with_priority_paths(&rank_by_abs_path, &route_augmented_tests);

    let mut merged_tests = selected_tests
        .into_iter()
        .chain(route_augmented_tests)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let rank_for_sort = augmented_rank.clone();
    merged_tests.sort_by(|left, right| compare_paths_by_rank(&rank_for_sort, left, right));

    RelatedTestSelection {
        selected_test_paths_abs: merged_tests,
        rank_by_abs_path: augmented_rank,
    }
}

fn discover_route_augmented_tests(
    repo_root: &Path,
    production_selection_paths_abs: &[String],
    exclude_globs: &[String],
) -> Vec<String> {
    let route_index = get_route_index(repo_root);
    let routes = production_selection_paths_abs
        .iter()
        .flat_map(|abs| route_index.http_routes_for_source(abs))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    discover_tests_for_http_paths(repo_root, &routes, exclude_globs)
        .into_iter()
        .map(|abs| normalize_abs_posix(&abs))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
}

fn bfs_related_tests(
    importers_by_target_abs: &BTreeMap<String, Vec<String>>,
    seed_paths_abs: &[String],
    classifier: &mut ProjectClassifier,
) -> (Vec<String>, BTreeMap<String, i64>) {
    let mut queue: VecDeque<(String, i64)> = seed_paths_abs
        .iter()
        .cloned()
        .map(|abs| (abs, 0i64))
        .collect::<VecDeque<_>>();
    let mut dist_by_abs: BTreeMap<String, i64> = seed_paths_abs
        .iter()
        .cloned()
        .map(|abs| (abs, 0i64))
        .collect::<BTreeMap<_, _>>();

    while let Some((target, dist)) = queue.pop_front() {
        let importers = importers_by_target_abs
            .get(&target)
            .cloned()
            .unwrap_or_default();
        for importer in importers {
            if dist_by_abs.contains_key(&importer) {
                continue;
            }
            let next = dist.saturating_add(1);
            dist_by_abs.insert(importer.clone(), next);
            queue.push_back((importer, next));
        }
    }

    let rank_by_test_abs: BTreeMap<String, i64> = dist_by_abs
        .iter()
        .filter(|(abs, _)| {
            matches!(
                classifier.classify_abs_path(Path::new(abs)),
                FileKind::Test | FileKind::Mixed
            )
        })
        .map(|(abs, dist)| (abs.clone(), *dist))
        .collect::<BTreeMap<_, _>>();

    let mut selected_test_paths_abs = rank_by_test_abs.keys().cloned().collect::<Vec<_>>();
    let rank_for_sort = rank_by_test_abs.clone();
    selected_test_paths_abs
        .sort_by(|left, right| compare_paths_by_rank(&rank_for_sort, left, right));

    (selected_test_paths_abs, rank_by_test_abs)
}

fn build_reverse_import_graph(
    repo_root: &Path,
    language: DependencyLanguageId,
    exclude_globs: &[String],
) -> BTreeMap<String, Vec<String>> {
    let exclude = build_exclude_globset(exclude_globs);
    let mut importers_by_target_abs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut dependency_cache = DependencyResolveCache::default();

    let walker = ignore::WalkBuilder::new(repo_root)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build();

    for entry in walker {
        let dent = match entry {
            Ok(d) => d,
            Err(_) => continue,
        };
        if !dent.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = dent.path();
        if !looks_like_source_file(language, path) {
            continue;
        }
        let rel = path
            .strip_prefix(repo_root)
            .ok()
            .map(|p| p.to_slash_lossy())
            .unwrap_or_default();
        if rel.is_empty() {
            continue;
        }
        if exclude.is_match(rel.as_ref()) {
            continue;
        }

        let from_abs = normalize_abs_posix(&path.to_slash_lossy());
        let specs = extract_import_specs(language, path);
        for spec in specs {
            let Some(resolved) = resolve_import_with_root_cached(
                language,
                path,
                &spec,
                repo_root,
                &mut dependency_cache,
            ) else {
                continue;
            };
            let target_abs = normalize_abs_posix(&resolved.to_slash_lossy());
            importers_by_target_abs
                .entry(target_abs)
                .or_default()
                .push(from_abs.clone());
        }
    }

    importers_by_target_abs.iter_mut().for_each(|(_, xs)| {
        xs.sort();
        xs.dedup();
    });
    importers_by_target_abs
}

fn build_exclude_globset(exclude_globs: &[String]) -> GlobSet {
    let mut builder = globset::GlobSetBuilder::new();
    for pat in [
        "**/node_modules/**",
        "**/dist/**",
        "**/build/**",
        "**/.next/**",
        "**/coverage/**",
        "**/target/**",
    ] {
        let _ = builder.add(globset::Glob::new(pat).unwrap());
    }
    for pat in exclude_globs {
        if let Ok(glob) = globset::Glob::new(pat) {
            let _ = builder.add(glob);
        }
    }
    builder
        .build()
        .unwrap_or_else(|_| globset::GlobSet::empty())
}

fn normalize_abs_posix(input: &str) -> String {
    let as_path = Path::new(input);
    dunce::canonicalize(as_path)
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| input.to_string())
        .replace('\\', "/")
}

fn compare_paths_by_rank(
    rank_by_abs_path: &BTreeMap<String, i64>,
    left_path: &str,
    right_path: &str,
) -> std::cmp::Ordering {
    let left_abs = normalize_abs_posix(left_path);
    let right_abs = normalize_abs_posix(right_path);
    let left_rank = rank_by_abs_path.get(&left_abs).copied().unwrap_or(i64::MAX);
    let right_rank = rank_by_abs_path
        .get(&right_abs)
        .copied()
        .unwrap_or(i64::MAX);
    left_rank
        .cmp(&right_rank)
        .then_with(|| left_abs.cmp(&right_abs))
}
