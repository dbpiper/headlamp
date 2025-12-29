use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::hashing::sha1_12;
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

pub(crate) fn build_env_map(repo: &Path, side_label: &ParitySideLabel) -> BTreeMap<String, String> {
    let mut env: BTreeMap<String, String> = BTreeMap::new();
    if std::env::var_os("HEADLAMP_CACHE_DIR").is_none() {
        env.insert(
            "HEADLAMP_CACHE_DIR".to_string(),
            repo.join(".headlamp-cache").to_string_lossy().to_string(),
        );
    }
    let needs_cargo_target_dir = side_label.runner_stack.contains("cargo-test")
        || side_label.runner_stack.contains("cargo-nextest");
    if needs_cargo_target_dir {
        let repo_key = sha1_12(&repo.to_string_lossy());
        let suffix = side_label.file_safe_label();
        env.insert(
            "CARGO_TARGET_DIR".to_string(),
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("parity-fixtures")
                .join("cargo-target")
                .join(repo_key)
                .join(suffix)
                .to_string_lossy()
                .to_string(),
        );
    }
    if let Ok(existing_path) = std::env::var("PATH") {
        let sep = if cfg!(windows) { ";" } else { ":" };
        let needs_pytest = side_label.runner_stack.contains("pytest");
        if needs_pytest {
            if let Some(py_bin) = ensure_pytest_venv_bin_dir() {
                env.insert(
                    "PATH".to_string(),
                    format!("{}{}{}", py_bin.to_string_lossy(), sep, existing_path),
                );
            } else {
                env.insert("PATH".to_string(), existing_path);
            }
        } else {
            env.insert("PATH".to_string(), existing_path);
        }
    }
    env
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
            let requirements = pytest_requirements_path()?;
            if let Some(bin) = repo_pytest_bin_if_installed(&requirements) {
                return Some(bin);
            }

            let venv = shared_py_deps_venv_dir();
            let bin = venv_bin_dir(&venv);
            if pytest_path(&bin).exists() {
                return Some(bin);
            }

            create_venv(&venv)?;
            let python = python_path(&bin);
            pip_install_requirements(&python, &requirements, &venv)?;
            Some(bin)
        })
        .clone()
}
