use std::io::Write;

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

pub(super) fn render_plain_line(
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

pub(super) fn terminal_columns() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| *n >= 20)
        .unwrap_or(120)
}

fn hard_wrap_to_terminal_width(text: &str, columns: usize) -> String {
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

pub(super) fn frame_line_count(frame: &str) -> usize {
    frame.chars().filter(|c| *c == '\n').count() + 1
}

pub(super) fn clear_previous_frame(lines: usize) {
    if lines == 0 {
        return;
    }
    let _ = std::io::stdout().write_all("\r\u{1b}[2K".as_bytes());
    for _ in 1..lines {
        let _ = std::io::stdout().write_all("\u{1b}[1A\r\u{1b}[2K".as_bytes());
    }
}
