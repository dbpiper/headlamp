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

fn ensure_pytest_venv_bin_dir() -> Option<PathBuf> {
    static PY_BIN: OnceLock<Option<PathBuf>> = OnceLock::new();
    PY_BIN
        .get_or_init(|| {
            let requirements = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("headlamp_tests")
                .join("tests")
                .join("py_deps")
                .join("requirements.txt");
            if !requirements.exists() {
                return None;
            }

            let repo_venv = requirements.parent().unwrap().join(".venv");
            let repo_venv_bin = if cfg!(windows) {
                repo_venv.join("Scripts")
            } else {
                repo_venv.join("bin")
            };
            let repo_pytest = if cfg!(windows) {
                repo_venv_bin.join("pytest.exe")
            } else {
                repo_venv_bin.join("pytest")
            };
            if repo_pytest.exists() {
                return Some(repo_venv_bin);
            }

            let venv = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("py_deps_venv");
            let bin = if cfg!(windows) {
                venv.join("Scripts")
            } else {
                venv.join("bin")
            };
            let python = if cfg!(windows) {
                bin.join("python.exe")
            } else {
                bin.join("python")
            };
            let pytest = if cfg!(windows) {
                bin.join("pytest.exe")
            } else {
                bin.join("pytest")
            };
            if pytest.exists() {
                return Some(bin);
            }

            let status = Command::new("python3")
                .args(["-m", "venv"])
                .arg(&venv)
                .status()
                .ok()?;
            if !status.success() {
                panic!(
                    "failed to create pytest venv at {} (python3 -m venv exit={})",
                    venv.display(),
                    status.code().unwrap_or(1)
                );
            }

            let status = Command::new(&python)
                .env("PIP_DISABLE_PIP_VERSION_CHECK", "1")
                .args([
                    "-m",
                    "pip",
                    "install",
                    "--disable-pip-version-check",
                    "--no-input",
                    "-r",
                ])
                .arg(&requirements)
                .status()
                .ok()?;
            if !status.success() {
                panic!(
                    "failed to install pytest deps into {} (pip exit={}). You may need network access.",
                    venv.display(),
                    status.code().unwrap_or(1)
                );
            }
            Some(bin)
        })
        .clone()
}
