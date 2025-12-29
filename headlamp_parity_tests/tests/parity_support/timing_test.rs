use super::timing;

use std::sync::{Mutex, OnceLock};

fn with_env_lock(run: impl FnOnce()) {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let lock = LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().unwrap();
    run();
}

#[test]
fn timings_enabled_defaults_false_when_env_missing() {
    with_env_lock(|| {
        unsafe { std::env::remove_var("HEADLAMP_PARITY_TIMINGS") };
        assert!(!timing::timings_enabled());
    });
}

#[test]
fn timings_enabled_parses_truthy_values() {
    with_env_lock(|| {
        for value in ["1", "true", "yes", "Y", "On"] {
            unsafe { std::env::set_var("HEADLAMP_PARITY_TIMINGS", value) };
            assert!(timing::timings_enabled(), "value={value:?}");
        }
    });
}

#[test]
fn timings_enabled_parses_falsy_values() {
    with_env_lock(|| {
        for value in ["0", "false", "no", "off", ""] {
            unsafe { std::env::set_var("HEADLAMP_PARITY_TIMINGS", value) };
            assert!(!timing::timings_enabled(), "value={value:?}");
        }
    });
}
