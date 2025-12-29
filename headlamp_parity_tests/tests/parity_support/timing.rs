use std::borrow::Cow;
use std::time::{Duration, Instant};

fn parse_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "y" | "on"
    )
}

pub fn timings_enabled() -> bool {
    std::env::var("HEADLAMP_PARITY_TIMINGS")
        .ok()
        .is_some_and(|value| parse_truthy(&value))
}

#[derive(Debug)]
pub struct TimingGuard {
    label: Cow<'static, str>,
    start: Option<Instant>,
}

impl TimingGuard {
    pub fn start(label: impl Into<Cow<'static, str>>) -> Self {
        let start = timings_enabled().then(Instant::now);
        Self {
            label: label.into(),
            start,
        }
    }

    fn elapsed(&self) -> Option<Duration> {
        self.start.map(|start| start.elapsed())
    }
}

impl Drop for TimingGuard {
    fn drop(&mut self) {
        let Some(elapsed) = self.elapsed() else {
            return;
        };
        eprintln!(
            "[headlamp_parity_tests] timing {}: {:?}",
            self.label, elapsed
        );
    }
}
