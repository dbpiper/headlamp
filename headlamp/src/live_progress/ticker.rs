use std::io::IsTerminal;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::LiveProgress;
use super::LiveProgressMode;

#[derive(Debug, Clone)]
struct TickerShared {
    stop: Arc<AtomicBool>,
    done_units: Arc<AtomicUsize>,
    current_label: Arc<Mutex<String>>,
    last_event_at: Arc<Mutex<Instant>>,
    last_runner_stdout_hint: Arc<Mutex<Option<String>>>,
    last_runner_stderr_hint: Arc<Mutex<Option<String>>>,
    spinner_index: Arc<AtomicUsize>,
    last_frame_lines: Arc<AtomicUsize>,
    write_lock: Arc<Mutex<()>>,
    started_at: Instant,
    total_units: usize,
}

#[derive(Debug, Clone)]
struct PlainTickerShared {
    shared: TickerShared,
    stdout_is_tty: bool,
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

        let shared = TickerShared {
            stop: Arc::clone(&stop),
            done_units: Arc::clone(&done_units),
            current_label: Arc::clone(&current_label),
            last_event_at: Arc::clone(&last_event_at),
            last_runner_stdout_hint: Arc::clone(&last_runner_stdout_hint),
            last_runner_stderr_hint: Arc::clone(&last_runner_stderr_hint),
            spinner_index: Arc::clone(&spinner_index),
            last_frame_lines: Arc::clone(&last_frame_lines),
            write_lock: Arc::clone(&write_lock),
            started_at: Instant::now(),
            total_units,
        };

        let ticker = match mode {
            LiveProgressMode::Off => None,
            LiveProgressMode::Interactive => Some(spawn_interactive_ticker(shared)),
            LiveProgressMode::Plain => Some(spawn_plain_ticker(PlainTickerShared {
                shared,
                stdout_is_tty: std::io::stdout().is_terminal(),
            })),
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
        let Some(hint) = super::classify::classify_runner_line_for_progress(line) else {
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
        let Some(hint) = super::classify::classify_runner_line_for_progress(line) else {
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
                super::frame::clear_previous_frame(prev_lines);
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
                super::frame::clear_previous_frame(prev_lines);
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
                super::frame::clear_previous_frame(prev_lines);
                self.last_frame_lines.store(0, Ordering::SeqCst);
                let _ = std::io::stdout().flush();
            }
        }
    }
}

fn spawn_interactive_ticker(shared: TickerShared) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        while !shared.stop.load(Ordering::SeqCst) {
            interactive_tick(&shared);
            std::thread::sleep(Duration::from_millis(120));
        }
    })
}

fn spawn_plain_ticker(shared: PlainTickerShared) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        while !shared.shared.stop.load(Ordering::SeqCst) {
            plain_tick(&shared);
            std::thread::sleep(Duration::from_secs(2));
        }
    })
}

fn interactive_tick(shared: &TickerShared) {
    shared.spinner_index.fetch_add(1, Ordering::SeqCst);
    let done = shared.done_units.load(Ordering::SeqCst);
    let label = locked_clone(&shared.current_label).unwrap_or_default();
    let (elapsed_seconds, idle_seconds) = elapsed_and_idle_seconds(shared);
    let columns = super::frame::terminal_columns();
    let recent = super::classify::recent_summary(
        locked_clone(&shared.last_runner_stdout_hint).flatten(),
        locked_clone(&shared.last_runner_stderr_hint).flatten(),
    );
    let frame = super::frame::render_run_frame_with_columns(super::frame::RenderRunFrameArgs {
        current_label: &label,
        done_units: done,
        total_units: shared.total_units.max(1),
        spinner_index: shared.spinner_index.load(Ordering::SeqCst),
        elapsed_seconds,
        idle_seconds,
        recent: &recent,
        columns,
    });
    write_frame(shared, &frame, columns);
}

fn plain_tick(shared: &PlainTickerShared) {
    let done = shared.shared.done_units.load(Ordering::SeqCst);
    let label = locked_clone(&shared.shared.current_label).unwrap_or_default();
    if label.trim().is_empty() {
        std::thread::sleep(Duration::from_millis(200));
        return;
    }
    let (elapsed_seconds, idle_seconds) = elapsed_and_idle_seconds(&shared.shared);
    if idle_seconds < 5 {
        std::thread::sleep(Duration::from_secs(2));
        return;
    }
    let columns = super::frame::terminal_columns();
    let recent = super::classify::recent_summary(
        locked_clone(&shared.shared.last_runner_stdout_hint).flatten(),
        locked_clone(&shared.shared.last_runner_stderr_hint).flatten(),
    );
    let line = super::frame::render_plain_line(
        &label,
        done,
        shared.shared.total_units,
        elapsed_seconds,
        idle_seconds,
        &recent,
        columns,
    );
    write_plain_line(shared, &line, columns);
}

fn elapsed_and_idle_seconds(shared: &TickerShared) -> (u64, u64) {
    let elapsed_seconds = shared.started_at.elapsed().as_secs();
    let idle_seconds = shared
        .last_event_at
        .lock()
        .ok()
        .map(|t| t.elapsed().as_secs())
        .unwrap_or(elapsed_seconds);
    (elapsed_seconds, idle_seconds)
}

fn write_frame(shared: &TickerShared, frame: &str, columns: usize) {
    if let Ok(_guard) = shared.write_lock.lock() {
        let prev_lines = shared.last_frame_lines.load(Ordering::SeqCst);
        super::frame::clear_previous_frame(prev_lines);
        let _ = std::io::stdout().write_all(frame.as_bytes());
        let _ = std::io::stdout().flush();
        shared.last_frame_lines.store(
            super::frame::frame_physical_line_count(frame, columns),
            Ordering::SeqCst,
        );
    }
}

fn write_plain_line(shared: &PlainTickerShared, line: &str, columns: usize) {
    if let Ok(_guard) = shared.shared.write_lock.lock() {
        if shared.stdout_is_tty {
            let prev_lines = shared.shared.last_frame_lines.load(Ordering::SeqCst);
            super::frame::clear_previous_frame(prev_lines);
            let _ = std::io::stdout().write_all(line.as_bytes());
            shared.shared.last_frame_lines.store(
                super::frame::frame_physical_line_count(line, columns),
                Ordering::SeqCst,
            );
        } else {
            let _ = std::io::stdout().write_all(line.as_bytes());
            let _ = std::io::stdout().write_all("\n".as_bytes());
        }
        let _ = std::io::stdout().flush();
    }
}

fn locked_clone<T: Clone>(value: &Mutex<T>) -> Option<T> {
    value.lock().ok().map(|g| g.clone())
}
