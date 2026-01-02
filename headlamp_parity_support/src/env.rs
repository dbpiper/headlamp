use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::parity_meta::ParitySideLabel;

pub(crate) fn program_display_name(program: &Path) -> String {
    program
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}

pub(crate) fn headlamp_runner_stack(runner: &str) -> String {
    match runner {
        "cargo-test" => "cargo-test->cargo".to_string(),
        "cargo-nextest" => "cargo-nextest->nextest".to_string(),
        other => format!("{other}->{other}"),
    }
}

pub(crate) fn build_env_map(
    repo: &Path,
    side_label: &ParitySideLabel,
    case_id: Option<&str>,
) -> BTreeMap<String, String> {
    let mut env: BTreeMap<String, String> = BTreeMap::new();
    insert_headlamp_cache_dir_if_missing(repo, &mut env);
    insert_cargo_isolation_env_if_needed(repo, side_label, case_id, &mut env);
    insert_path_with_pytest_venv_if_needed(side_label, &mut env);
    env
}

fn insert_headlamp_cache_dir_if_missing(repo: &Path, env: &mut BTreeMap<String, String>) {
    if std::env::var_os("HEADLAMP_CACHE_DIR").is_none() {
        env.insert(
            "HEADLAMP_CACHE_DIR".to_string(),
            repo.join(".headlamp-cache").to_string_lossy().to_string(),
        );
    }
}

fn insert_cargo_isolation_env_if_needed(
    repo: &Path,
    side_label: &ParitySideLabel,
    case_id: Option<&str>,
    env: &mut BTreeMap<String, String>,
) {
    let needs_cargo_target_dir = side_label.runner_stack.contains("cargo-test")
        || side_label.runner_stack.contains("cargo-nextest");
    if !needs_cargo_target_dir {
        return;
    }
    let repo_key = headlamp::fast_related::stable_repo_key_hash_12(repo);
    let suffix = env_isolation_suffix(side_label, case_id);
    let (base_cargo_home, base_target_dir) = base_cargo_paths();
    env.insert(
        "CARGO_HOME".to_string(),
        base_cargo_home
            .join("parity-fixtures")
            .join("cargo-home")
            .join(repo_key.clone())
            .join(suffix.clone())
            .to_string_lossy()
            .to_string(),
    );
    env.insert(
        "CARGO_TARGET_DIR".to_string(),
        base_target_dir
            .join("parity-fixtures")
            .join("cargo-target")
            .join(repo_key)
            .join(suffix)
            .to_string_lossy()
            .to_string(),
    );
    env.insert(
        "HEADLAMP_PARITY_REUSE_INSTRUMENTED_BUILD".to_string(),
        "1".to_string(),
    );
    env.insert(
        "HEADLAMP_PARITY_SKIP_LLVM_COV_JSON".to_string(),
        "1".to_string(),
    );
}

fn env_isolation_suffix(side_label: &ParitySideLabel, case_id: Option<&str>) -> String {
    let base = side_label.file_safe_label();
    let isolate_by_case = std::env::var_os("HEADLAMP_PARITY_ISOLATE_BY_CASE").is_some();
    if !isolate_by_case {
        return base;
    }
    case_id
        .map(file_safe_component)
        .filter(|s| !s.is_empty())
        .map(|case_component| format!("{base}-{case_component}"))
        .unwrap_or(base)
}

fn file_safe_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_dash = false;
    for ch in text.chars() {
        let lower = ch.to_ascii_lowercase();
        let keep = lower.is_ascii_alphanumeric() || matches!(lower, '-' | '_' | '.');
        let mapped = if keep { lower } else { '-' };
        if mapped == '-' {
            if !prev_dash {
                out.push('-');
            }
            prev_dash = true;
        } else {
            out.push(mapped);
            prev_dash = false;
        }
    }
    out.trim_matches(['-', '_']).to_string()
}

fn base_cargo_paths() -> (PathBuf, PathBuf) {
    let base_target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target"));
    let base_cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("parity-fixtures")
                .join("cargo-home")
        });
    (base_cargo_home, base_target_dir)
}

fn insert_path_with_pytest_venv_if_needed(
    side_label: &ParitySideLabel,
    env: &mut BTreeMap<String, String>,
) {
    let Ok(existing_path) = std::env::var("PATH") else {
        return;
    };
    if !side_label.runner_stack.contains("pytest") {
        env.insert("PATH".to_string(), existing_path);
        return;
    }
    let sep = if cfg!(windows) { ";" } else { ":" };
    let Some(py_bin) = ensure_pytest_venv_bin_dir() else {
        env.insert("PATH".to_string(), existing_path);
        return;
    };
    env.insert(
        "PATH".to_string(),
        format!("{}{}{}", py_bin.to_string_lossy(), sep, existing_path),
    );
}

