use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub struct LiveProgress {
    enabled: bool,
    stop: Arc<AtomicBool>,
    done_units: Arc<AtomicUsize>,
    current_label: Arc<Mutex<String>>,
    write_lock: Arc<Mutex<()>>,
    ticker: Option<JoinHandle<()>>,
}

pub fn should_enable_live_progress(stdout_is_tty: bool, ci: bool) -> bool {
    stdout_is_tty && !ci && std::env::var("CI").ok().is_none()
}

pub fn render_run_frame(
    current_label: &str,
    done_units: usize,
    total_units: usize,
    spinner_index: usize,
    elapsed_seconds: u64,
) -> String {
    let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let spinner = spinner_frames[spinner_index % spinner_frames.len()];
    format!(
        "\u{1b}[2K\rRUN [{spinner} +{elapsed_seconds}s] ({done_units}/{total_units}) {current_label}"
    )
}

impl LiveProgress {
    pub fn start(total_units: usize, enabled: bool) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let done_units = Arc::new(AtomicUsize::new(0));
        let current_label = Arc::new(Mutex::new(String::new()));
        let spinner_index = Arc::new(AtomicUsize::new(0));
        let write_lock = Arc::new(Mutex::new(()));
        let started_at = Instant::now();

        let ticker = enabled.then(|| {
            let stop = Arc::clone(&stop);
            let done_units = Arc::clone(&done_units);
            let current_label = Arc::clone(&current_label);
            let spinner_index = Arc::clone(&spinner_index);
            let write_lock = Arc::clone(&write_lock);
            std::thread::spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    spinner_index.fetch_add(1, Ordering::SeqCst);
                    let done = done_units.load(Ordering::SeqCst);
                    let label = current_label
                        .lock()
                        .ok()
                        .map(|g| g.clone())
                        .unwrap_or_default();
                    let elapsed_seconds = started_at.elapsed().as_secs();
                    let frame = render_run_frame(
                        &label,
                        done,
                        total_units.max(1),
                        spinner_index.load(Ordering::SeqCst),
                        elapsed_seconds,
                    );
                    if let Ok(_guard) = write_lock.lock() {
                        let _ = std::io::stdout().write_all(frame.as_bytes());
                        let _ = std::io::stdout().flush();
                    }
                    std::thread::sleep(Duration::from_millis(120));
                }
            })
        });

        Self {
            enabled,
            stop,
            done_units,
            current_label,
            write_lock,
            ticker,
        }
    }

    pub fn set_current_label(&self, label: String) {
        if !self.enabled {
            return;
        }
        if let Ok(mut guard) = self.current_label.lock() {
            *guard = label;
        }
    }

    pub fn increment_done(&self, delta: usize) {
        if !self.enabled {
            return;
        }
        self.done_units.fetch_add(delta, Ordering::SeqCst);
    }

    pub fn println_stdout(&self, line: &str) {
        if let Ok(_guard) = self.write_lock.lock() {
            if self.enabled {
                let _ = std::io::stdout().write_all("\u{1b}[2K\r".as_bytes());
            }
            let _ = std::io::stdout().write_all(line.as_bytes());
            let _ = std::io::stdout().write_all("\n".as_bytes());
            let _ = std::io::stdout().flush();
        }
    }

    pub fn eprintln_stderr(&self, line: &str) {
        if let Ok(_guard) = self.write_lock.lock() {
            if self.enabled {
                let _ = std::io::stdout().write_all("\u{1b}[2K\r".as_bytes());
                let _ = std::io::stdout().flush();
            }
            let _ = std::io::stderr().write_all(line.as_bytes());
            let _ = std::io::stderr().write_all("\n".as_bytes());
            let _ = std::io::stderr().flush();
        }
    }

    pub fn finish(mut self) {
        if !self.enabled {
            return;
        }
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.ticker.take() {
            let _ = handle.join();
        }
        if let Ok(_guard) = self.write_lock.lock() {
            let _ = std::io::stdout().write_all("\u{1b}[2K\r".as_bytes());
            let _ = std::io::stdout().flush();
        }
    }
}
