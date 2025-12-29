use std::path::{Path, PathBuf};

fn should_skip_directory_name(dir_name: &str) -> bool {
    matches!(
        dir_name,
        "target" | "npm" | "coverage" | "node_modules" | ".git" | "snapshots"
    )
}

pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("headlamp_tests should be nested under workspace root")
        .to_path_buf()
}

fn is_workspace_rust_file(path: &Path) -> bool {
    let path_string = path.to_string_lossy();
    (path_string.contains("/src/") || path_string.contains("/tests/"))
        && path.extension().and_then(|extension| extension.to_str()) == Some("rs")
}

fn list_rust_source_files_under(root: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            entry
                .file_name()
                .to_str()
                .map(should_skip_directory_name)
                .map(|should_skip| !should_skip)
                .unwrap_or(true)
        })
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("rs"))
        .collect()
}

pub fn list_workspace_rust_files() -> Vec<PathBuf> {
    let root = workspace_root();
    let mut candidate_files = list_rust_source_files_under(&root)
        .into_iter()
        .filter(|path| is_workspace_rust_file(path))
        .collect::<Vec<_>>();
    candidate_files.sort();
    candidate_files
}
