use std::process::Command;
use std::time::Duration;

#[cfg(unix)]
fn bash(script: &str) -> Command {
    let mut cmd = Command::new("bash");
    cmd.args(["-lc", script]);
    cmd
}

#[cfg(not(unix))]
fn bash(_script: &str) -> Command {
    Command::new("cmd")
}

#[cfg(unix)]
#[test]
fn process_capture_with_timeout_does_not_hang_when_background_holds_fds() {
    // Regression test: if the child exits but a background process inherits stdout/stderr,
    // capture threads must not block forever waiting for EOF.
    let cmd = bash(
        r#"
set -euo pipefail
printf 'X\n'
( trap "" HUP; sleep 10 ) &
exit 0
"#,
    );

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        let out = headlamp::process::run_command_capture_with_timeout(
            cmd,
            "bash -lc <script>".to_string(),
            Duration::from_secs(2),
        )
        .expect("run_command_capture_with_timeout");
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let _ = tx.send(stdout);
    });

    let stdout = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("timed out waiting for process capture to finish");
    assert!(stdout.contains("X\n"));
}
