use std::sync::atomic::{AtomicUsize, Ordering};

static CAPTURE_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn next_capture_id() -> usize {
    CAPTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn sha1_12(text: &str) -> String {
    use sha1::Digest;
    let mut h = sha1::Sha1::new();
    h.update(text.as_bytes());
    let hex = hex::encode(h.finalize());
    hex.chars().take(12).collect()
}
