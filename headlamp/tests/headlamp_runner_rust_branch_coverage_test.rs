use std::path::Path;
use std::process::Command;

fn nightly_with_llvm_tools_preview_installed(repo_root: &Path) -> bool {
    let nightly_exists = Command::new("rustup")
        .current_dir(repo_root)
        .args(["run", "nightly", "rustc", "--version"])
        .output()
        .is_ok_and(|o| o.status.success());
    if !nightly_exists {
        return false;
    }
    let components = Command::new("rustup")
        .current_dir(repo_root)
        .args(["component", "list", "--toolchain", "nightly"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    components.contains("llvm-tools-preview") && components.contains("installed")
}

fn llvm_tools_preview_available_for_toolchain(repo_root: &Path, toolchain: &str) -> bool {
    Command::new("rustup")
        .current_dir(repo_root)
        .args(["run", toolchain, "llvm-cov", "--version"])
        .output()
        .is_ok_and(|o| o.status.success())
        && Command::new("rustup")
            .current_dir(repo_root)
            .args(["run", toolchain, "llvm-profdata", "--version"])
            .output()
            .is_ok_and(|o| o.status.success())
}

fn nightly_rustc_exists(repo_root: &Path) -> bool {
    Command::new("rustup")
        .current_dir(repo_root)
        .args(["run", "nightly", "rustc", "--version"])
        .output()
        .is_ok_and(|o| o.status.success())
}

fn should_run_test(project_root: &Path) -> bool {
    if !nightly_rustc_exists(project_root) {
        return false;
    }
    let stable_tools = llvm_tools_preview_available_for_toolchain(project_root, "stable");
    if !stable_tools {
        return false;
    }
    let nightly_tools = llvm_tools_preview_available_for_toolchain(project_root, "nightly");
    if nightly_tools && nightly_with_llvm_tools_preview_installed(project_root) {
        return false;
    }
    true
}

fn write_fixture_project(project_root: &Path) {
    std::fs::write(
        project_root.join("Cargo.toml"),
        r#"[package]
name = "headlamp_runner_branch_coverage_smoke"
version = "0.1.0"
edition = "2024"
"#,
    )
    .expect("write Cargo.toml");

    std::fs::create_dir_all(project_root.join("src")).expect("create src/");
    std::fs::write(
        project_root.join("src").join("lib.rs"),
        r#"#[inline(never)]
pub fn branchy(x: u8) -> u8 {
    if x == 0 { 1 } else { 2 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_true_branch() {
        assert_eq!(branchy(0), 1);
    }

    #[test]
    fn covers_false_branch() {
        assert_eq!(branchy(1), 2);
    }
}
"#,
    )
    .expect("write src/lib.rs");
}

fn read_lcov(session: &headlamp::session::RunSession) -> String {
    let lcov_path = session.subdir("coverage").join("rust").join("lcov.info");
    assert!(
        lcov_path.exists(),
        "expected lcov at {}",
        lcov_path.display()
    );
    std::fs::read_to_string(&lcov_path).expect("read lcov")
}

#[test]
fn headlamp_runner_rust_coverage_includes_branch_data_when_nightly_available() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let project_root = temp_dir.path();

    if !should_run_test(project_root) {
        return;
    }
    write_fixture_project(project_root);

    let argv = vec!["--coverage".to_string()];
    let parsed = headlamp::args::derive_args(&[], &argv, false);
    let session = headlamp::session::RunSession::new(false).expect("session");
    let exit_code =
        headlamp::rust_runner::run_headlamp_rust(project_root, &parsed, &session).expect("run");
    assert_eq!(exit_code, 0);

    let lcov_text = read_lcov(&session);
    assert!(
        lcov_text.contains("BRDA:") || lcov_text.contains("BRF:"),
        "expected branch coverage entries (BRDA/BRF) in lcov, got:\n{}",
        lcov_text.lines().take(80).collect::<Vec<_>>().join("\n")
    );
}
