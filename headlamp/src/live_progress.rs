use std::io::IsTerminal;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveProgressMode {
    Off,
    Plain,
    Interactive,
}

pub struct LiveProgress {
    mode: LiveProgressMode,
    stop: Arc<AtomicBool>,
    done_units: Arc<AtomicUsize>,
    current_label: Arc<Mutex<String>>,
    last_event_at: Arc<Mutex<Instant>>,
    last_runner_stdout_hint: Arc<Mutex<Option<String>>>,
    last_runner_stderr_hint: Arc<Mutex<Option<String>>>,
    last_frame_lines: Arc<AtomicUsize>,
    write_lock: Arc<Mutex<()>>,
    ticker: Option<JoinHandle<()>>,
}

pub fn live_progress_mode_with_env_ci(
    stdout_is_tty: bool,
    ci: bool,
    env_ci: bool,
) -> LiveProgressMode {
    if ci || env_ci {
        return LiveProgressMode::Plain;
    }
    if stdout_is_tty && !env_ci {
        return LiveProgressMode::Interactive;
    }
    LiveProgressMode::Off
}

pub fn live_progress_mode(stdout_is_tty: bool, ci: bool) -> LiveProgressMode {
    let env_ci = std::env::var("CI").ok().is_some();
    live_progress_mode_with_env_ci(stdout_is_tty, ci, env_ci)
}

pub fn render_run_frame(
    current_label: &str,
    done_units: usize,
    total_units: usize,
    spinner_index: usize,
    elapsed_seconds: u64,
    idle_seconds: u64,
    recent: &str,
) -> String {
    render_run_frame_with_columns(RenderRunFrameArgs {
        current_label,
        done_units,
        total_units,
        spinner_index,
        elapsed_seconds,
        idle_seconds,
        recent,
        columns: terminal_columns(),
    })
}

pub struct RenderRunFrameArgs<'a> {
    pub current_label: &'a str,
    pub done_units: usize,
    pub total_units: usize,
    pub spinner_index: usize,
    pub elapsed_seconds: u64,
    pub idle_seconds: u64,
    pub recent: &'a str,
    pub columns: usize,
}

pub fn render_run_frame_with_columns(args: RenderRunFrameArgs<'_>) -> String {
    let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let spinner = spinner_frames[args.spinner_index % spinner_frames.len()];
    let mut lines = Vec::new();
    lines.push(format!(
        "RUN [{spinner} +{}s] ({}/{}) {}",
        args.elapsed_seconds, args.done_units, args.total_units, args.current_label
    ));
    let recent = args.recent.trim();
    if recent.is_empty() {
        lines.push(format!("idle {}s", args.idle_seconds));
    } else {
        lines.push(recent.to_string());
    }
    hard_wrap_lines_to_terminal_width(&lines, args.columns)
}

fn render_plain_line(
    current_label: &str,
    done_units: usize,
    total_units: usize,
    elapsed_seconds: u64,
    idle_seconds: u64,
    recent: &str,
) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "RUN (+{elapsed_seconds}s) ({done_units}/{}) {current_label}",
        total_units.max(1)
    ));
    let recent = recent.trim();
    if recent.is_empty() {
        lines.push(format!("idle {idle_seconds}s"));
    } else {
        lines.push(format!("idle {idle_seconds}s | {recent}"));
    }
    hard_wrap_lines_to_terminal_width(&lines, terminal_columns())
}

fn terminal_columns() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| *n >= 20)
        .unwrap_or(120)
}

fn hard_wrap_to_terminal_width(text: &str, columns: usize) -> String {
    // Keep one column spare to avoid accidental wrapping differences across terminals.
    let width = columns.saturating_sub(1).max(10);
    let mut out = String::with_capacity(text.len() + 16);

    let mut col = 0usize;
    for ch in text.chars() {
        if ch == '\n' {
            out.push('\n');
            col = 0;
            continue;
        }
        if col >= width {
            out.push('\n');
            col = 0;
        }
        out.push(ch);
        col += 1;
    }
    out
}

fn hard_wrap_lines_to_terminal_width(lines: &[String], columns: usize) -> String {
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&hard_wrap_to_terminal_width(line, columns));
    }
    out
}

fn frame_line_count(frame: &str) -> usize {
    // At least one line, even for empty strings.
    frame.chars().filter(|c| *c == '\n').count() + 1
}

