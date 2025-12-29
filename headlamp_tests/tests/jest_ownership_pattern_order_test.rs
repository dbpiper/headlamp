use std::path::{Path, PathBuf};
use std::time::Duration;

use headlamp::jest_discovery::discover_jest_list_tests_for_project_with_patterns_with_timeout;

fn mk_temp_dir(name: &str) -> PathBuf {
    let base = std::env::temp_dir()
        .join("headlamp-jest-ownership-pattern-order-tests")
        .join(name);
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    base
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

#[cfg(unix)]
fn write_executable(path: &Path, contents: &str) {
    use std::os::unix::fs::PermissionsExt;
    write_file(path, contents);
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

#[test]
fn ownership_list_tests_places_patterns_after_list_tests_flag() {
    #[cfg(not(unix))]
    {
        return;
    }

    let repo_root = mk_temp_dir("patterns_after_list_tests");
    let jest_bin = repo_root.join("node_modules").join(".bin").join("jest");
    let args_file = repo_root.join("argv.txt");
    let test_file = repo_root.join("tests/a.test.js");
    write_file(&test_file, "test('a', () => {});\n");

    // fake jest: record argv, print a listTests line so the parser returns a non-empty list
    write_executable(
        &jest_bin,
        &format!(
            "#!/bin/sh\n\
printf \"%s\\n\" \"$@\" > \"{}\"\n\
echo \"{}\"\n\
exit 0\n",
            args_file.to_string_lossy(),
            test_file.to_string_lossy()
        ),
    );

    let discovery_args: Vec<String> = vec!["--no-watchman".to_string()];
    let patterns: Vec<String> = vec!["tests/a.test.js".to_string()];
    let _listed = discover_jest_list_tests_for_project_with_patterns_with_timeout(
        &repo_root,
        &jest_bin,
        &discovery_args,
        "jest.config.js",
        &repo_root,
        &patterns,
        Duration::from_secs(10),
    )
    .unwrap();

    let argv_lines = std::fs::read_to_string(&args_file).unwrap_or_default();
    let argv = argv_lines
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>();

    let list_idx = argv.iter().position(|t| t == "--listTests");
    let pattern_idx = argv.iter().position(|t| t == "tests/a.test.js");
    assert!(
        list_idx.is_some(),
        "expected --listTests in argv: {argv_lines}"
    );
    assert!(
        pattern_idx.is_some(),
        "expected pattern in argv: {argv_lines}"
    );
    assert!(
        pattern_idx.unwrap() > list_idx.unwrap(),
        "expected patterns after --listTests; argv was:\n{argv_lines}"
    );
}
