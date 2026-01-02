use std::path::Path;
use std::process::Command;
use std::time::Duration;

use headlamp::live_progress::{LiveProgress, LiveProgressMode};
use headlamp::streaming::{OutputStream, StreamAction, StreamAdapter};

#[derive(Default)]
struct CaptureAdapter {
    lines: Vec<String>,
}

impl StreamAdapter for CaptureAdapter {
    fn on_start(&mut self) -> Option<String> {
        None
    }

    fn on_line(&mut self, _stream: OutputStream, line: &str) -> Vec<StreamAction> {
        self.lines.push(line.to_string());
        vec![]
    }
}

#[cfg(unix)]
fn bash_command(script: &str) -> Command {
    let mut cmd = Command::new("bash");
    cmd.args(["-lc", script]);
    cmd
}

#[cfg(not(unix))]
fn bash_command(_script: &str) -> Command {
    // We only run these subprocess-based tests on unix-like platforms for now.
    // (The unsafe site we’re testing is unix-only.)
    Command::new("cmd")
}

#[cfg(unix)]
#[test]
fn merged_subprocess_does_not_hang_and_captures_all_lines_in_order() {
    // This test is intentionally subprocess-based: it catches real FD/EOF lifetime bugs where the
    // parent process accidentally keeps a write-end open and the reader never sees EOF.
    //
    // Emit interleaved stdout/stderr with deterministic sequencing.
    let mut cmd = bash_command(
        r#"
set -euo pipefail
printf 'A\n'
printf 'B\n' 1>&2
printf 'C\n'
printf 'D\n' 1>&2
"#,
    );
    cmd.current_dir(Path::new("."));

    let progress = LiveProgress::start(1, LiveProgressMode::Off);
    let mut adapter = CaptureAdapter::default();

    let (tx, rx) = std::sync::mpsc::channel::<(i32, Vec<String>)>();
    std::thread::spawn(move || {
        let res = headlamp::streaming::run_streaming_capture_tail_merged(
            cmd,
            &progress,
            &mut adapter,
            1024 * 1024,
        );
        progress.finish();
        let (code, _ring) = res.expect("run_streaming_capture_tail_merged");
        let _ = tx.send((code, adapter.lines));
    });

    let (exit, lines) = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("timed out waiting for merged streaming to finish");

    assert_eq!(exit, 0);
    assert_eq!(lines, vec!["A", "B", "C", "D"]);
}

#[cfg(unix)]
#[test]
fn merged_subprocess_child_exits_but_background_holds_fds_does_not_hang() {
    // Regression test for the classic EOF bug:
    // - child exits quickly
    // - a background process keeps stdout/stderr open
    // If capture waits for EOF, it can hang forever.
    let mut cmd = bash_command(
        r#"
set -euo pipefail
printf 'HELLO\n'
bash -c 'trap "" HUP; sleep 10' &
exit 0
"#,
    );
    cmd.current_dir(Path::new("."));

    let progress = LiveProgress::start(1, LiveProgressMode::Off);
    let mut adapter = CaptureAdapter::default();

    let (tx, rx) = std::sync::mpsc::channel::<(i32, Vec<String>)>();
    std::thread::spawn(move || {
        let res = headlamp::streaming::run_streaming_capture_tail_merged(
            cmd,
            &progress,
            &mut adapter,
            1024 * 1024,
        );
        progress.finish();
        let (code, _ring) = res.expect("run_streaming_capture_tail_merged");
        let _ = tx.send((code, adapter.lines));
    });

    let (exit, lines) = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("timed out waiting for merged streaming to finish");
    assert_eq!(exit, 0);
    assert!(lines.iter().any(|l| l == "HELLO"));
}

#[cfg(unix)]
#[test]
fn merged_subprocess_preserves_exit_code() {
    let mut cmd = bash_command(
        r#"
set -euo pipefail
printf 'hello\n'
printf 'world\n' 1>&2
exit 7
"#,
    );
    cmd.current_dir(Path::new("."));

    let progress = LiveProgress::start(1, LiveProgressMode::Off);
    let mut adapter = CaptureAdapter::default();

    let (tx, rx) = std::sync::mpsc::channel::<(i32, Vec<String>)>();
    std::thread::spawn(move || {
        let res = headlamp::streaming::run_streaming_capture_tail_merged(
            cmd,
            &progress,
            &mut adapter,
            1024 * 1024,
        );
        progress.finish();
        let (code, _ring) = res.expect("run_streaming_capture_tail_merged");
        let _ = tx.send((code, adapter.lines));
    });

    let (exit, lines) = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("timed out waiting for merged streaming to finish");

    assert_eq!(exit, 7);
    assert!(lines.iter().any(|l| l == "hello"));
    assert!(lines.iter().any(|l| l == "world"));
}

#[cfg(unix)]
#[test]
fn merged_subprocess_normalizes_crlf_and_keeps_last_line_without_newline() {
    let mut cmd = bash_command(
        r#"
set -euo pipefail
printf 'A\r\nB\r\n'
printf 'NO_NEWLINE'
exit 0
"#,
    );
    cmd.current_dir(Path::new("."));

    let progress = LiveProgress::start(1, LiveProgressMode::Off);
    let mut adapter = CaptureAdapter::default();

    let (tx, rx) = std::sync::mpsc::channel::<(i32, Vec<String>)>();
    std::thread::spawn(move || {
        let res = headlamp::streaming::run_streaming_capture_tail_merged(
            cmd,
            &progress,
            &mut adapter,
            1024 * 1024,
        );
        progress.finish();
        let (code, _ring) = res.expect("run_streaming_capture_tail_merged");
        let _ = tx.send((code, adapter.lines));
    });

    let (exit, lines) = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("timed out waiting for merged streaming to finish");

    assert_eq!(exit, 0);
    assert!(lines.iter().any(|l| l == "A"));
    assert!(lines.iter().any(|l| l == "B"));
    assert!(lines.iter().any(|l| l == "NO_NEWLINE"));
    assert!(!lines.iter().any(|l| l.ends_with('\r')));
}