fn clear_previous_frame(lines: usize) {
    if lines == 0 {
        return;
    }
    // We are currently at the end of the last printed line.
    // Clear bottom line, then move up and clear until we've cleared the whole block.
    let _ = std::io::stdout().write_all("\r\u{1b}[2K".as_bytes());
    for _ in 1..lines {
        let _ = std::io::stdout().write_all("\u{1b}[1A\r\u{1b}[2K".as_bytes());
    }
}

fn trim_hint(raw: &str) -> String {
    let compact = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    // We now render progress frames as multi-line blocks; keep hints large enough to be useful,
    // but still bounded so we don't blow up the terminal with huge JSON payloads.
    let max_chars: usize = terminal_columns().saturating_mul(6).clamp(120, 1200);
    if compact.chars().count() <= max_chars {
        return compact;
    }
    compact.chars().take(max_chars).collect::<String>()
}

fn classify_json_line_for_progress(line: &str) -> Option<String> {
    let s = line.trim();
    if !s.starts_with('{') {
        return None;
    }
    let value = serde_json::from_str::<serde_json::Value>(s).ok()?;
    let obj = value.as_object()?;

    let ty = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let event = obj.get("event").and_then(|v| v.as_str()).unwrap_or("");
    let name = obj.get("name").and_then(|v| v.as_str()).unwrap_or("");

    // nextest's structured output (and similar formats) often looks like:
    // {"type":"test","event":"failed","name":"crate::suite::test", ...}
    if !event.is_empty() && !name.is_empty() {
        let tag = ty;
        let tag = if tag.is_empty() { "event" } else { tag };
        return Some(trim_hint(&format!("{tag} {event}: {name}")));
    }

    // Fallback: show a compacted JSON line if it contains "failed"/"error" hints.
    let lower = s.to_ascii_lowercase();
    if lower.contains("failed") || lower.contains("error") || lower.contains("panic") {
        return Some(trim_hint(s));
    }
    None
}

fn classify_runner_line_for_progress(line: &str) -> Option<String> {
    let s = line.trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_ascii_lowercase();

    let prefix = |p: &str| s.starts_with(p).then(|| trim_hint(s));
    classify_json_line_for_progress(s).or_else(|| {
        // nextest (and some other runners) print a clear "START ..." for the currently-running test
        // which is much more informative than compilation "Finished ..." lines.
        prefix("START ")
            .or_else(|| prefix("RUNS "))
            .or_else(|| prefix("PASS "))
            .or_else(|| prefix("FAIL "))
            .or_else(|| prefix("Compiling "))
            .or_else(|| prefix("Finished "))
            .or_else(|| prefix("Running "))
            .or_else(|| prefix("test "))
            .or_else(|| prefix("error:"))
            .or_else(|| {
                ["downloading", "installing", "resolving"]
                    .iter()
                    .find(|needle| lower.contains(**needle))
                    .map(|_| trim_hint(s))
            })
            .or_else(|| lower.contains("collecting").then(|| trim_hint(s)))
            .or_else(|| {
                (lower.contains("building") || lower.contains("compiling")).then(|| trim_hint(s))
            })
            .or_else(|| {
                (lower.contains("running") || lower.contains("executing")).then(|| trim_hint(s))
            })
            .or_else(|| {
                (lower.contains("failed") || lower.contains("fail ") || lower.contains("timeout"))
                    .then(|| trim_hint(s))
            })
    })
}

fn hint_score(hint: &str) -> i32 {
    if hint.starts_with("START ") || hint.starts_with("RUNS ") {
        return 100;
    }
    if hint.starts_with("test ") || hint.starts_with("Running ") {
        return 90;
    }
    if hint.starts_with("FAIL ")
        || hint.starts_with("error:")
        || hint.to_ascii_lowercase().contains("failed")
    {
        return 95;
    }
    if hint.starts_with("Finished ") {
        return 10;
    }
    50
}

fn recent_summary(stdout: Option<String>, stderr: Option<String>) -> String {
    match (stdout, stderr) {
        (None, None) => "no activity yet".to_string(),
        (Some(s), None) => format!("stdout: {s}"),
        (None, Some(e)) => format!("stderr: {e}"),
        (Some(s), Some(e)) => {
            // If stderr is just "Finished ...", but stdout has a "START ..." (or other higher signal),
            // show both, ordered by usefulness.
            let mut items = [("stdout", s), ("stderr", e)];
            items.sort_by_key(|(_, h)| -hint_score(h));
            // Put each stream on its own line so long details can wrap cleanly without becoming
            // one enormous line that feels "truncated".
            format!(
                "{}: {}\n{}: {}",
                items[0].0, items[0].1, items[1].0, items[1].1
            )
        }
    }
}

