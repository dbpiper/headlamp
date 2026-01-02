use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::run::RunError;
use wait_timeout::ChildExt;

#[derive(Debug)]
pub struct CapturedProcessOutput {
    pub status: std::process::ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

fn drain_after_exit_deadline(now: Instant) -> Instant {
    now + Duration::from_millis(250)
}

fn spawn_capture_receiver(
    reader: Option<impl std::io::Read + Send + 'static>,
) -> Option<std::sync::mpsc::Receiver<Vec<u8>>> {
    let mut reader = reader?;
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 16 * 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => return,
                Ok(n) => {
                    let _ = tx.send(buf[..n].to_vec());
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return,
            }
        }
    });
    Some(rx)
}

fn drain_receiver_until_deadline(
    receiver: Option<std::sync::mpsc::Receiver<Vec<u8>>>,
    deadline: Instant,
) -> Vec<u8> {
    let Some(receiver) = receiver else {
        return vec![];
    };
    let mut out: Vec<u8> = Vec::new();
    loop {
        let now = Instant::now();
        if now >= deadline {
            return out;
        }
        let remaining = deadline.duration_since(now);
        match receiver.recv_timeout(remaining) {
            Ok(chunk) => out.extend_from_slice(&chunk),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => return out,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return out,
        }
    }
}

pub fn run_command_capture_with_timeout(
    mut command: Command,
    display_command: String,
    timeout: Duration,
) -> Result<CapturedProcessOutput, RunError> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(RunError::SpawnFailed)?;

    let stdout_receiver = spawn_capture_receiver(child.stdout.take());
    let stderr_receiver = spawn_capture_receiver(child.stderr.take());

    let maybe_status = ChildExt::wait_timeout(&mut child, timeout).map_err(RunError::WaitFailed)?;
    let Some(status) = maybe_status else {
        let _ = child.kill();
        let _ = child.wait();
        let deadline = drain_after_exit_deadline(Instant::now());
        let _ = drain_receiver_until_deadline(stdout_receiver, deadline);
        let _ = drain_receiver_until_deadline(stderr_receiver, deadline);
        return Err(RunError::TimedOut {
            command: display_command,
            timeout_ms: timeout.as_millis() as u64,
        });
    };

    let deadline = drain_after_exit_deadline(Instant::now());
    let stdout = drain_receiver_until_deadline(stdout_receiver, deadline);
    let stderr = drain_receiver_until_deadline(stderr_receiver, deadline);
    Ok(CapturedProcessOutput {
        status,
        stdout,
        stderr,
    })
}
