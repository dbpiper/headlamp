use headlamp::live_progress::{render_run_frame, should_enable_live_progress};

#[test]
fn live_progress_render_run_frame_includes_clear_and_run_prefix() {
    let frame = render_run_frame("jest.config.js", 2, 5, 3, 12);
    assert!(frame.starts_with("\u{1b}[2K\rRUN "));
    assert!(frame.contains("(2/5)"));
    assert!(frame.contains("jest.config.js"));
    assert!(frame.contains("+12s"));
}

#[test]
fn live_progress_enable_gate_follows_tty_flag() {
    assert!(!should_enable_live_progress(false, false));
    assert!(should_enable_live_progress(true, false));
}

#[test]
fn live_progress_disabled_in_ci_mode() {
    assert!(!should_enable_live_progress(true, true));
}
