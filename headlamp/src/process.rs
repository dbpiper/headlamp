use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tempfile::NamedTempFile;

use crate::run::RunError;

#[derive(Debug)]
pub struct CapturedProcessOutput {
    pub status: std::process::ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub fn run_command_capture_with_timeout(
    mut command: Command,
    display_command: String,
    timeout: Duration,
) -> Result<CapturedProcessOutput, RunError> {
    let stdout_file = NamedTempFile::new().map_err(RunError::Io)?;
    let stderr_file = NamedTempFile::new().map_err(RunError::Io)?;

    let stdout_handle = stdout_file.reopen().map_err(RunError::Io)?;
    let stderr_handle = stderr_file.reopen().map_err(RunError::Io)?;

    command
        .stdout(Stdio::from(stdout_handle))
        .stderr(Stdio::from(stderr_handle));

    let mut child = command.spawn().map_err(RunError::SpawnFailed)?;

    let started_at = Instant::now();
    loop {
        if started_at.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(RunError::TimedOut {
                command: display_command,
                timeout_ms: timeout.as_millis() as u64,
            });
        }

        match child.try_wait().map_err(RunError::WaitFailed)? {
            Some(status) => {
                let stdout = std::fs::read(stdout_file.path()).map_err(RunError::Io)?;
                let stderr = std::fs::read(stderr_file.path()).map_err(RunError::Io)?;
                return Ok(CapturedProcessOutput {
                    status,
                    stdout,
                    stderr,
                });
            }
            None => std::thread::sleep(Duration::from_millis(25)),
        }
    }
}
