use std::process::{Command, Stdio};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::run::RunError;
use wait_timeout::ChildExt;

#[derive(Debug)]
pub struct CapturedProcessOutput {
    pub status: std::process::ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

fn join_capture_thread(
    handle: Option<JoinHandle<Result<Vec<u8>, std::io::Error>>>,
) -> Result<Vec<u8>, RunError> {
    let Some(handle) = handle else {
        return Ok(vec![]);
    };
    handle
        .join()
        .map_err(|_| RunError::Io(std::io::Error::other("capture thread panicked")))?
        .map_err(RunError::Io)
}

fn spawn_capture_thread(
    reader: Option<impl std::io::Read + Send + 'static>,
) -> Option<JoinHandle<Result<Vec<u8>, std::io::Error>>> {
    reader.map(|mut r| {
        std::thread::spawn(move || {
            let mut buf: Vec<u8> = vec![];
            r.read_to_end(&mut buf)?;
            Ok(buf)
        })
    })
}

pub fn run_command_capture_with_timeout(
    mut command: Command,
    display_command: String,
    timeout: Duration,
) -> Result<CapturedProcessOutput, RunError> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(RunError::SpawnFailed)?;

    let stdout_thread = spawn_capture_thread(child.stdout.take());
    let stderr_thread = spawn_capture_thread(child.stderr.take());

    let maybe_status = ChildExt::wait_timeout(&mut child, timeout).map_err(RunError::WaitFailed)?;
    let Some(status) = maybe_status else {
        let _ = child.kill();
        let _ = child.wait();
        let _ = join_capture_thread(stdout_thread);
        let _ = join_capture_thread(stderr_thread);
        return Err(RunError::TimedOut {
            command: display_command,
            timeout_ms: timeout.as_millis() as u64,
        });
    };

    let stdout = join_capture_thread(stdout_thread)?;
    let stderr = join_capture_thread(stderr_thread)?;
    Ok(CapturedProcessOutput {
        status,
        stdout,
        stderr,
    })
}
