mod parity_support;

#[test]
fn jest_bin_setup_failure_renders_like_jest() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path();

    std::fs::write(repo.join("node_modules"), "not a dir").expect("write node_modules file");

    parity_support::runner_parity::write_real_runner_repo(repo);

    let node_modules = repo.join("node_modules");
    assert!(
        node_modules.is_dir(),
        "expected node_modules to be repaired into a directory"
    );
}
