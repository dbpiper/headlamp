use super::runner_parity;

#[test]
fn worktree_lease_provides_isolated_repo_with_expected_files() {
    let lease = runner_parity::lease_real_runner_worktree("worktree_lease_smoke");
    let repo = lease.path();
    assert!(repo.join("src/sum.js").exists());
    assert!(repo.join("src/sum.rs").exists());
    assert!(repo.join("src/sum.py").exists());
    assert!(repo.join("node_modules").exists());
    assert!(repo.join("node_modules/.bin").exists());
}

#[test]
fn worktree_lease_cleans_untracked_files_on_reacquire() {
    let injected_name = "parity_untracked_from_test.txt";
    {
        let lease = runner_parity::lease_real_runner_worktree("worktree_lease_clean_1");
        let repo = lease.path();
        std::fs::write(repo.join(injected_name), "hello").unwrap();
        assert!(repo.join(injected_name).exists());
    }
    {
        let lease = runner_parity::lease_real_runner_worktree("worktree_lease_clean_2");
        let repo = lease.path();
        assert!(
            !repo.join(injected_name).exists(),
            "expected lease acquire to clean untracked files"
        );
    }
}
