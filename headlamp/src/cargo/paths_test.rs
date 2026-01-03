use std::sync::Mutex;

use crate::session::RunSession;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn with_env_var_removed<T>(key: &str, f: impl FnOnce() -> T) -> T {
    let previous_value = std::env::var_os(key);
    unsafe { std::env::remove_var(key) };
    let result = f();
    if let Some(value) = previous_value {
        unsafe { std::env::set_var(key, value) };
    }
    result
}

fn env_value(cmd: &std::process::Command, key: &str) -> Option<std::ffi::OsString> {
    cmd.get_envs()
        .find_map(|(k, v)| (k == key).then(|| v.map(|vv| vv.to_os_string()).unwrap_or_default()))
}

#[test]
fn apply_headlamp_cargo_target_dir_defaults_to_repo_target_headlamp_cargo_even_when_ephemeral() {
    let _guard = ENV_LOCK.lock().unwrap();
    with_env_var_removed("CARGO_TARGET_DIR", || {
        let temp_dir = tempfile::tempdir().unwrap();
        let repo_root = temp_dir.path();
        let session = RunSession::new(false).unwrap();
        let mut cmd = std::process::Command::new("cargo");

        super::paths::apply_headlamp_cargo_target_dir(&mut cmd, false, repo_root, &session);

        let expected = repo_root.join("target").join("headlamp-cargo");
        let actual = env_value(&cmd, "CARGO_TARGET_DIR").expect("CARGO_TARGET_DIR should be set");
        assert_eq!(std::path::PathBuf::from(actual), expected);
    });
}
