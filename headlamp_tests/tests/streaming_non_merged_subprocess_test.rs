use std::path::Path;
use std::process::Command;
use std::time::Duration;

use headlamp::live_progress::{LiveProgress, LiveProgressMode};
use headlamp::streaming::{OutputStream, StreamAction, StreamAdapter};

#[derive(Default)]
struct CaptureAdapter {
    lines: Vec<(OutputStream, String)>,
}

impl StreamAdapter for CaptureAdapter {
    fn on_start(&mut self) -> Option<String> {
        None
    }

    fn on_line(&mut self, stream: OutputStream, line: &str) -> Vec<StreamAction> {
        self.lines.push((stream, line.to_string()));
        vec![]
    }
}

#[cfg(unix)]
fn bash(script: &str) -> Command {
    let mut cmd = Command::new("bash");
    cmd.args(["-lc", script]);
    cmd
}

#[cfg(unix)]
#[test]
fn non_merged_subprocess_no_output_exits_and_does_not_hang() {
    // This catches the classic EOF bug: if the parent accidentally keeps a pipe write end open,
    // reading `stdout`/`stderr` can block forever when the child produces no output.
    let mut cmd = bash("exit 0");
    cmd.current_dir(Path::new("."));

    let progress = LiveProgress::start(1, LiveProgressMode::Off);
    let mut adapter = CaptureAdapter::default();

    let (tx, rx) = std::sync::mpsc::channel::<i32>();
    std::thread::spawn(move || {
        let res = headlamp::streaming::run_streaming_capture_tail(
            cmd,
            &progress,
            &mut adapter,
            1024 * 1024,
        );
        progress.finish();
        let (code, _ring) = res.expect("run_streaming_capture_tail");
        let _ = tx.send(code);
    });

    let exit = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("timed out waiting for non-merged streaming to finish");
    assert_eq!(exit, 0);
}

#[cfg(unix)]
#[test]
fn non_merged_subprocess_captures_trailing_background_output_after_child_exit() {
    // Regression test: if the child exits quickly but a background process writes shortly after,
    // we still want to capture that trailing output (within a small drain window) without hanging.
    let mut cmd = bash(
        r#"
set -euo pipefail
printf 'FIRST\n'
( sleep 0.10; printf 'LATE\n' ) &
exit 0
"#,
    );
    cmd.current_dir(Path::new("."));

    let progress = LiveProgress::start(1, LiveProgressMode::Off);
    let mut adapter = CaptureAdapter::default();

    let (tx, rx) = std::sync::mpsc::channel::<(i32, Vec<(OutputStream, String)>)>();
    std::thread::spawn(move || {
        let res = headlamp::streaming::run_streaming_capture_tail(
            cmd,
            &progress,
            &mut adapter,
            1024 * 1024,
        );
        progress.finish();
        let (code, _ring) = res.expect("run_streaming_capture_tail");
        let _ = tx.send((code, adapter.lines));
    });

    let (exit, lines) = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("timed out waiting for non-merged streaming to finish");
    assert_eq!(exit, 0);

    let texts = lines
        .into_iter()
        .map(|(_stream, line)| line)
        .collect::<Vec<_>>();
    assert!(texts.iter().any(|l| l == "FIRST"));
    assert!(texts.iter().any(|l| l == "LATE"));
}
