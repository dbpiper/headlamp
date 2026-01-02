use std::io::Cursor;
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

#[test]
fn consume_lines_capture_tail_normalizes_crlf_and_limits_tail_bytes() {
    let input = b"A\r\nBB\r\nCCC\r\n";
    let reader = Cursor::new(input.as_slice());
    let progress = LiveProgress::start(1, LiveProgressMode::Off);
    let mut adapter = CaptureAdapter::default();

    let ring = headlamp::streaming::consume_lines_capture_tail(reader, &progress, &mut adapter, 4);
    progress.finish();

    assert_eq!(adapter.lines, vec!["A", "BB", "CCC"]);

    // With a 4-byte ring buffer and lines sized 1,2,3 bytes respectively, we should retain only
    // the last line ("CCC" = 3 bytes). Pushing "BB" then "CCC" would exceed 4, so "BB" is evicted.
    let tail = ring.lines().cloned().collect::<Vec<_>>();
    assert_eq!(tail, vec!["CCC"]);
}

#[test]
fn consume_lines_capture_tail_keeps_last_line_without_newline() {
    let input = b"NO_NEWLINE";
    let reader = Cursor::new(input.as_slice());
    let progress = LiveProgress::start(1, LiveProgressMode::Off);
    let mut adapter = CaptureAdapter::default();

    let ring =
        headlamp::streaming::consume_lines_capture_tail(reader, &progress, &mut adapter, 1024);
    progress.finish();

    assert_eq!(adapter.lines, vec!["NO_NEWLINE"]);
    let tail = ring.lines().cloned().collect::<Vec<_>>();
    assert_eq!(tail, vec!["NO_NEWLINE"]);
}

#[test]
fn consume_lines_capture_tail_does_not_hang_on_empty_input() {
    let reader = Cursor::new(b"".as_slice());
    let progress = LiveProgress::start(1, LiveProgressMode::Off);
    let mut adapter = CaptureAdapter::default();

    let (tx, rx) = std::sync::mpsc::channel::<usize>();
    std::thread::spawn(move || {
        let ring =
            headlamp::streaming::consume_lines_capture_tail(reader, &progress, &mut adapter, 1024);
        progress.finish();
        let _ = tx.send(ring.lines().count());
    });

    let line_count = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("timed out waiting for consume_lines_capture_tail to finish");
    assert_eq!(line_count, 0);
}
