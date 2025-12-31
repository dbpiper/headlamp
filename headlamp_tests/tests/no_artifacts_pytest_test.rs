use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use tempfile::TempDir;

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone)]
struct EnvSnapshot {
    path: Option<std::ffi::OsString>,
    tmpdir: Option<std::ffi::OsString>,
    headlamp_cache_dir: Option<std::ffi::OsString>,
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

fn snapshot_files(root: &Path) -> BTreeMap<PathBuf, u64> {
    fn walk(acc: &mut BTreeMap<PathBuf, u64>, base: &Path, dir: &Path) {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                walk(acc, base, &path);
                continue;
            }
            if !meta.is_file() {
                continue;
            }
            let rel = path.strip_prefix(base).unwrap_or(&path).to_path_buf();
            acc.insert(rel, meta.len());
        }
    }

    let mut out: BTreeMap<PathBuf, u64> = BTreeMap::new();
    walk(&mut out, root, root);
    out
}

fn list_dir_entries(dir: &Path) -> BTreeSet<String> {
    std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .collect()
}

fn setup_minimal_pytest_repo(repo_root: &Path) {
    write_file(&repo_root.join("pyproject.toml"), "[tool.headlamp]\n");
    write_file(&repo_root.join("foo.py"), "def foo():\n    return 1\n");
    write_file(
        &repo_root.join("test_foo.py"),
        "def test_foo():\n    assert True\n",
    );
}

fn write_fake_pytest(bin_root: &Path) -> PathBuf {
    let fake_pytest = bin_root.join("pytest");
    write_file(
        &fake_pytest,
        r#"#!/bin/sh
set -eu

LCOV_PATH=""
prev=""
for arg in "$@"; do
  case "$arg" in
    --cov-report=lcov:*)
      LCOV_PATH="${arg#--cov-report=lcov:}"
      ;;
    lcov:*)
      if [ "$prev" = "--cov-report" ]; then
        LCOV_PATH="${arg#lcov:}"
      fi
      ;;
  esac
  prev="$arg"
done

if [ -n "${LCOV_PATH}" ]; then
  mkdir -p "$(dirname "$LCOV_PATH")"
  cat > "$LCOV_PATH" <<'EOF'
TN:
SF:foo.py
DA:1,1
end_of_record
EOF
fi

if [ -n "${COVERAGE_FILE:-}" ]; then
  mkdir -p "$(dirname "$COVERAGE_FILE")"
  echo "x" > "$COVERAGE_FILE"
fi

exit 0
"#,
    );
    make_executable(&fake_pytest);
    fake_pytest
}

fn write_fake_python(bin_root: &Path) -> PathBuf {
    let fake_python = bin_root.join("python");
    write_file(
        &fake_python,
        r#"#!/bin/sh
set -eu

OUT=""
prev=""
for arg in "$@"; do
  if [ "$prev" = "-o" ]; then
    OUT="$arg"
    break
  fi
  prev="$arg"
done

if [ -n "$OUT" ]; then
  mkdir -p "$(dirname "$OUT")"
  cat > "$OUT" <<'EOF'
{
  "files": {
    "foo.py": { "summary": { "num_statements": 1, "covered_lines": 1 } }
  }
}
EOF
fi
exit 0
"#,
    );
    make_executable(&fake_python);
    fake_python
}

fn snapshot_env() -> EnvSnapshot {
    EnvSnapshot {
        path: std::env::var_os("PATH"),
        tmpdir: std::env::var_os("TMPDIR"),
        headlamp_cache_dir: std::env::var_os("HEADLAMP_CACHE_DIR"),
    }
}

fn apply_test_env(bin_root: &Path, tmp_root: &Path) {
    unsafe {
        std::env::set_var(
            "PATH",
            format!(
                "{}:{}",
                bin_root.to_string_lossy(),
                std::env::var("PATH").unwrap_or_default()
            ),
        );
        std::env::set_var("TMPDIR", tmp_root);
    }
}

fn restore_env(snapshot: EnvSnapshot) {
    match snapshot.path {
        Some(v) => unsafe { std::env::set_var("PATH", v) },
        None => unsafe { std::env::remove_var("PATH") },
    }
    match snapshot.tmpdir {
        Some(v) => unsafe { std::env::set_var("TMPDIR", v) },
        None => unsafe { std::env::remove_var("TMPDIR") },
    }
    match snapshot.headlamp_cache_dir {
        Some(v) => unsafe { std::env::set_var("HEADLAMP_CACHE_DIR", v) },
        None => unsafe { std::env::remove_var("HEADLAMP_CACHE_DIR") },
    }
}

fn run_headlamp_pytest_coverage(repo_root: &Path) {
    let cfg = headlamp::config::HeadlampConfig::default();
    let argv = vec!["--coverage".to_string()];
    let parsed =
        headlamp::args::derive_args(&headlamp::args::config_tokens(&cfg, &argv), &argv, false);
    let session = headlamp::session::RunSession::new(false).unwrap();
    let cache_dir = session.subdir("cache");
    let _ = std::fs::create_dir_all(&cache_dir);
    unsafe { std::env::set_var("HEADLAMP_CACHE_DIR", cache_dir) };
    let exit_code = headlamp::pytest::run_pytest(repo_root, &parsed, &session).unwrap();
    assert_eq!(exit_code, 0);
}

#[test]
fn headlamp_pytest_leaves_no_repo_or_tmpdir_artifacts() {
    let _env_guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    let repo_dir = TempDir::new().unwrap();
    let repo_root = repo_dir.path();
    setup_minimal_pytest_repo(repo_root);

    let bin_dir = TempDir::new().unwrap();
    let bin_root = bin_dir.path();

    let _fake_pytest = write_fake_pytest(bin_root);
    let _fake_python = write_fake_python(bin_root);

    let tmp_base = TempDir::new().unwrap();

    let before_repo = snapshot_files(repo_root);
    let before_tmp_entries = list_dir_entries(tmp_base.path());

    let env_before = snapshot_env();
    apply_test_env(bin_root, tmp_base.path());
    run_headlamp_pytest_coverage(repo_root);
    restore_env(env_before);

    let after_repo = snapshot_files(repo_root);
    let after_tmp_entries = list_dir_entries(tmp_base.path());

    assert_eq!(before_repo, after_repo);
    assert_eq!(before_tmp_entries, after_tmp_entries);
}
