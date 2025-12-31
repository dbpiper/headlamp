use std::path::Path;

use tempfile::tempdir;

use super::find_pyproject_toml_root;

fn write_file(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn finds_nearest_pyproject_toml_ancestor() {
    let dir = tempdir().unwrap();
    write_file(
        &dir.path().join("pyproject.toml"),
        b"[project]\nname = \"x\"\n",
    );
    write_file(&dir.path().join("a/b/c/file.txt"), b"not relevant");

    let found = find_pyproject_toml_root(&dir.path().join("a/b/c/file.txt")).unwrap();
    assert_eq!(found, dir.path());
}

#[test]
fn returns_none_when_no_pyproject_toml_exists() {
    let dir = tempdir().unwrap();
    write_file(&dir.path().join("a/b/c/file.txt"), b"not relevant");
    let found = find_pyproject_toml_root(&dir.path().join("a/b/c/file.txt"));
    assert!(found.is_none());
}
