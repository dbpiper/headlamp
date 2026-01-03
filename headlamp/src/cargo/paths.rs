use std::path::{Path, PathBuf};

use duct::cmd as duct_cmd;

pub(crate) fn nightly_rustc_exists(repo_root: &Path) -> bool {
    // IMPORTANT: do not probe `cargo +nightly ...` here.
    //
    // `cargo +nightly` can cause rustup to auto-download the nightly toolchain on-demand.
    duct_cmd("rustup", ["run", "nightly", "rustc", "--version"])
        .dir(repo_root)
        .stdout_capture()
        .stderr_capture()
        .unchecked()
        .run()
        .ok()
        .is_some_and(|o| o.status.success())
}

pub(crate) fn can_use_nightly(repo_root: &Path) -> bool {
    // IMPORTANT: do not probe `cargo +nightly ...` here.
    //
    // `cargo +nightly` can cause rustup to auto-download the nightly toolchain on-demand, which
    // makes behavior CI-dependent and can fail later when `llvm-tools-preview` isn't installed
    // for nightly. Instead, only enable nightly if the toolchain already exists *and* has
    // `llvm-tools-preview` installed.
    if !nightly_rustc_exists(repo_root) {
        return false;
    }
    let components = duct_cmd("rustup", ["component", "list", "--toolchain", "nightly"])
        .dir(repo_root)
        .stdout_capture()
        .stderr_capture()
        .unchecked()
        .run()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    components.contains("llvm-tools-preview") && components.contains("installed")
}

fn headlamp_cargo_target_dir(repo_root: &Path) -> PathBuf {
    repo_root.join("target").join("headlamp-cargo")
}

fn headlamp_cargo_target_dir_for_session(
    keep_artifacts: bool,
    repo_root: &Path,
    session: &crate::session::RunSession,
) -> PathBuf {
    if keep_artifacts {
        headlamp_cargo_target_dir(repo_root)
    } else {
        session.subdir("cargo-target")
    }
}

pub(crate) fn apply_headlamp_cargo_target_dir(
    cmd: &mut std::process::Command,
    keep_artifacts: bool,
    repo_root: &Path,
    session: &crate::session::RunSession,
) {
    // Respect caller-provided CARGO_TARGET_DIR (important for isolation in tests and for users
    // who already manage their own target dirs).
    if std::env::var_os("CARGO_TARGET_DIR").is_none() {
        cmd.env(
            "CARGO_TARGET_DIR",
            headlamp_cargo_target_dir_for_session(keep_artifacts, repo_root, session),
        );
    }
}

pub(crate) fn headlamp_cargo_target_dir_for_duct(
    keep_artifacts: bool,
    repo_root: &Path,
    session: &crate::session::RunSession,
) -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            headlamp_cargo_target_dir_for_session(keep_artifacts, repo_root, session)
        })
}
