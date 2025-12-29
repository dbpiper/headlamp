use std::path::Path;

use headlamp::live_progress::{LiveProgress, LiveProgressMode};
use headlamp::streaming::{OutputStream, StreamAction, StreamAdapter, consume_lines_capture_tail};

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
fn merged_stdout_stderr_preserves_observed_order() {
    let repo_root = Path::new(".");
    let _ = repo_root;

    let (reader, writer) = os_pipe::pipe().expect("pipe");
    let mut writer2 = writer.try_clone().expect("clone");
    let mut writer1 = writer;

    use std::io::Write;
    writer1.write_all(b"A\n").expect("write A");
    writer2.write_all(b"B\n").expect("write B");
    writer1.write_all(b"C\n").expect("write C");
    writer2.write_all(b"D\n").expect("write D");
    drop(writer1);
    drop(writer2);

    let progress = LiveProgress::start(1, LiveProgressMode::Off);
    let mut adapter = CaptureAdapter::default();
    let _ring = consume_lines_capture_tail(
        std::io::BufReader::new(reader),
        &progress,
        &mut adapter,
        1024 * 1024,
    );
    progress.finish();

    assert_eq!(
        adapter.lines,
        vec![
            "A".to_string(),
            "B".to_string(),
            "C".to_string(),
            "D".to_string()
        ]
    );
}
