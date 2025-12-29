use std::time::Instant;

pub fn enabled() -> bool {
    matches!(
        std::env::var("HEADLAMP_PROFILE").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
    )
}

pub struct ProfileSpan {
    name: &'static str,
    start: Instant,
}

impl ProfileSpan {
    pub fn new(name: &'static str) -> Option<Self> {
        enabled().then_some(Self {
            name,
            start: Instant::now(),
        })
    }
}

impl Drop for ProfileSpan {
    fn drop(&mut self) {
        let ms = self.start.elapsed().as_millis();
        eprintln!("[headlamp-profile] {name} took {ms}ms", name = self.name);
    }
}

pub fn span(name: &'static str) -> Option<ProfileSpan> {
    ProfileSpan::new(name)
}
