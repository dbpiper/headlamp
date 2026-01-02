use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use crate::hashing::next_capture_id;

use super::portable_pty::run_cmd_tty_portable_pty;
use super::shell::{build_tty_shell_command, build_tty_shell_command_stdout_redirect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtyBackend {
    PortablePty,
    Script,
}

impl std::fmt::Display for TtyBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TtyBackend::PortablePty => write!(f, "portable_pty"),
            TtyBackend::Script => write!(f, "script"),
        }
    }
}

pub fn run_cmd_tty_with_backend(mut cmd: Command, columns: usize) -> (i32, String, TtyBackend) {
    let _timing = crate::timing::TimingGuard::start("tty_run");
    apply_tty_env(&mut cmd, true);

    // Prefer the portable PTY implementation for stability across platforms/CI images.
    // (Different `script(1)` implementations and PTY behaviors can subtly change runner output.)
    let portable = run_cmd_tty_portable_pty(&cmd, columns, tty_timeout());
    if let Some((code, out)) = portable {
        return (code, sanitize_tty_output(out), TtyBackend::PortablePty);
    }

    let tty_capture_path = capture_path("tty-capture");
    let shell_cmd = build_tty_shell_command(&cmd, columns);
    let script = build_script_command(&cmd, &tty_capture_path, shell_cmd);
    let Some((code, stderr_text)) = run_script_capture_stderr(script, tty_timeout()) else {
        // `portable_pty` was already attempted above; if we get here and `script` failed to run,
        // we have no other fallback.
        return (1, String::new(), TtyBackend::Script);
    };
    let combined = sanitize_tty_output(format!("{}{}", read_lossy(&tty_capture_path), stderr_text));
    let _ = std::fs::remove_file(&tty_capture_path);
    (code, combined, TtyBackend::Script)
}

pub fn run_cmd_tty_with_backend_timeout(
    mut cmd: Command,
    columns: usize,
    timeout: Duration,
) -> (i32, String, TtyBackend) {
    let _timing = crate::timing::TimingGuard::start("tty_run_timeout");
    apply_tty_env(&mut cmd, true);

    let portable = run_cmd_tty_portable_pty(&cmd, columns, timeout);
    if let Some((code, out)) = portable {
        return (code, sanitize_tty_output(out), TtyBackend::PortablePty);
    }

    let tty_capture_path = capture_path("tty-capture");
    let shell_cmd = build_tty_shell_command(&cmd, columns);
    let script = build_script_command(&cmd, &tty_capture_path, shell_cmd);
    let Some((code, stderr_text)) = run_script_capture_stderr(script, timeout) else {
        return (1, String::new(), TtyBackend::Script);
    };
    let combined = sanitize_tty_output(format!("{}{}", read_lossy(&tty_capture_path), stderr_text));
    let _ = std::fs::remove_file(&tty_capture_path);
    (code, combined, TtyBackend::Script)
}

pub fn run_cmd_tty(cmd: Command, columns: usize) -> (i32, String) {
    let (code, out, _backend) = run_cmd_tty_with_backend(cmd, columns);
    (code, out)
}

pub fn run_cmd_tty_stdout_piped(mut cmd: Command, columns: usize) -> (i32, String) {
    let _timing = crate::timing::TimingGuard::start("tty_stdout_piped_run");
    apply_tty_env(&mut cmd, false);

    let stdout_capture_path = capture_path("tty-stdout-capture");
    let tty_capture_path = capture_path("tty-capture");
    let shell_cmd = build_tty_shell_command_stdout_redirect(&cmd, columns, &stdout_capture_path);
    let script = build_script_command(&cmd, &tty_capture_path, shell_cmd);
    let Some((code, stderr_text)) = run_script_capture_stderr(script, tty_timeout()) else {
        return run_cmd_tty_portable_pty(&cmd, columns, tty_timeout())
            .unwrap_or((1, String::new()));
    };
    // If `script` itself failed due to unsupported flags/options, fall back to the portable PTY.
    if code != 0
        && stderr_text.contains("script:")
        && (stderr_text.contains("illegal option") || stderr_text.contains("invalid option"))
    {
        return run_cmd_tty_portable_pty(&cmd, columns, tty_timeout()).unwrap_or((1, stderr_text));
    }
    let combined = sanitize_tty_output(format!(
        "{}{}{}",
        read_lossy(&stdout_capture_path),
        read_lossy(&tty_capture_path),
        stderr_text
    ));
    let _ = std::fs::remove_file(&stdout_capture_path);
    let _ = std::fs::remove_file(&tty_capture_path);
    (code, combined)
}

