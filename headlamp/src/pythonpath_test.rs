use std::path::Path;

use tempfile::TempDir;

use crate::pythonpath::python_import_roots;

fn make_temp_repo_root() -> TempDir {
    tempfile::tempdir().expect("tempdir")
}

fn write_text_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create_dir_all");
    }
    std::fs::write(path, contents).expect("write");
}

#[test]
fn python_import_roots_flat_layout_includes_only_repo_root() {
    let repo_root = make_temp_repo_root();

    let roots = python_import_roots(repo_root.path());

    assert_eq!(roots, vec![repo_root.path().to_path_buf()]);
}

#[test]
fn python_import_roots_src_layout_includes_repo_root_and_src() {
    let repo_root = make_temp_repo_root();
    std::fs::create_dir_all(repo_root.path().join("src")).expect("create src/");

    let roots = python_import_roots(repo_root.path());

    assert_eq!(
        roots,
        vec![repo_root.path().to_path_buf(), repo_root.path().join("src"),]
    );
}

#[test]
fn python_import_roots_includes_package_dir_from_pyproject_setuptools() {
    let repo_root = make_temp_repo_root();
    std::fs::create_dir_all(repo_root.path().join("python")).expect("create python/");

    write_text_file(
        &repo_root.path().join("pyproject.toml"),
        r#"
[tool.setuptools]
package-dir = { "" = "python" }

[tool.setuptools.packages.find]
where = ["python"]
"#,
    );

    let roots = python_import_roots(repo_root.path());

    assert_eq!(
        roots,
        vec![
            repo_root.path().to_path_buf(),
            repo_root.path().join("python")
        ]
    );
}

#[test]
fn python_import_roots_includes_packages_from_pyproject_poetry() {
    let repo_root = make_temp_repo_root();
    std::fs::create_dir_all(repo_root.path().join("pkgsrc")).expect("create pkgsrc/");

    write_text_file(
        &repo_root.path().join("pyproject.toml"),
        r#"
[tool.poetry]
name = "x"
version = "0.1.0"

[[tool.poetry.packages]]
include = "mypkg"
from = "pkgsrc"
"#,
    );

    let roots = python_import_roots(repo_root.path());

    assert_eq!(
        roots,
        vec![
            repo_root.path().to_path_buf(),
            repo_root.path().join("pkgsrc")
        ]
    );
}
