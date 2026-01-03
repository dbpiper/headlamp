#[test]
fn headlamp_runner_rust_smoke_runs_one_test_by_filter() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let project_root = temp_dir.path();

    std::fs::write(
        project_root.join("Cargo.toml"),
        r#"[package]
name = "headlamp_runner_smoke"
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

    #[test]
    fn smoke_two() {
        assert_eq!(2, 2);
    }
}
"#,
    )
    .expect("write src/lib.rs");

    let argv = vec!["smoke_one".to_string()];
    let parsed = headlamp::args::derive_args(&[], &argv, false);
    let session = headlamp::session::RunSession::new(false).expect("session");

    let exit_code = headlamp::rust_runner::run_headlamp_rust(project_root, &parsed, &session)
        .expect("run_headlamp_rust");
    assert_eq!(exit_code, 0);
}
