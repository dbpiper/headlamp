use std::path::{Path, PathBuf};

use crate::seed_match::SeedMatcher;

pub fn list_pytest_files(tests_dir: &Path) -> Vec<PathBuf> {
    ignore::WalkBuilder::new(tests_dir)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build()
        .filter_map(Result::ok)
        .filter(|dent| dent.file_type().is_some_and(|t| t.is_file()))
        .map(|dent| dent.into_path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("py"))
        .collect()
}

pub fn changed_seeds(repo_root: &Path, changed: &[PathBuf]) -> Vec<String> {
    changed
        .iter()
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("py"))
        .filter_map(|p| p.strip_prefix(repo_root).ok())
        .flat_map(|rel| {
            use path_slash::PathExt;
            let rel = rel.to_slash_lossy();
            let no_ext = rel.strip_suffix(".py").unwrap_or(&rel);
            let module = no_ext.replace('/', ".");
            let base = no_ext.split('/').next_back().unwrap_or(no_ext).to_string();
            vec![module, base]
        })
        .collect::<Vec<_>>()
}

pub fn filter_tests_by_seeds(tests: &[PathBuf], seeds: &[String]) -> Vec<PathBuf> {
    let Some(matcher) = SeedMatcher::new(seeds) else {
        return vec![];
    };
    tests
        .iter()
        .filter(|&p| matcher.is_match_file_name_or_body(p))
        .cloned()
        .collect()
}
