use std::io::IsTerminal;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::LiveProgress;
use super::LiveProgressMode;

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
                        let recent = super::classify::recent_summary(
                            last_runner_stdout_hint.lock().ok().and_then(|g| g.clone()),
                            last_runner_stderr_hint.lock().ok().and_then(|g| g.clone()),
                        );
                        let frame = super::frame::render_run_frame(
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
                            super::frame::clear_previous_frame(prev_lines);
                            let _ = std::io::stdout().write_all(frame.as_bytes());
                            let _ = std::io::stdout().flush();
                            last_frame_lines
                                .store(super::frame::frame_line_count(&frame), Ordering::SeqCst);
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
                        let recent = super::classify::recent_summary(
                            last_runner_stdout_hint.lock().ok().and_then(|g| g.clone()),
                            last_runner_stderr_hint.lock().ok().and_then(|g| g.clone()),
                        );
                        let line = super::frame::render_plain_line(
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
                                super::frame::clear_previous_frame(prev_lines);
                                let _ = std::io::stdout().write_all(line.as_bytes());
                                last_frame_lines
                                    .store(super::frame::frame_line_count(&line), Ordering::SeqCst);
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
