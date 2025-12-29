use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use path_slash::PathExt;

use headlamp_core::args::ParsedArgs;
use headlamp_core::selection::dependency_language::DependencyLanguageId;
use headlamp_core::selection::related_tests::{RelatedTestSelection, select_related_tests};
use headlamp_core::selection::route_index::{discover_tests_for_http_paths, get_route_index};
use headlamp_core::selection::transitive_seed_refine::{
    MaxDepth, filter_tests_by_transitive_seed, max_depth_from_args,
};
use indexmap::IndexSet;

use crate::fast_related::{
    DEFAULT_TEST_GLOBS, FAST_RELATED_TIMEOUT, cached_related, find_related_tests_fast,
};
use crate::git::changed_files;
use crate::jest_discovery::{
    JEST_LIST_TESTS_TIMEOUT, discover_jest_list_tests_cached_with_timeout,
};
use crate::run::RunError;

pub(super) fn selection_paths_abs(
    repo_root: &Path,
    args: &ParsedArgs,
) -> Result<Vec<String>, RunError> {
    let mut selected_abs_paths: IndexSet<String> = IndexSet::new();
    args.selection_paths
        .iter()
        .map(|p| repo_root.join(p))
        .filter(|p| p.exists())
        .map(|p| p.to_slash_lossy().to_string())
        .for_each(|abs| {
            selected_abs_paths.insert(abs);
        });

    args.changed
        .map(|mode| changed_files(repo_root, mode))
        .transpose()?
        .unwrap_or_default()
        .into_iter()
        .filter(|p| p.exists())
        .map(|p| p.to_slash_lossy().to_string())
        .for_each(|abs| {
            selected_abs_paths.insert(abs);
        });

    Ok(selected_abs_paths.into_iter().collect::<Vec<_>>())
}

pub(super) fn exclude_globs_for_selection(exclude_globs: &[String]) -> Vec<String> {
    exclude_globs
        .iter()
        .filter(|glob| glob.as_str() != "**/tests/**")
        .cloned()
        .collect::<Vec<_>>()
}

pub(super) fn looks_like_test_path(candidate_path: &str) -> bool {
    let mut classifier = headlamp_core::project::classify::ProjectClassifier::for_path(
        DependencyLanguageId::TsJs,
        Path::new(candidate_path),
    );
    matches!(
        classifier.classify_abs_path(Path::new(candidate_path)),
        headlamp_core::project::classify::FileKind::Test
            | headlamp_core::project::classify::FileKind::Mixed
    )
}

#[derive(Debug)]
pub(super) struct ComputeRelatedSelectionArgs<'a> {
    pub(super) repo_root: &'a Path,
    pub(super) args: &'a ParsedArgs,
    pub(super) project_configs: &'a [PathBuf],
    pub(super) jest_bin: &'a Path,
    pub(super) discovery_args: &'a [String],
    pub(super) dependency_language: DependencyLanguageId,
    pub(super) selection_key: Option<&'a str>,
    pub(super) selection_is_tests_only: bool,
    pub(super) selection_paths_abs: &'a [String],
    pub(super) production_seeds_abs: &'a [String],
    pub(super) selection_exclude_globs: &'a [String],
}

pub(super) fn compute_related_selection(
    args: ComputeRelatedSelectionArgs<'_>,
) -> Result<RelatedTestSelection, RunError> {
    let ComputeRelatedSelectionArgs {
        repo_root,
        args,
        project_configs,
        jest_bin,
        discovery_args,
        dependency_language,
        selection_key,
        selection_is_tests_only,
        selection_paths_abs,
        production_seeds_abs,
        selection_exclude_globs,
    } = args;

    if selection_is_tests_only {
        return Ok(RelatedTestSelection {
            selected_test_paths_abs: selection_paths_abs.to_vec(),
            rank_by_abs_path: BTreeMap::new(),
        });
    }

    let Some(key) = selection_key else {
        return Ok(RelatedTestSelection {
            selected_test_paths_abs: vec![],
            rank_by_abs_path: BTreeMap::new(),
        });
    };

    cached_related(repo_root, key, args.no_cache, || {
        find_related_tests_fast(
            repo_root,
            production_seeds_abs,
            &DEFAULT_TEST_GLOBS,
            selection_exclude_globs,
            FAST_RELATED_TIMEOUT,
        )
    })
    .map(|fast_tests| {
        if !fast_tests.is_empty() {
            let augmented = augment_with_http_tests(
                repo_root,
                production_seeds_abs,
                selection_exclude_globs,
                fast_tests,
            );
            if args.changed.is_some() || args.changed_depth.is_some() {
                return refine_by_transitive_seed_scan(RefineByTransitiveSeedScanArgs {
                    repo_root,
                    dependency_language,
                    project_configs,
                    jest_bin,
                    discovery_args,
                    production_seeds_abs,
                    candidate_tests_abs: augmented,
                    max_depth: max_depth_from_args(args.changed_depth),
                    no_cache: args.no_cache,
                });
            }
            RelatedTestSelection {
                selected_test_paths_abs: augmented,
                rank_by_abs_path: BTreeMap::new(),
            }
        } else {
            if args.changed.is_some() || args.changed_depth.is_some() {
                return refine_by_transitive_seed_scan(RefineByTransitiveSeedScanArgs {
                    repo_root,
                    dependency_language,
                    project_configs,
                    jest_bin,
                    discovery_args,
                    production_seeds_abs,
                    candidate_tests_abs: vec![],
                    max_depth: max_depth_from_args(args.changed_depth),
                    no_cache: args.no_cache,
                });
            }
            select_related_tests(
                repo_root,
                dependency_language,
                production_seeds_abs,
                selection_exclude_globs,
            )
        }
    })
}

