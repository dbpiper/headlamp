use std::path::{Path, PathBuf};

use duct::cmd as duct_cmd;

pub fn build_cargo_llvm_cov_command_args(
    enable_branch_coverage: bool,
    use_nightly: bool,
    reuse_instrumented_build: bool,
    subcommand_args: &[String],
) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    if enable_branch_coverage && use_nightly {
        out.push("+nightly".to_string());
    }
    out.push("llvm-cov".to_string());
    if enable_branch_coverage && use_nightly {
        out.push("--branch".to_string());
    }
    if reuse_instrumented_build {
        // Dev-speed optimization: reuse the instrumented target directory between runs.
        // This maps to `cargo llvm-cov --no-clean` (we still purge profraw/profdata ourselves).
        out.push("--no-clean".to_string());
    }
    out.extend(subcommand_args.iter().cloned());
    out
}

pub(super) fn can_use_nightly(repo_root: &Path) -> bool {
    // IMPORTANT: do not probe `cargo +nightly ...` here.
    //
    // `cargo +nightly` can cause rustup to auto-download the nightly toolchain on-demand, which
    // makes behavior CI-dependent and can fail later when `llvm-tools-preview` isn't installed
    // for nightly. Instead, only enable nightly if the toolchain already exists *and* has
    // `llvm-tools-preview` installed.
    let nightly_exists = duct_cmd("rustup", ["run", "nightly", "rustc", "--version"])
        .dir(repo_root)
        .stdout_capture()
        .stderr_capture()
        .unchecked()
        .run()
        .ok()
        .is_some_and(|o| o.status.success());
    if !nightly_exists {
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

pub(super) fn apply_headlamp_cargo_target_dir(
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

pub(super) fn headlamp_cargo_target_dir_for_duct(
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
