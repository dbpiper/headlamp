use std::path::Path;

use duct::cmd as duct_cmd;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RunError {
    #[error("missing runner: {runner} ({hint})")]
    MissingRunner { runner: String, hint: String },

    #[error("Error: {message}")]
    CommandFailed { message: String },

    #[error("command timed out after {timeout_ms}ms: {command}")]
    TimedOut { command: String, timeout_ms: u64 },

    #[error("failed to spawn process: {0}")]
    SpawnFailed(std::io::Error),

    #[error("failed to wait on process: {0}")]
    WaitFailed(std::io::Error),

    #[error("io error: {0}")]
    Io(std::io::Error),

    #[error("bootstrap failed: {command}")]
    BootstrapFailed { command: String },
}

pub fn run_bootstrap(repo_root: &Path, raw: &str) -> Result<(), RunError> {
    let raw_cmd = raw.trim();
    if raw_cmd.is_empty() {
        return Ok(());
    }

    let status = if raw_cmd.contains(char::is_whitespace) {
        if cfg!(windows) {
            duct_cmd("cmd.exe", ["/d", "/s", "/c", raw_cmd])
                .dir(repo_root)
                .unchecked()
                .run()
        } else {
            duct_cmd("bash", ["-lc", raw_cmd])
                .dir(repo_root)
                .unchecked()
                .run()
        }
    } else {
        let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
        duct_cmd(npm, ["run", "-s", raw_cmd])
            .dir(repo_root)
            .unchecked()
            .run()
    }
    .map_err(|e| {
        RunError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))
    })?;

    status
        .status
        .success()
        .then_some(())
        .ok_or(RunError::BootstrapFailed {
            command: raw_cmd.to_string(),
        })
}