fn augment_with_http_tests(
    repo_root: &Path,
    production_seeds_abs: &[String],
    exclude_globs: &[String],
    related_tests_abs: Vec<String>,
) -> Vec<String> {
    let route_index = get_route_index(repo_root);
    let http_paths = production_seeds_abs
        .iter()
        .flat_map(|seed| route_index.http_routes_for_source(seed))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut route_tests = discover_tests_for_http_paths(repo_root, &http_paths, exclude_globs);
    route_tests.sort();

    let mut combined: IndexSet<String> = IndexSet::new();
    route_tests.into_iter().for_each(|t| {
        combined.insert(t);
    });
    related_tests_abs.into_iter().for_each(|t| {
        combined.insert(t);
    });
    combined.into_iter().collect::<Vec<_>>()
}

#[derive(Debug)]
struct RefineByTransitiveSeedScanArgs<'a> {
    repo_root: &'a Path,
    dependency_language: DependencyLanguageId,
    project_configs: &'a [PathBuf],
    jest_bin: &'a Path,
    discovery_args: &'a [String],
    production_seeds_abs: &'a [String],
    candidate_tests_abs: Vec<String>,
    max_depth: MaxDepth,
    no_cache: bool,
}

fn refine_by_transitive_seed_scan(
    args: RefineByTransitiveSeedScanArgs<'_>,
) -> RelatedTestSelection {
    let RefineByTransitiveSeedScanArgs {
        repo_root,
        dependency_language,
        project_configs,
        jest_bin,
        discovery_args,
        production_seeds_abs,
        candidate_tests_abs,
        max_depth,
        no_cache,
    } = args;
    if !candidate_tests_abs.is_empty() {
        return RelatedTestSelection {
            selected_test_paths_abs: candidate_tests_abs,
            rank_by_abs_path: BTreeMap::new(),
        };
    }

    let all_tests = project_configs
        .iter()
        .filter_map(|cfg_path| {
            let cfg_token = config_token(repo_root, cfg_path);
            let mut list_args = discovery_args.to_vec();
            list_args.extend(["--config".to_string(), cfg_token.clone()]);
            discover_jest_list_tests_cached_with_timeout(
                cfg_path.parent().unwrap_or(repo_root),
                jest_bin,
                &list_args,
                no_cache,
                JEST_LIST_TESTS_TIMEOUT,
            )
            .ok()
        })
        .flatten()
        .collect::<IndexSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let mut kept = filter_tests_by_transitive_seed(
        repo_root,
        dependency_language,
        &all_tests,
        production_seeds_abs,
        max_depth,
    );
    kept.sort();
    let rank_by_abs_path = kept
        .iter()
        .enumerate()
        .fold(BTreeMap::new(), |mut acc, (idx, abs)| {
            acc.insert(normalize_abs_posix(abs), idx as i64);
            acc
        });
    RelatedTestSelection {
        selected_test_paths_abs: kept,
        rank_by_abs_path,
    }
}

fn config_token(repo_root: &Path, cfg: &Path) -> String {
    cfg.strip_prefix(repo_root)
        .ok()
        .and_then(|p| p.to_str())
        .filter(|rel| !rel.starts_with(".."))
        .map(|rel| std::path::Path::new(rel).to_slash_lossy().to_string())
        .unwrap_or_else(|| cfg.to_slash_lossy().to_string())
}

fn normalize_abs_posix(input_path: &str) -> String {
    let posix = input_path.replace('\\', "/");
    if std::path::Path::new(&posix).is_absolute() {
        return posix;
    }
    std::env::current_dir()
        .ok()
        .map(|cwd| {
            cwd.join(&posix)
                .to_string_lossy()
                .to_string()
                .replace('\\', "/")
        })
        .unwrap_or(posix)
}

pub(super) fn compute_directness_rank_base(
    repo_root: &Path,
    selection_paths_abs: &[String],
    exclude_globs: &[String],
    no_cache: bool,
) -> Result<BTreeMap<String, i64>, RunError> {
    let production_seeds = selection_paths_abs
        .iter()
        .filter(|abs| !looks_like_test_path(abs))
        .cloned()
        .collect::<Vec<_>>();
    if production_seeds.is_empty() {
        return Ok(BTreeMap::new());
    }

    let mut selection_key_parts = production_seeds
        .iter()
        .filter_map(|abs| {
            Path::new(abs)
                .strip_prefix(repo_root)
                .ok()
                .map(|p| p.to_slash_lossy().to_string())
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    selection_key_parts.sort();
    let selection_key = selection_key_parts.join("|");

    let related = cached_related(repo_root, &selection_key, no_cache, || {
        find_related_tests_fast(
            repo_root,
            &production_seeds,
            &DEFAULT_TEST_GLOBS,
            exclude_globs,
            FAST_RELATED_TIMEOUT,
        )
    })?;

    let route_index = get_route_index(repo_root);
    let http_paths = production_seeds
        .iter()
        .flat_map(|seed| route_index.http_routes_for_source(seed))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let route_tests = discover_tests_for_http_paths(repo_root, &http_paths, exclude_globs);

    let existing = related
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let augmented = related
        .into_iter()
        .chain(route_tests.into_iter().filter(|t| !existing.contains(t)))
        .collect::<Vec<_>>();

    Ok(augmented
        .into_iter()
        .enumerate()
        .fold(BTreeMap::new(), |mut acc, (index, abs)| {
            acc.insert(normalize_abs_posix(&abs), index as i64);
            acc
        }))
}
