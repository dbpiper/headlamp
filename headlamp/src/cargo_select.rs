use std::path::{Path, PathBuf};

use crate::seed_match::SeedMatcher;

pub fn list_rust_test_files(repo_root: &Path) -> Vec<PathBuf> {
    let tests_dir = repo_root.join("tests");
    if !tests_dir.exists() {
        return vec![];
    }
    ignore::WalkBuilder::new(&tests_dir)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build()
        .filter_map(Result::ok)
        .filter(|dent| dent.file_type().map_or(false, |t| t.is_file()))
        .map(|dent| dent.into_path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("rs"))
        .collect()
}

pub fn changed_rust_seeds(repo_root: &Path, changed: &[PathBuf]) -> Vec<String> {
    changed
        .iter()
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("rs"))
        .filter_map(|p| p.strip_prefix(repo_root).ok())
        .flat_map(|rel| {
            use path_slash::PathExt;
            let rel = rel.to_slash_lossy();
            let no_ext = rel.strip_suffix(".rs").unwrap_or(&rel);
            let base = no_ext.split('/').last().unwrap_or(no_ext).to_string();
            let module = no_ext.replace('/', "::");
            vec![module, base]
        })
        .collect::<Vec<_>>()
}

pub fn filter_rust_tests_by_seeds(tests: &[PathBuf], seeds: &[String]) -> Vec<PathBuf> {
    let Some(matcher) = SeedMatcher::new(seeds) else {
        return vec![];
    };
    tests
        .iter()
        .cloned()
        .filter(|p| matcher.is_match_file_name_or_body(p))
        .collect()
}
