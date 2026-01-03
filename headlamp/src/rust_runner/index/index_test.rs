use super::{CachedBinary, CachedBinaryIndex, try_load_cache};

#[test]
fn binary_index_cache_is_not_reused_across_different_repo_roots() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_a = temp_dir.path().join("repo-a");
    let repo_b = temp_dir.path().join("repo-b");
    std::fs::create_dir_all(&repo_a).expect("create repo-a");
    std::fs::create_dir_all(&repo_b).expect("create repo-b");

    let exe_dir = temp_dir.path().join("bin");
    std::fs::create_dir_all(&exe_dir).expect("create bin");
    let exe_path = exe_dir.join("fake-test-bin");
    std::fs::write(&exe_path, b"").expect("write fake exe");

    let cache_file = temp_dir.path().join("binary_index.json");
    let expected_fingerprint = "same-fingerprint";
    let cached = CachedBinaryIndex {
        repo_root: repo_a.to_string_lossy().to_string(),
        fingerprint: expected_fingerprint.to_string(),
        binaries: vec![CachedBinary {
            executable: exe_path.to_string_lossy().to_string(),
            suite_source_path: "tests/basic.rs".to_string(),
        }],
    };
    let bytes = serde_json::to_vec(&cached).expect("serialize");
    std::fs::write(&cache_file, bytes).expect("write cache");

    let hit_a = try_load_cache(
        &cache_file,
        repo_a.to_string_lossy().as_ref(),
        expected_fingerprint,
    );
    assert!(hit_a.is_some(), "expected cache hit for repo-a");

    let hit_b = try_load_cache(
        &cache_file,
        repo_b.to_string_lossy().as_ref(),
        expected_fingerprint,
    );
    assert!(
        hit_b.is_none(),
        "did not expect cache hit for different repo root"
    );
}
