use crate::live_progress::{LiveProgressMode, live_progress_mode_with_env_ci};

#[test]
fn live_progress_mode_ci_flag_is_plain() {
    assert_eq!(
        live_progress_mode_with_env_ci(false, true, false, false),
        LiveProgressMode::Plain
    );
    assert_eq!(
        live_progress_mode_with_env_ci(true, true, false, false),
        LiveProgressMode::Plain
    );
}

#[test]
fn live_progress_mode_env_ci_is_plain() {
    assert_eq!(
        live_progress_mode_with_env_ci(false, false, true, false),
        LiveProgressMode::Plain
    );
    assert_eq!(
        live_progress_mode_with_env_ci(true, false, true, false),
        LiveProgressMode::Plain
    );
}

#[test]
fn live_progress_mode_tty_non_ci_is_interactive() {
    assert_eq!(
        live_progress_mode_with_env_ci(true, false, false, false),
        LiveProgressMode::Interactive
    );
}

#[test]
fn live_progress_mode_non_tty_non_ci_is_plain() {
    assert_eq!(
        live_progress_mode_with_env_ci(false, false, false, false),
        LiveProgressMode::Plain
    );
}
