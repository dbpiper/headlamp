use std::sync::{LazyLock, Mutex};

static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[test]
fn parity_env_allows_reuse_instrumented_build_in_ci() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe {
        std::env::remove_var("HEADLAMP_PARITY_REUSE_INSTRUMENTED_BUILD");
    }
    assert!(!super::llvm_cov::should_reuse_instrumented_build(true));

    unsafe {
        std::env::set_var("HEADLAMP_PARITY_REUSE_INSTRUMENTED_BUILD", "1");
    }
    assert!(super::llvm_cov::should_reuse_instrumented_build(true));
    unsafe {
        std::env::remove_var("HEADLAMP_PARITY_REUSE_INSTRUMENTED_BUILD");
    }
}
