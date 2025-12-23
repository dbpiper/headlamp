use std::path::{Path, PathBuf};
use std::time::Duration;

use path_slash::PathExt;

use headlamp::jest_discovery::{
    discover_jest_list_tests_resilient_with_timeout, discover_jest_list_tests_with_timeout,
};
use headlamp::run::RunError;

fn mk_temp_dir(name: &str) -> PathBuf {
    let base = std::env::temp_dir()
        .join("headlamp-jest-discovery-timeout-tests")
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
fn jest_list_tests_times_out() {
    #[cfg(not(unix))]
    {
        return;
    }

    let repo_root = mk_temp_dir("jest_list_tests_times_out");
    write_file(
        &repo_root.join("jest.config.js"),
        "module.exports = { testMatch: ['**/tests/**/*.test.js'] };\n",
    );
    write_file(&repo_root.join("tests/a.test.js"), "test('a', () => {});\n");

    let jest_bin = repo_root.join("node_modules").join(".bin").join("jest");
    write_executable(&jest_bin, "#!/bin/sh\nsleep 2\nexit 0\n");

    let err = discover_jest_list_tests_with_timeout(
        &repo_root,
        &jest_bin,
        &[],
        Duration::from_millis(50),
    )
    .unwrap_err();
    assert!(matches!(err, RunError::TimedOut { .. }));
}

#[test]
fn jest_list_tests_timeout_falls_back_to_rg_when_related_paths_exist() {
    #[cfg(not(unix))]
    {
        return;
    }

    let Ok(_rg) = which::which("rg") else {
        return;
    };

    let repo_root = mk_temp_dir("jest_list_tests_timeout_rg_fallback");
    write_file(
        &repo_root.join("jest.config.js"),
        "module.exports = { testMatch: ['**/tests/**/*.test.ts'] };\n",
    );

    let production_file = repo_root.join("src/foo.ts");
    write_file(&production_file, "export const foo = 1;\n");
    let test_file = repo_root.join("tests/foo.test.ts");
    write_file(
        &test_file,
        "import { foo } from '../src/foo';\nexpect(foo).toBe(1);\n",
    );

    let jest_bin = repo_root.join("node_modules").join(".bin").join("jest");
    write_executable(&jest_bin, "#!/bin/sh\nsleep 2\nexit 0\n");

    let related_production_paths_abs = vec![production_file.to_string_lossy().to_string()];
    let discovered = discover_jest_list_tests_resilient_with_timeout(
        &repo_root,
        &jest_bin,
        &[],
        &related_production_paths_abs,
        &[],
        Duration::from_millis(50),
    )
    .unwrap();

    let test_file_posix = test_file.as_path().to_slash_lossy().to_string();
    assert!(discovered.iter().any(|p| p == &test_file_posix));
}
