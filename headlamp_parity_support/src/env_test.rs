use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::env::build_env_map;
use crate::parity_meta::ParitySideLabel;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn build_env_map_sets_isolated_cargo_home_and_target_dir_per_runner_stack() {
    let _guard = env_lock().lock().unwrap();

    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path().join("repo");
    std::fs::create_dir_all(&repo_root).unwrap();

    let outer_target_dir = temp_dir.path().join("outer-target");
    let outer_cargo_home = temp_dir.path().join("outer-cargo-home");
    std::fs::create_dir_all(&outer_target_dir).unwrap();
    std::fs::create_dir_all(&outer_cargo_home).unwrap();

    unsafe {
        std::env::set_var("CARGO_TARGET_DIR", &outer_target_dir);
        std::env::set_var("CARGO_HOME", &outer_cargo_home);
    }

    let cargo_test_label = ParitySideLabel {
        binary: "headlamp".to_string(),
        runner_stack: "cargo-test->cargo".to_string(),
    };
    let cargo_nextest_label = ParitySideLabel {
        binary: "headlamp".to_string(),
        runner_stack: "cargo-nextest->nextest".to_string(),
    };

    let cargo_test_env = build_env_map(&repo_root, &cargo_test_label, None);
    let cargo_nextest_env = build_env_map(&repo_root, &cargo_nextest_label, None);

    let cargo_test_target_dir = PathBuf::from(cargo_test_env.get("CARGO_TARGET_DIR").unwrap());
    let cargo_nextest_target_dir =
        PathBuf::from(cargo_nextest_env.get("CARGO_TARGET_DIR").unwrap());

    assert!(cargo_test_target_dir.starts_with(&outer_target_dir));
    assert!(cargo_nextest_target_dir.starts_with(&outer_target_dir));
    assert_ne!(cargo_test_target_dir, cargo_nextest_target_dir);

    let cargo_test_home = PathBuf::from(cargo_test_env.get("CARGO_HOME").unwrap());
    let cargo_nextest_home = PathBuf::from(cargo_nextest_env.get("CARGO_HOME").unwrap());

    assert!(cargo_test_home.starts_with(&outer_cargo_home));
    assert!(cargo_nextest_home.starts_with(&outer_cargo_home));
    assert_ne!(cargo_test_home, cargo_nextest_home);
}
