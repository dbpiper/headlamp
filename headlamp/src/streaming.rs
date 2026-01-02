use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

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

fn drain_after_child_exit_deadline(now: Instant) -> Instant {
    now + Duration::from_millis(250)
}

fn recv_poll_interval() -> Duration {
    Duration::from_millis(50)
}

fn normalize_crlf_line(line: &str) -> String {
    line.strip_suffix('\r').unwrap_or(line).to_string()
}

fn spawn_lines_thread(
    reader: impl std::io::Read + Send + 'static,
    tx: mpsc::Sender<(OutputStream, String)>,
    stream: OutputStream,
) {
    std::thread::spawn(move || {
        let reader = BufReader::new(reader);
        reader.lines().map_while(Result::ok).for_each(|line| {
            let line = normalize_crlf_line(&line);
            let _ = tx.send((stream, line));
        });
    });
}

fn drain_channel_until_exit_then_deadline(
    mut child: std::process::Child,
    rx: mpsc::Receiver<(OutputStream, String)>,
    ring_bytes: usize,
    mut on_line: impl FnMut(OutputStream, &str, &mut RingBuffer),
) -> Result<(i32, RingBuffer), RunError> {
    let mut ring = RingBuffer::new(ring_bytes);
    let mut child_exited = false;
    let mut drain_deadline: Option<Instant> = None;
    loop {
        match rx.recv_timeout(recv_poll_interval()) {
            Ok((stream, line)) => on_line(stream, &line, &mut ring),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                let now = Instant::now();
                if child_exited {
                    if drain_deadline.is_some_and(|deadline| now >= deadline) {
                        break;
                    }
                    continue;
                }
                if child.try_wait().map_err(RunError::WaitFailed)?.is_some() {
                    child_exited = true;
                    drain_deadline = Some(drain_after_child_exit_deadline(now));
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let status = child.wait().map_err(RunError::WaitFailed)?;
    let exit_code = status.code().unwrap_or(1);
    Ok((exit_code, ring))
}

#[doc(hidden)]
pub fn consume_lines_capture_tail(
    reader: impl BufRead,
    progress: &LiveProgress,
    adapter: &mut dyn StreamAdapter,
    ring_bytes: usize,
) -> RingBuffer {
    let mut ring = RingBuffer::new(ring_bytes);
    reader.lines().map_while(Result::ok).for_each(|line| {
        // Normalize CRLF -> LF. BufRead::lines strips '\n' but keeps a trailing '\r' if present.
        let line = normalize_crlf_line(&line);
        ring.push_line(line.clone());
        // Once merged, stream distinction is no longer meaningful.
        progress.record_runner_stdout_line(&line);
        let actions = adapter.on_line(OutputStream::Stdout, &line);
        apply_actions(progress, actions);
    });
    ring
}

pub fn run_streaming_capture_tail(
    mut command: Command,
    progress: &LiveProgress,
    adapter: &mut dyn StreamAdapter,
    ring_bytes: usize,
) -> Result<(i32, RingBuffer), RunError> {
    // IMPORTANT: use explicit pipes so we control FD/handle ownership and never retain a write end
    // in the parent. If the parent accidentally keeps a write end open, reader threads can block
    // forever waiting for EOF (especially when the child produces little/no output).
    let (stdout_reader, stdout_writer) = os_pipe::pipe().map_err(RunError::SpawnFailed)?;
    let (stderr_reader, stderr_writer) = os_pipe::pipe().map_err(RunError::SpawnFailed)?;
    command
        .stdout(std::process::Stdio::from(stdout_writer))
        .stderr(std::process::Stdio::from(stderr_writer));
    let child = command.spawn().map_err(RunError::SpawnFailed)?;
    // IMPORTANT: ensure the parent does not retain any pipe write ends via `Command`/`Stdio`
    // ownership. If a write end stays open in the parent, reader threads can block forever and
    // we hang (especially when the child produces little/no output).
    drop(command);

    if let Some(label) = adapter.on_start() {
        progress.set_current_label(label);
    }

    let (tx, rx) = mpsc::channel::<(OutputStream, String)>();

    spawn_lines_thread(stdout_reader, tx.clone(), OutputStream::Stdout);
    spawn_lines_thread(stderr_reader, tx.clone(), OutputStream::Stderr);

    drop(tx);

    drain_channel_until_exit_then_deadline(child, rx, ring_bytes, |stream, line, ring| {
        ring.push_line(line.to_string());
        match stream {
            OutputStream::Stdout => progress.record_runner_stdout_line(line),
            OutputStream::Stderr => progress.record_runner_stderr_line(line),
        }
        let actions = adapter.on_line(stream, line);
        apply_actions(progress, actions);
    })
}

pub fn run_streaming_capture_tail_merged(
    command: Command,
    progress: &LiveProgress,
    adapter: &mut dyn StreamAdapter,
    ring_bytes: usize,
) -> Result<(i32, RingBuffer), RunError> {
    struct MergeStreamsAdapter<'a> {
        inner: &'a mut dyn StreamAdapter,
    }

    impl<'a> StreamAdapter for MergeStreamsAdapter<'a> {
        fn on_start(&mut self) -> Option<String> {
            self.inner.on_start()
        }

        fn on_line(&mut self, _stream: OutputStream, line: &str) -> Vec<StreamAction> {
            self.inner.on_line(OutputStream::Stdout, line)
        }
    }

    let mut merged = MergeStreamsAdapter { inner: adapter };

    #[cfg(unix)]
    {
        // Create a single pipe and point both stdout and stderr at the same write end. This
        // preserves the kernel-observed ordering of interleaved writes across the two streams.
        //
        // IMPORTANT: never block waiting for EOF. It is possible for stdout/stderr to remain open
        // (e.g. background processes inheriting FDs), even after the direct child exits. We read on
        // a thread and stop when the child exits + a short drain window elapses.
        let (merged_reader, merged_writer) = os_pipe::pipe().map_err(RunError::SpawnFailed)?;
        let merged_writer2 = merged_writer.try_clone().map_err(RunError::SpawnFailed)?;

        let mut command = command;
        command
            .stdout(std::process::Stdio::from(merged_writer))
            .stderr(std::process::Stdio::from(merged_writer2));

        let child = command.spawn().map_err(RunError::SpawnFailed)?;
        drop(command);

        if let Some(label) = merged.on_start() {
            progress.set_current_label(label);
        }

        let (tx, rx) = mpsc::channel::<(OutputStream, String)>();
        spawn_lines_thread(merged_reader, tx, OutputStream::Stdout);

        drain_channel_until_exit_then_deadline(child, rx, ring_bytes, |stream, line, ring| {
            ring.push_line(line.to_string());
            progress.record_runner_stdout_line(line);
            let actions = merged.on_line(stream, line);
            apply_actions(progress, actions);
        })
    }

    #[cfg(not(unix))]
    {
        run_streaming_capture_tail(command, progress, &mut merged, ring_bytes)
    }
}
