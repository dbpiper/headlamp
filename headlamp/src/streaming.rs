use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;

use crate::live_progress::LiveProgress;
use crate::run::RunError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone)]
pub enum StreamAction {
    PrintStdout(String),
    PrintStderr(String),
    SetProgressLabel(String),
}

#[derive(Debug, Clone)]
pub struct RingBuffer {
    max_bytes: usize,
    current_bytes: usize,
    lines: VecDeque<String>,
}

impl RingBuffer {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes: max_bytes.max(1),
            current_bytes: 0,
            lines: VecDeque::new(),
        }
    }

    pub fn push_line(&mut self, line: String) {
        let bytes = line.len();
        self.lines.push_back(line);
        self.current_bytes = self.current_bytes.saturating_add(bytes);
        while self.current_bytes > self.max_bytes {
            let Some(front) = self.lines.pop_front() else {
                break;
            };
            self.current_bytes = self.current_bytes.saturating_sub(front.len());
        }
    }

    pub fn lines(&self) -> impl Iterator<Item = &String> {
        self.lines.iter()
    }
}

pub trait StreamAdapter {
    fn on_start(&mut self) -> Option<String>;

    fn on_line(&mut self, stream: OutputStream, line: &str) -> Vec<StreamAction>;
}

fn apply_actions(progress: &LiveProgress, actions: Vec<StreamAction>) {
    actions.into_iter().for_each(|action| match action {
        StreamAction::SetProgressLabel(label) => progress.set_current_label(label),
        StreamAction::PrintStdout(line) => progress.println_stdout(&line),
        StreamAction::PrintStderr(line) => progress.eprintln_stderr(&line),
    });
}

pub fn run_streaming_capture_tail(
    mut command: Command,
    progress: &LiveProgress,
    adapter: &mut dyn StreamAdapter,
    ring_bytes: usize,
) -> Result<(i32, RingBuffer), RunError> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(RunError::SpawnFailed)?;

    if let Some(label) = adapter.on_start() {
        progress.set_current_label(label);
    }

    let (tx, rx) = mpsc::channel::<(OutputStream, String)>();

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // Reader threads (stdout + stderr).
    // We read via BufRead::lines so we can incrementally feed the adapter.
    if let Some(out) = stdout {
        let tx_out = tx.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(out);
            reader
                .lines()
                .filter_map(Result::ok)
                .for_each(|line| {
                    let _ = tx_out.send((OutputStream::Stdout, line));
                });
        });
    }

    if let Some(err) = stderr {
        let tx_err = tx.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(err);
            reader
                .lines()
                .filter_map(Result::ok)
                .for_each(|line| {
                    let _ = tx_err.send((OutputStream::Stderr, line));
                });
        });
    }

    drop(tx);

    let mut ring = RingBuffer::new(ring_bytes);
    rx.iter().for_each(|(stream, line)| {
        ring.push_line(line.clone());
        let actions = adapter.on_line(stream, &line);
        apply_actions(progress, actions);
    });

    let status = child.wait().map_err(RunError::WaitFailed)?;
    let exit_code = status.code().unwrap_or(1);
    Ok((exit_code, ring))
}


