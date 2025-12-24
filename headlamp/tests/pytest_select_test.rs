use headlamp::pytest_select::{changed_seeds, filter_tests_by_seeds};

fn mk_temp_dir(name: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir()
        .join("headlamp-pytest-select-tests")
        .join(name);
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    base
}

#[test]
fn selects_tests_by_import_seed() {
    let root = mk_temp_dir("selects_by_import");
    let src = root.join("pkg");
    let tests = root.join("tests");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&tests).unwrap();

    let changed_file = src.join("foo.py");
    std::fs::write(&changed_file, "x=1\n").unwrap();

    let test_file = tests.join("test_foo.py");
    std::fs::write(&test_file, "from pkg.foo import x\n").unwrap();

    let seeds = changed_seeds(&root, std::slice::from_ref(&changed_file));
    let kept = filter_tests_by_seeds(std::slice::from_ref(&test_file), &seeds);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0], test_file);
}
