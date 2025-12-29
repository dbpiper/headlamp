use std::path::{Path, PathBuf};
use std::time::Duration;

use headlamp::jest_discovery::discover_jest_list_tests_cached_with_timeout;

fn mk_temp_dir(name: &str) -> PathBuf {
    let unique_suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_nanos();
    let base = std::env::temp_dir()
        .join("headlamp-jest-discovery-cache-tests")
        .join(format!("{name}-{unique_suffix}"));
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
fn discover_jest_list_tests_cached_avoids_rerun() {
    #[cfg(not(unix))]
    {
        return;
    }

    let repo_root = mk_temp_dir("cached_avoids_rerun");
    write_file(
        &repo_root.join("jest.config.js"),
        "module.exports = { testMatch: ['**/tests/**/*.test.js'] };\n",
    );
    write_file(&repo_root.join("tests/a.test.js"), "test('a', () => {});\n");

    let jest_bin = repo_root.join("node_modules").join(".bin").join("jest");
    let counter_file = repo_root.join("jest_invocations.txt");
    let test_file_abs = repo_root.join("tests/a.test.js");
    write_executable(
        &jest_bin,
        &format!(
            "#!/bin/sh\n\
echo 1 >> \"{}\"\n\
echo \"{}\"\n\
exit 0\n",
            counter_file.to_string_lossy(),
            test_file_abs.to_string_lossy()
        ),
    );

    let args: Vec<String> = vec!["--config".to_string(), "jest.config.js".to_string()];
    let first = discover_jest_list_tests_cached_with_timeout(
        &repo_root,
        &jest_bin,
        &args,
        false,
        Duration::from_secs(10),
    )
    .unwrap();
    let second = discover_jest_list_tests_cached_with_timeout(
        &repo_root,
        &jest_bin,
        &args,
        false,
        Duration::from_secs(10),
    )
    .unwrap();

    assert_eq!(first, second);
    let invocations = std::fs::read_to_string(&counter_file).unwrap_or_default();
    let count = invocations.lines().filter(|l| !l.trim().is_empty()).count();
    assert_eq!(count, 1);
}

#[test]
fn discover_jest_list_tests_no_cache_reruns_each_time() {
    #[cfg(not(unix))]
    {
        return;
    }

    let repo_root = mk_temp_dir("nocache_reruns_each_time");
    write_file(
        &repo_root.join("jest.config.js"),
        "module.exports = { testMatch: ['**/tests/**/*.test.js'] };\n",
    );
    write_file(&repo_root.join("tests/a.test.js"), "test('a', () => {});\n");

    let jest_bin = repo_root.join("node_modules").join(".bin").join("jest");
    let counter_file = repo_root.join("jest_invocations.txt");
    let test_file_abs = repo_root.join("tests/a.test.js");
    write_executable(
        &jest_bin,
        &format!(
            "#!/bin/sh\n\
echo 1 >> \"{}\"\n\
echo \"{}\"\n\
exit 0\n",
            counter_file.to_string_lossy(),
            test_file_abs.to_string_lossy()
        ),
    );

    let args: Vec<String> = vec!["--config".to_string(), "jest.config.js".to_string()];
    let _ = discover_jest_list_tests_cached_with_timeout(
        &repo_root,
        &jest_bin,
        &args,
        true,
        Duration::from_secs(10),
    )
    .unwrap();
    let _ = discover_jest_list_tests_cached_with_timeout(
        &repo_root,
        &jest_bin,
        &args,
        true,
        Duration::from_secs(10),
    )
    .unwrap();

    let invocations = std::fs::read_to_string(&counter_file).unwrap_or_default();
    let count = invocations.lines().filter(|l| !l.trim().is_empty()).count();
    assert_eq!(count, 2);
}
