use std::path::Path;

use crate::cargo_select::list_rust_test_files;

fn write_file(path: &Path, contents: &str) {
    let parent = path.parent().unwrap();
    std::fs::create_dir_all(parent).unwrap();
    std::fs::write(path, contents).unwrap();
}

#[test]
fn list_rust_test_files_finds_nested_crate_tests_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();

    write_file(
        &repo.join("crates/a/tests/a_test.rs"),
        "#[test]\nfn t() { assert_eq!(1, 1); }\n",
    );
    write_file(
        &repo.join("crates/b/tests/b_test.rs"),
        "#[test]\nfn t() { assert_eq!(1, 1); }\n",
    );
    write_file(
        &repo.join("tests/root_test.rs"),
        "#[test]\nfn t() { assert_eq!(1, 1); }\n",
    );

    let mut found = list_rust_test_files(repo)
        .into_iter()
        .filter_map(|p| {
            p.strip_prefix(repo)
                .ok()
                .map(|r| r.to_string_lossy().to_string())
        })
        .collect::<Vec<_>>();
    found.sort();

    assert_eq!(
        found,
        vec![
            "crates/a/tests/a_test.rs".to_string(),
            "crates/b/tests/b_test.rs".to_string(),
            "tests/root_test.rs".to_string(),
        ]
    );
}
