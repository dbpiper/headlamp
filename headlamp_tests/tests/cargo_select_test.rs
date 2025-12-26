use headlamp::cargo_select::{changed_rust_seeds, filter_rust_tests_by_seeds};

fn mk_temp_dir(name: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir()
        .join("headlamp-cargo-select-tests")
        .join(name);
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    base
}

#[test]
fn selects_rust_tests_by_seed() {
    let root = mk_temp_dir("selects_by_seed");
    let src = root.join("src");
    let tests = root.join("tests");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&tests).unwrap();

    let changed_file = src.join("foo.rs");
    std::fs::write(&changed_file, "pub fn foo() {}\n").unwrap();

    let test_file = tests.join("foo_test.rs");
    std::fs::write(&test_file, "use crate::foo;\n").unwrap();

    let seeds = changed_rust_seeds(&root, std::slice::from_ref(&changed_file));
    let kept = filter_rust_tests_by_seeds(std::slice::from_ref(&test_file), &seeds);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0], test_file);
}