impl LiveProgress {
    pub fn start(total_units: usize, mode: LiveProgressMode) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let done_units = Arc::new(AtomicUsize::new(0));
        let current_label = Arc::new(Mutex::new(String::new()));
        let last_event_at = Arc::new(Mutex::new(Instant::now()));
        let last_runner_stdout_hint = Arc::new(Mutex::new(None));
        let last_runner_stderr_hint = Arc::new(Mutex::new(None));
        let spinner_index = Arc::new(AtomicUsize::new(0));
        let last_frame_lines = Arc::new(AtomicUsize::new(0));
        let write_lock = Arc::new(Mutex::new(()));
        let started_at = Instant::now();

        let ticker = match mode {
            LiveProgressMode::Off => None,
            LiveProgressMode::Interactive => {
                let stop = Arc::clone(&stop);
                let done_units = Arc::clone(&done_units);
                let current_label = Arc::clone(&current_label);
                let spinner_index = Arc::clone(&spinner_index);
                let last_event_at = Arc::clone(&last_event_at);
                let last_runner_stdout_hint = Arc::clone(&last_runner_stdout_hint);
                let last_runner_stderr_hint = Arc::clone(&last_runner_stderr_hint);
                let last_frame_lines = Arc::clone(&last_frame_lines);
                let write_lock = Arc::clone(&write_lock);
                Some(std::thread::spawn(move || {
                    while !stop.load(Ordering::SeqCst) {
                        spinner_index.fetch_add(1, Ordering::SeqCst);
                        let done = done_units.load(Ordering::SeqCst);
                        let label = current_label
                            .lock()
                            .ok()
                            .map(|g| g.clone())
                            .unwrap_or_default();
                        let elapsed_seconds = started_at.elapsed().as_secs();
                        let idle_seconds = last_event_at
                            .lock()
                            .ok()
                            .map(|t| t.elapsed().as_secs())
                            .unwrap_or(elapsed_seconds);
                        let recent = recent_summary(
                            last_runner_stdout_hint.lock().ok().and_then(|g| g.clone()),
                            last_runner_stderr_hint.lock().ok().and_then(|g| g.clone()),
                        );
                        let frame = render_run_frame(
                            &label,
                            done,
                            total_units.max(1),
                            spinner_index.load(Ordering::SeqCst),
                            elapsed_seconds,
                            idle_seconds,
                            &recent,
                        );
                        if let Ok(_guard) = write_lock.lock() {
                            let prev_lines = last_frame_lines.load(Ordering::SeqCst);
                            clear_previous_frame(prev_lines);
                            let _ = std::io::stdout().write_all(frame.as_bytes());
                            let _ = std::io::stdout().flush();
                            last_frame_lines.store(frame_line_count(&frame), Ordering::SeqCst);
                        }
                        std::thread::sleep(Duration::from_millis(120));
                    }
                }))
            }
            LiveProgressMode::Plain => {
                let stop = Arc::clone(&stop);
                let done_units = Arc::clone(&done_units);
                let current_label = Arc::clone(&current_label);
                let last_event_at = Arc::clone(&last_event_at);
                let last_runner_stdout_hint = Arc::clone(&last_runner_stdout_hint);
                let last_runner_stderr_hint = Arc::clone(&last_runner_stderr_hint);
                let last_frame_lines = Arc::clone(&last_frame_lines);
                let write_lock = Arc::clone(&write_lock);
                let stdout_is_tty = std::io::stdout().is_terminal();
                Some(std::thread::spawn(move || {
                    while !stop.load(Ordering::SeqCst) {
                        let done = done_units.load(Ordering::SeqCst);
                        let label = current_label
                            .lock()
                            .ok()
                            .map(|g| g.clone())
                            .unwrap_or_default();
                        if label.trim().is_empty() {
                            std::thread::sleep(Duration::from_millis(200));
                            continue;
                        }
                        let elapsed_seconds = started_at.elapsed().as_secs();
                        let idle_seconds = last_event_at
                            .lock()
                            .ok()
                            .map(|t| t.elapsed().as_secs())
                            .unwrap_or(elapsed_seconds);
                        if idle_seconds < 5 {
                            std::thread::sleep(Duration::from_secs(2));
                            continue;
                        }
                        let recent = recent_summary(
                            last_runner_stdout_hint.lock().ok().and_then(|g| g.clone()),
                            last_runner_stderr_hint.lock().ok().and_then(|g| g.clone()),
                        );
                        let line = render_plain_line(
                            &label,
                            done,
                            total_units,
                            elapsed_seconds,
                            idle_seconds,
                            &recent,
                        );
                        if let Ok(_guard) = write_lock.lock() {
                            if stdout_is_tty {
                                let prev_lines = last_frame_lines.load(Ordering::SeqCst);
                                clear_previous_frame(prev_lines);
                                let _ = std::io::stdout().write_all(line.as_bytes());
                                last_frame_lines.store(frame_line_count(&line), Ordering::SeqCst);
                            } else {
                                let _ = std::io::stdout().write_all(line.as_bytes());
                                let _ = std::io::stdout().write_all("\n".as_bytes());
                            }
                            let _ = std::io::stdout().flush();
                        }
                        std::thread::sleep(Duration::from_secs(2));
                    }
                }))
            }
        };