fn apply_tty_env(cmd: &mut Command, force_color: bool) {
    cmd.env("TERM", "xterm-256color");
    cmd.env("CI", "1");
    cmd.env_remove("NO_COLOR");
    if force_color {
        cmd.env("FORCE_COLOR", "1");
    } else {
        cmd.env_remove("FORCE_COLOR");
    }
}

fn tty_timeout() -> Duration {
    let seconds = std::env::var("HEADLAMP_PARITY_TTY_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(60);
    Duration::from_secs(seconds.clamp(30, 600))
}

fn capture_path(prefix: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("parity-fixtures")
        .join(format!(
            "{}-{}-{}.txt",
            prefix,
            std::process::id(),
            next_capture_id()
        ));
    let _ = std::fs::remove_file(&path);
    path
}

fn build_script_command(cmd: &Command, capture_path: &PathBuf, shell_command: String) -> Command {
    let mut script = Command::new("script");
    // We need to support both:
    // - GNU util-linux `script` (Linux): use `-c` to run a command under a PTY.
    // - BSD `script` (macOS): does NOT support `-c`; instead it accepts a command + args
    //   after the output file path.
    //
    // If we try the BSD style on Linux, `-lc` (intended for `sh`) may be parsed as a `script`
    // option and fail ("invalid option -- 'l'"). So we branch by OS.
    if cfg!(target_os = "macos") {
        script.arg("-q").arg(capture_path);
        script.arg("sh").arg("-lc").arg(shell_command);
    } else {
        script
            .arg("-q")
            .arg("-c")
            .arg(shell_command)
            .arg(capture_path);
    }
    script.stdout(std::process::Stdio::null());
    script.current_dir(
        cmd.get_current_dir()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".")),
    );
    cmd.get_envs().for_each(|(key, value)| match value {
        Some(v) => {
            script.env(key, v);
        }
        None => {
            script.env_remove(key);
        }
    });
    script
}

fn run_script_capture_stderr(mut script: Command, timeout: Duration) -> Option<(i32, String)> {
    script.stderr(std::process::Stdio::piped());
    let mut child = script.spawn().ok()?;
    let status = wait_timeout::ChildExt::wait_timeout(&mut child, timeout).ok()?;
    match status {
        Some(s) => Some((s.code().unwrap_or(1), read_child_stderr(&mut child))),
        None => {
            let _ = child.kill();
            let _ = child.wait();
            Some((
                124,
                format!(
                    "[headlamp_parity_support] timeout after {}s (killed)\n",
                    timeout.as_secs()
                ),
            ))
        }
    }
}

fn read_child_stderr(child: &mut std::process::Child) -> String {
    child
        .stderr
        .take()
        .and_then(|mut stderr| {
            let mut buf: Vec<u8> = vec![];
            std::io::Read::read_to_end(&mut stderr, &mut buf).ok()?;
            Some(String::from_utf8_lossy(&buf).to_string())
        })
        .unwrap_or_default()
}

fn read_lossy(path: &PathBuf) -> String {
    let bytes = std::fs::read(path).unwrap_or_default();
    String::from_utf8_lossy(&bytes).to_string()
}

fn sanitize_tty_output(raw: String) -> String {
    raw.replace(['\u{0008}', '\u{0004}'], "").replace("^D", "")
}