fn pytest_requirements_path() -> Option<PathBuf> {
    let requirements = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("headlamp_tests")
        .join("tests")
        .join("py_deps")
        .join("requirements.txt");
    requirements.exists().then_some(requirements)
}

fn venv_bin_dir(venv: &Path) -> PathBuf {
    if cfg!(windows) {
        venv.join("Scripts")
    } else {
        venv.join("bin")
    }
}

fn pytest_path(bin: &Path) -> PathBuf {
    if cfg!(windows) {
        bin.join("pytest.exe")
    } else {
        bin.join("pytest")
    }
}

fn python_path(bin: &Path) -> PathBuf {
    if cfg!(windows) {
        bin.join("python.exe")
    } else {
        bin.join("python")
    }
}

fn repo_pytest_bin_if_installed(requirements: &Path) -> Option<PathBuf> {
    let repo_venv = requirements.parent()?.join(".venv");
    let repo_venv_bin = venv_bin_dir(&repo_venv);
    pytest_path(&repo_venv_bin)
        .exists()
        .then_some(repo_venv_bin)
}

fn shared_py_deps_venv_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("py_deps_venv")
}

#[derive(Debug)]
struct VenvInitLockGuard {
    lock_dir: PathBuf,
}

impl Drop for VenvInitLockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.lock_dir);
    }
}

fn acquire_venv_init_lock(lock_dir: &Path) -> VenvInitLockGuard {
    let mut attempts: usize = 0;
    loop {
        match std::fs::create_dir(lock_dir) {
            Ok(()) => {
                return VenvInitLockGuard {
                    lock_dir: lock_dir.to_path_buf(),
                };
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                attempts = attempts.saturating_add(1);
                if attempts > 600 {
                    panic!(
                        "timed out waiting for pytest venv init lock {}",
                        lock_dir.display()
                    );
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(error) => panic!(
                "failed to acquire pytest venv init lock {} ({error})",
                lock_dir.display()
            ),
        }
    }
}

fn create_venv(venv: &Path) -> Option<()> {
    let status = Command::new("python3")
        .args(["-m", "venv"])
        .arg(venv)
        .status()
        .ok()?;
    if !status.success() {
        panic!(
            "failed to create pytest venv at {} (python3 -m venv exit={})",
            venv.display(),
            status.code().unwrap_or(1)
        );
    }
    Some(())
}

fn pip_install_requirements(python: &Path, requirements: &Path, venv: &Path) -> Option<()> {
    let status = Command::new(python)
        .env("PIP_DISABLE_PIP_VERSION_CHECK", "1")
        .args([
            "-m",
            "pip",
            "install",
            "--disable-pip-version-check",
            "--no-input",
            "-r",
        ])
        .arg(requirements)
        .status()
        .ok()?;
    if !status.success() {
        panic!(
            "failed to install pytest deps into {} (pip exit={}). You may need network access.",
            venv.display(),
            status.code().unwrap_or(1)
        );
    }
    Some(())
}

fn ensure_pytest_venv_bin_dir() -> Option<PathBuf> {
    static PY_BIN: OnceLock<Option<PathBuf>> = OnceLock::new();
    PY_BIN
        .get_or_init(|| {
            // CI speed: allow using a preinstalled venv inside the CI image.
            let prebuilt = PathBuf::from("/opt/headlamp/py_venv/bin");
            if pytest_path(&prebuilt).exists() {
                return Some(prebuilt);
            }

            let requirements = pytest_requirements_path()?;
            if let Some(bin) = repo_pytest_bin_if_installed(&requirements) {
                return Some(bin);
            }

            let venv = shared_py_deps_venv_dir();
            let bin = venv_bin_dir(&venv);
            let _lock = acquire_venv_init_lock(&venv.with_extension("init-lock"));

            if pytest_path(&bin).exists() {
                return Some(bin);
            }

            if !python_path(&bin).exists() {
                create_venv(&venv)?;
            }
            let python = python_path(&bin);
            pip_install_requirements(&python, &requirements, &venv)?;
            pytest_path(&bin).exists().then_some(bin)
        })
        .clone()
}