        Self {
            mode,
            stop,
            done_units,
            current_label,
            last_event_at,
            last_runner_stdout_hint,
            last_runner_stderr_hint,
            last_frame_lines,
            write_lock,
            ticker,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.mode != LiveProgressMode::Off
    }

    pub fn set_current_label(&self, label: String) {
        if let Ok(mut guard) = self.current_label.lock() {
            *guard = label.clone();
        }
        if self.mode != LiveProgressMode::Off {
            if let Ok(mut guard) = self.last_event_at.lock() {
                *guard = Instant::now();
            }
        }
    }

    pub fn record_runner_stdout_line(&self, line: &str) {
        let Some(hint) = classify_runner_line_for_progress(line) else {
            return;
        };
        if let Ok(mut guard) = self.last_runner_stdout_hint.lock() {
            *guard = Some(hint);
        }
        if self.mode != LiveProgressMode::Off {
            if let Ok(mut guard) = self.last_event_at.lock() {
                *guard = Instant::now();
            }
        }
    }

    pub fn record_runner_stderr_line(&self, line: &str) {
        let Some(hint) = classify_runner_line_for_progress(line) else {
            return;
        };
        if let Ok(mut guard) = self.last_runner_stderr_hint.lock() {
            *guard = Some(hint);
        }
        if self.mode != LiveProgressMode::Off {
            if let Ok(mut guard) = self.last_event_at.lock() {
                *guard = Instant::now();
            }
        }
    }

    pub fn increment_done(&self, delta: usize) {
        if self.mode != LiveProgressMode::Off {
            self.done_units.fetch_add(delta, Ordering::SeqCst);
            if let Ok(mut guard) = self.last_event_at.lock() {
                *guard = Instant::now();
            }
        }
    }

    pub fn println_stdout(&self, line: &str) {
        if self.mode != LiveProgressMode::Off {
            if let Ok(mut guard) = self.last_event_at.lock() {
                *guard = Instant::now();
            }
        }
        if let Ok(_guard) = self.write_lock.lock() {
            if self.mode != LiveProgressMode::Off && std::io::stdout().is_terminal() {
                let prev_lines = self.last_frame_lines.load(Ordering::SeqCst);
                clear_previous_frame(prev_lines);
                self.last_frame_lines.store(0, Ordering::SeqCst);
            }
            let _ = std::io::stdout().write_all(line.as_bytes());
            let _ = std::io::stdout().write_all("\n".as_bytes());
            let _ = std::io::stdout().flush();
        }
    }

    pub fn eprintln_stderr(&self, line: &str) {
        if self.mode != LiveProgressMode::Off {
            if let Ok(mut guard) = self.last_event_at.lock() {
                *guard = Instant::now();
            }
        }
        if let Ok(_guard) = self.write_lock.lock() {
            if self.mode != LiveProgressMode::Off && std::io::stdout().is_terminal() {
                let prev_lines = self.last_frame_lines.load(Ordering::SeqCst);
                clear_previous_frame(prev_lines);
                self.last_frame_lines.store(0, Ordering::SeqCst);
                let _ = std::io::stdout().flush();
            }
            let _ = std::io::stderr().write_all(line.as_bytes());
            let _ = std::io::stderr().write_all("\n".as_bytes());
            let _ = std::io::stderr().flush();
        }
    }

    pub fn finish(mut self) {
        if self.mode == LiveProgressMode::Off {
            return;
        }
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.ticker.take() {
            let _ = handle.join();
        }
        if self.mode != LiveProgressMode::Off && std::io::stdout().is_terminal() {
            if let Ok(_guard) = self.write_lock.lock() {
                let prev_lines = self.last_frame_lines.load(Ordering::SeqCst);
                clear_previous_frame(prev_lines);
                self.last_frame_lines.store(0, Ordering::SeqCst);
                let _ = std::io::stdout().flush();
            }
        }
    }
}
