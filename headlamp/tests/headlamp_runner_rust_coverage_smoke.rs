#[test]
fn headlamp_runner_rust_coverage_smoke_generates_lcov() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let project_root = temp_dir.path();

    std::fs::write(
        project_root.join("Cargo.toml"),
        r#"[package]
name = "headlamp_runner_coverage_smoke"
version = "0.1.0"
edition = "2024"
"#,
    )
    .expect("write Cargo.toml");

    std::fs::create_dir_all(project_root.join("src")).expect("create src/");
    std::fs::write(
        project_root.join("src").join("lib.rs"),
        r#"#[cfg(test)]
mod tests {
    #[test]
    fn smoke_one() {
        assert_eq!(1, 1);
    }
}
"#,
    )
    .expect("write src/lib.rs");

    let argv = vec!["--coverage".to_string(), "smoke_one".to_string()];
    let parsed = headlamp::args::derive_args(&[], &argv, false);
    let session = headlamp::session::RunSession::new(false).expect("session");

    let run = headlamp::rust_runner::run_headlamp_rust(project_root, &parsed, &session);
    let exit_code = match run {
        Ok(code) => code,
        Err(headlamp::run::RunError::MissingRunner { runner, hint }) => {
            assert!(
                runner.contains("llvm-"),
                "expected llvm tool missing runner, got {runner}"
            );
            assert!(
                hint.contains("llvm-tools-preview"),
                "expected actionable hint, got: {hint}"
            );
            return;
        }
        Err(err) => panic!("unexpected error: {err}"),
    };
    assert_eq!(exit_code, 0);

    let lcov_path = session.subdir("coverage").join("rust").join("lcov.info");
    assert!(
        lcov_path.exists(),
        "expected lcov at {}",
        lcov_path.display()
    );
    let lcov_text = std::fs::read_to_string(&lcov_path).expect("read lcov");
    assert!(
        lcov_text.contains("src/lib.rs"),
        "expected src/lib.rs to appear in lcov"
    );
}
