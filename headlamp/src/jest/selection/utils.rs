use std::path::Path;

use path_slash::PathExt;

pub(super) fn config_token(repo_root: &Path, cfg: &Path) -> String {
    cfg.strip_prefix(repo_root)
        .ok()
        .and_then(|p| p.to_str())
        .filter(|rel| !rel.starts_with(".."))
        .map(|rel| std::path::Path::new(rel).to_slash_lossy().to_string())
        .unwrap_or_else(|| cfg.to_slash_lossy().to_string())
}

pub(super) fn normalize_abs_posix(input_path: &str) -> String {
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

pub(super) fn selection_key_for_production_seeds(
    repo_root: &Path,
    production_seeds: &[String],
) -> String {
    let mut parts = production_seeds
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
    parts.sort();
    parts.join("|")
}

pub(super) fn discover_route_tests_for_production_seeds(
    repo_root: &Path,
    production_seeds: &[String],
    exclude_globs: &[String],
) -> Vec<String> {
    let route_index = headlamp_core::selection::route_index::get_route_index(repo_root);
    let http_paths = production_seeds
        .iter()
        .flat_map(|seed| route_index.http_routes_for_source(seed))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    headlamp_core::selection::route_index::discover_tests_for_http_paths(
        repo_root,
        &http_paths,
        exclude_globs,
    )
}

pub(super) fn append_missing_paths_preserving_order(
    base: Vec<String>,
    extras: Vec<String>,
) -> Vec<String> {
    let existing = base
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    base.into_iter()
        .chain(extras.into_iter().filter(|t| !existing.contains(t)))
        .collect::<Vec<_>>()
}
