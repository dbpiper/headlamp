mod classify;
mod frame;
mod ticker;

pub use classify::classify_runner_line_for_progress;
pub use frame::{RenderRunFrameArgs, render_run_frame, render_run_frame_with_columns};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveProgressMode {
    Off,
    Plain,
    Interactive,
}

pub struct LiveProgress {
    pub(super) mode: LiveProgressMode,
    pub(super) stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(super) done_units: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    pub(super) current_label: std::sync::Arc<std::sync::Mutex<String>>,
    pub(super) last_event_at: std::sync::Arc<std::sync::Mutex<std::time::Instant>>,
    pub(super) last_runner_stdout_hint: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    pub(super) last_runner_stderr_hint: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    pub(super) last_frame_lines: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    pub(super) write_lock: std::sync::Arc<std::sync::Mutex<()>>,
    pub(super) ticker: Option<std::thread::JoinHandle<()>>,
}

pub fn live_progress_mode_with_env_ci(
    stdout_is_tty: bool,
    ci: bool,
    env_ci: bool,
) -> LiveProgressMode {
    if ci || env_ci {
        return LiveProgressMode::Plain;
    }
    if stdout_is_tty && !env_ci {
        return LiveProgressMode::Interactive;
    }
    LiveProgressMode::Off
}

pub fn live_progress_mode(stdout_is_tty: bool, ci: bool) -> LiveProgressMode {
    let env_ci = std::env::var("CI").ok().is_some();
    live_progress_mode_with_env_ci(stdout_is_tty, ci, env_ci)
}
