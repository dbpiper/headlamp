use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use crate::hashing::next_capture_id;

use super::portable_pty::run_cmd_tty_portable_pty;
use super::shell::{build_tty_shell_command, build_tty_shell_command_stdout_redirect};

pub fn run_cmd_tty(mut cmd: Command, columns: usize) -> (i32, String) {
    let _timing = crate::timing::TimingGuard::start("tty_run");
    cmd.env("TERM", "xterm-256color");
    cmd.env("FORCE_COLOR", "1");
    cmd.env("CI", "1");
    cmd.env_remove("NO_COLOR");

    if let Some((code, out)) = run_cmd_tty_portable_pty(&cmd, columns, Duration::from_secs(60)) {
        return (code, out);
    }

    let mut script = Command::new("script");
    let capture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("parity-fixtures")
        .join(format!(
            "tty-capture-{}-{}.txt",
            std::process::id(),
            next_capture_id()
        ));
    let _ = std::fs::remove_file(&capture_path);
    script.arg("-q").arg(&capture_path);
    script
        .arg("sh")
        .arg("-lc")
        .arg(build_tty_shell_command(&cmd, columns));
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

    let timeout = std::time::Duration::from_secs(60);
    script.stderr(std::process::Stdio::piped());
    let mut child = script.spawn().unwrap();
    let status = wait_timeout::ChildExt::wait_timeout(&mut child, timeout).unwrap();
    let (code, stderr_text) = match status {
        Some(s) => (
            s.code().unwrap_or(1),
            child
                .stderr
                .take()
                .and_then(|mut stderr| {
                    let mut buf: Vec<u8> = vec![];
                    std::io::Read::read_to_end(&mut stderr, &mut buf).ok()?;
                    Some(String::from_utf8_lossy(&buf).to_string())
                })
                .unwrap_or_default(),
        ),
        None => {
            let _ = child.kill();
            let _ = child.wait();
            (
                124,
                format!(
                    "[headlamp_parity_support] timeout after {}s (killed)\n",
                    timeout.as_secs()
                ),
            )
        }
    };
    let bytes = std::fs::read(&capture_path).unwrap_or_default();
    let mut combined = String::from_utf8_lossy(&bytes).to_string();
    combined.push_str(&stderr_text);
    let combined = combined
        .replace(['\u{0008}', '\u{0004}'], "")
        .replace("^D", "");
    let _ = std::fs::remove_file(&capture_path);
    (code, combined)
}

pub fn run_cmd_tty_stdout_piped(mut cmd: Command, columns: usize) -> (i32, String) {
    let _timing = crate::timing::TimingGuard::start("tty_stdout_piped_run");
    cmd.env("TERM", "xterm-256color");
    cmd.env("CI", "1");
    cmd.env_remove("NO_COLOR");
    cmd.env_remove("FORCE_COLOR");

    if let Some((code, out)) = run_cmd_tty_portable_pty(&cmd, columns, Duration::from_secs(60)) {
        return (code, out);
    }

    let stdout_capture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("parity-fixtures")
        .join(format!(
            "tty-stdout-capture-{}-{}.txt",
            std::process::id(),
            next_capture_id()
        ));
    let _ = std::fs::remove_file(&stdout_capture_path);

    let mut script = Command::new("script");
    let tty_capture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("parity-fixtures")
        .join(format!(
            "tty-capture-{}-{}.txt",
            std::process::id(),
            next_capture_id()
        ));
    let _ = std::fs::remove_file(&tty_capture_path);
    script.arg("-q").arg(&tty_capture_path);
    script
        .arg("sh")
        .arg("-lc")
        .arg(build_tty_shell_command_stdout_redirect(
            &cmd,
            columns,
            &stdout_capture_path,
        ));
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

    let timeout = std::time::Duration::from_secs(60);
    script.stderr(std::process::Stdio::piped());
    let mut child = script.spawn().unwrap();
    let status = wait_timeout::ChildExt::wait_timeout(&mut child, timeout).unwrap();
    let (code, stderr_text) = match status {
        Some(s) => (
            s.code().unwrap_or(1),
            child
                .stderr
                .take()
                .and_then(|mut stderr| {
                    let mut buf: Vec<u8> = vec![];
                    std::io::Read::read_to_end(&mut stderr, &mut buf).ok()?;
                    Some(String::from_utf8_lossy(&buf).to_string())
                })
                .unwrap_or_default(),
        ),
        None => {
            let _ = child.kill();
            let _ = child.wait();
            (
                124,
                format!(
                    "[headlamp_parity_support] timeout after {}s (killed)\n",
                    timeout.as_secs()
                ),
            )
        }
    };
    let mut combined = String::new();
    combined.push_str(&String::from_utf8_lossy(
        &std::fs::read(&stdout_capture_path).unwrap_or_default(),
    ));
    combined.push_str(&String::from_utf8_lossy(
        &std::fs::read(&tty_capture_path).unwrap_or_default(),
    ));
    combined.push_str(&stderr_text);
    let combined = combined
        .replace(['\u{0008}', '\u{0004}'], "")
        .replace("^D", "");
    let _ = std::fs::remove_file(&stdout_capture_path);
    let _ = std::fs::remove_file(&tty_capture_path);
    (code, combined)
}
