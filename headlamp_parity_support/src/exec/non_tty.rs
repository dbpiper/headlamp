use std::process::Command;
use std::time::Duration;

pub fn run_cmd(mut cmd: Command) -> (i32, String) {
    let out = cmd.output().unwrap();
    let code = out.status.code().unwrap_or(1);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{stdout}{stderr}");
    let combined = combined.replace("\u{1b}[2K\rRUN ", "");
    let combined = combined.replace("\u{1b}[2K\r", "");
    (code, combined)
}

pub fn run_cmd_with_timeout(mut cmd: Command, timeout: Duration) -> (i32, String) {
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().expect("spawn");
    // Drain stdout/stderr concurrently while waiting; otherwise the child can block on a full
    // pipe buffer and appear “hung”.
    let stdout_handle = child.stdout.take().map(|mut out| {
        std::thread::spawn(move || {
            let mut buf: Vec<u8> = vec![];
            let _ = std::io::Read::read_to_end(&mut out, &mut buf);
            buf
        })
    });
    let stderr_handle = child.stderr.take().map(|mut err| {
        std::thread::spawn(move || {
            let mut buf: Vec<u8> = vec![];
            let _ = std::io::Read::read_to_end(&mut err, &mut buf);
            buf
        })
    });

    let status = wait_timeout::ChildExt::wait_timeout(&mut child, timeout).expect("wait_timeout");
    if status.is_none() {
        let _ = child.kill();
        let _ = child.wait();
        let stdout_bytes = stdout_handle
            .and_then(|h| h.join().ok())
            .unwrap_or_default();
        let stderr_bytes = stderr_handle
            .and_then(|h| h.join().ok())
            .unwrap_or_default();
        let stdout = String::from_utf8_lossy(&stdout_bytes);
        let stderr = String::from_utf8_lossy(&stderr_bytes);
        let combined = format!("{stdout}{stderr}")
            .replace("\u{1b}[2K\rRUN ", "")
            .replace("\u{1b}[2K\r", "");
        return (
            124,
            format!(
                "{}[headlamp_parity_support] timeout after {}s (killed)\n",
                combined,
                timeout.as_secs()
            ),
        );
    }

    let code = status.and_then(|s| s.code()).unwrap_or(1);
    let stdout_bytes = stdout_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();
    let stderr_bytes = stderr_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();
    let stdout = String::from_utf8_lossy(&stdout_bytes);
    let stderr = String::from_utf8_lossy(&stderr_bytes);
    let combined = format!("{stdout}{stderr}")
        .replace("\u{1b}[2K\rRUN ", "")
        .replace("\u{1b}[2K\r", "");
    (code, combined)
}
