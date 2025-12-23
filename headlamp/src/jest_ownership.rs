use std::path::Path;

use path_slash::PathExt;

use crate::jest_discovery::{
    JEST_LIST_TESTS_TIMEOUT, discover_jest_list_tests_for_project,
    discover_jest_list_tests_for_project_with_patterns_with_timeout,
};
use crate::run::RunError;

pub fn filter_candidates_for_project(
    repo_root: &Path,
    jest_bin: &Path,
    discovery_args: &[String],
    config_path: &Path,
    candidates_abs: &[String],
) -> Result<Vec<String>, RunError> {
    if candidates_abs.is_empty() {
        return Ok(vec![]);
    }

    let config_dir = config_path.parent().unwrap_or(repo_root);
    let cfg_token = config_token(repo_root, config_path);
    let relative_patterns = candidates_abs
        .iter()
        .map(|abs_or_rel| {
            let as_path = Path::new(abs_or_rel);
            if as_path.is_absolute() {
                pathdiff::diff_paths(as_path, config_dir)
                    .and_then(|p| p.to_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| abs_or_rel.to_string())
            } else {
                abs_or_rel.to_string()
            }
        })
        .map(|p| p.replace('\\', "/"))
        .collect::<Vec<_>>();

    let attempt = match discover_jest_list_tests_for_project_with_patterns_with_timeout(
        repo_root,
        jest_bin,
        discovery_args,
        &cfg_token,
        config_dir,
        &relative_patterns,
        JEST_LIST_TESTS_TIMEOUT,
    ) {
        Ok(v) => v,
        Err(RunError::TimedOut { .. }) => vec![],
        Err(_e) => vec![],
    };
    let attempt_norm = attempt
        .into_iter()
        .map(normalize_abs_posix)
        .collect::<Vec<_>>();
    if !attempt_norm.is_empty() {
        return Ok(dedup_sorted(attempt_norm));
    }

    let all_in_project = match discover_jest_list_tests_for_project(
        repo_root,
        jest_bin,
        discovery_args,
        &cfg_token,
        config_dir,
    ) {
        Ok(v) => v,
        Err(RunError::TimedOut { .. }) => vec![],
        Err(_e) => vec![],
    }
    .into_iter()
    .map(|p| p.replace('\\', "/"))
    .collect::<Vec<_>>();

    let by_suffix = all_in_project
        .into_iter()
        .filter(|abs| {
            relative_patterns.iter().any(|rel| {
                abs.ends_with(&format!("/{rel}"))
                    || abs.ends_with(rel)
                    || rel
                        .split('/')
                        .last()
                        .map(|base| abs.ends_with(&format!("/{base}")) || abs.ends_with(base))
                        .unwrap_or(false)
            })
        })
        .map(normalize_abs_posix)
        .collect::<Vec<_>>();
    if !by_suffix.is_empty() {
        return Ok(dedup_sorted(by_suffix));
    }

    let absolute_from_relative = relative_patterns
        .into_iter()
        .map(|rel| config_dir.join(rel).to_slash_lossy().to_string())
        .map(|abs| abs.replace('\\', "/"))
        .collect::<Vec<_>>();
    Ok(dedup_sorted(absolute_from_relative))
}

fn normalize_abs_posix(input: String) -> String {
    dunce::canonicalize(Path::new(&input))
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or(input)
        .replace('\\', "/")
}

fn dedup_sorted(mut items: Vec<String>) -> Vec<String> {
    items.sort();
    items.dedup();
    items
}

fn config_token(repo_root: &Path, cfg: &Path) -> String {
    cfg.strip_prefix(repo_root)
        .ok()
        .and_then(|p| p.to_str())
        .filter(|rel| !rel.starts_with(".."))
        .map(|rel| std::path::Path::new(rel).to_slash_lossy().to_string())
        .unwrap_or_else(|| cfg.to_slash_lossy().to_string())
}
