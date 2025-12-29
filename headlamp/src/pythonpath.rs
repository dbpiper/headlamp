use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub(crate) fn prepend_path_env(prefix: &str, existing: Option<String>) -> String {
    let sep = if cfg!(windows) { ";" } else { ":" };
    existing
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| format!("{prefix}{sep}{s}"))
        .unwrap_or_else(|| prefix.to_string())
}

pub(crate) fn python_import_roots(repo_root: &Path) -> Vec<PathBuf> {
    let base = vec![repo_root.to_path_buf()];

    let extra_roots_in_prepend_call_order = read_pyproject_python_roots(repo_root)
        .into_iter()
        .chain(existing_src_layout_root(repo_root))
        .filter(|p| is_dir(p))
        .filter(|p| p != repo_root)
        .collect::<Vec<_>>();

    base.into_iter()
        .chain(dedup_keep_last(extra_roots_in_prepend_call_order))
        .collect()
}

pub(crate) fn build_pytest_pythonpath(
    repo_root: &Path,
    extra_prefixes_in_prepend_call_order: &[&Path],
    existing_pythonpath: Option<String>,
) -> String {
    let repo_prefixes = python_import_roots(repo_root)
        .into_iter()
        .map(|p| p.to_string_lossy().to_string());

    let all_prefixes_in_prepend_call_order = extra_prefixes_in_prepend_call_order
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .chain(repo_prefixes);

    all_prefixes_in_prepend_call_order
        .fold(existing_pythonpath, |acc, prefix| {
            Some(prepend_path_env(&prefix, acc))
        })
        .unwrap_or_default()
}

fn is_dir(path: &Path) -> bool {
    path.is_dir()
}

fn existing_src_layout_root(repo_root: &Path) -> Option<PathBuf> {
    let src = repo_root.join("src");
    src.is_dir().then_some(src)
}

fn dedup_keep_last(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut reversed = paths
        .into_iter()
        .rev()
        .filter(|p| seen.insert(p.clone()))
        .collect::<Vec<_>>();
    reversed.reverse();
    reversed
}

fn read_pyproject_python_roots(repo_root: &Path) -> Vec<PathBuf> {
    let pyproject = repo_root.join("pyproject.toml");
    let raw = std::fs::read_to_string(&pyproject).ok();
    let parsed = raw
        .as_deref()
        .and_then(|text| toml::from_str::<toml::Value>(text).ok());

    let candidate_dirs = parsed
        .as_ref()
        .map(extract_pyproject_candidate_dirs)
        .unwrap_or_default();

    candidate_dirs
        .into_iter()
        .map(|dir| resolve_repo_relative_path(repo_root, &dir))
        .collect()
}

fn resolve_repo_relative_path(repo_root: &Path, path_text: &str) -> PathBuf {
    let candidate = PathBuf::from(path_text);
    if candidate.is_absolute() {
        candidate
    } else {
        repo_root.join(path_text)
    }
}

fn extract_pyproject_candidate_dirs(pyproject: &toml::Value) -> Vec<String> {
    let setuptools_package_dir_values = pyproject
        .get("tool")
        .and_then(|t| t.get("setuptools"))
        .and_then(|s| s.get("package-dir"))
        .and_then(|v| v.as_table())
        .into_iter()
        .flat_map(|table| table.values())
        .filter_map(|v| v.as_str())
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

    let setuptools_find_where_values = pyproject
        .get("tool")
        .and_then(|t| t.get("setuptools"))
        .and_then(|s| s.get("packages"))
        .and_then(|p| p.get("find"))
        .and_then(|f| f.get("where"))
        .and_then(|w| w.as_array())
        .into_iter()
        .flat_map(|arr| arr.iter())
        .filter_map(|v| v.as_str())
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

    let poetry_packages_from_values = pyproject
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .and_then(|p| p.get("packages"))
        .and_then(|pkgs| pkgs.as_array())
        .into_iter()
        .flat_map(|arr| arr.iter())
        .filter_map(|pkg| pkg.get("from").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

    setuptools_package_dir_values
        .into_iter()
        .chain(setuptools_find_where_values)
        .chain(poetry_packages_from_values)
        .collect()
}
