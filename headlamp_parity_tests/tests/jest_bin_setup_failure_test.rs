mod parity_support;

use std::panic::AssertUnwindSafe;

#[test]
fn jest_bin_setup_failure_renders_like_jest() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path();

    std::fs::write(repo.join("node_modules"), "not a dir").expect("write node_modules file");

    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        parity_support::runner_parity::write_real_runner_repo(repo);
    }));

    let panic_payload = result.expect_err("expected panic");
    let message = panic_payload
        .downcast_ref::<String>()
        .map(|s| s.as_str())
        .or_else(|| panic_payload.downcast_ref::<&str>().copied())
        .unwrap_or("");

    assert!(message.contains("Test suite failed to run"));
    assert!(message.contains("jest"));
    assert!(message.contains("node_modules"));
}
