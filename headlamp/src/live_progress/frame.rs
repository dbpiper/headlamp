use std::io::Write;
use std::time::Duration;

use terminal_size::Width;
use unicode_width::UnicodeWidthChar;

use crate::format::time::{TimeUnit, format_duration_at_least};

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
    let elapsed =
        format_duration_at_least(Duration::from_secs(args.elapsed_seconds), TimeUnit::Second);
    let mut lines = Vec::new();
    lines.push(format!(
        "RUN [{spinner} +{elapsed}] ({}/{}) {}",
        args.done_units, args.total_units, args.current_label
    ));
    let recent = args.recent.trim();
    if recent.is_empty() {
        let idle =
            format_duration_at_least(Duration::from_secs(args.idle_seconds), TimeUnit::Second);
        lines.push(format!("idle {idle}"));
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
    columns: usize,
) -> String {
    let elapsed = format_duration_at_least(Duration::from_secs(elapsed_seconds), TimeUnit::Second);
    let idle = format_duration_at_least(Duration::from_secs(idle_seconds), TimeUnit::Second);
    let mut lines = Vec::new();
    lines.push(format!(
        "RUN (+{elapsed}) ({done_units}/{}) {current_label}",
        total_units.max(1)
    ));
    let recent = recent.trim();
    if recent.is_empty() {
        lines.push(format!("idle {idle}"));
    } else {
        lines.push(format!("idle {idle} | {recent}"));
    }
    hard_wrap_lines_to_terminal_width(&lines, columns)
}

pub(super) fn terminal_columns() -> usize {
    terminal_size::terminal_size()
        .map(|(Width(columns), _)| usize::from(columns))
        .filter(|columns| *columns >= 20)
        .or_else(|| {
            std::env::var("COLUMNS")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .filter(|columns| *columns >= 20)
        })
        .unwrap_or(120)
}

fn hard_wrap_to_terminal_width(text: &str, columns: usize) -> String {
    let width = columns.max(10);
    let mut out = String::with_capacity(text.len() + 16);

    let mut col = 0usize;
    for ch in text.chars() {
        if ch == '\n' {
            out.push('\n');
            col = 0;
            continue;
        }
        let ch_width = ch.width().unwrap_or(0);
        if col > 0 && col.saturating_add(ch_width) > width {
            out.push('\n');
            col = 0;
        }
        out.push(ch);
        col = col.saturating_add(ch_width).min(width);
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

pub fn frame_physical_line_count(frame: &str, columns: usize) -> usize {
    let stripped = strip_ansi(frame);
    stripped
        .split('\n')
        .map(|line| physical_rows_for_line(line, columns))
        .sum::<usize>()
        .max(1)
}

fn physical_rows_for_line(line: &str, columns: usize) -> usize {
    let width = columns.max(1);
    let display_width = unicode_display_width(line);
    display_width.div_ceil(width).max(1)
}

fn unicode_display_width(text: &str) -> usize {
    text.chars()
        .map(|ch| ch.width().unwrap_or(0))
        .sum::<usize>()
}

fn strip_ansi(text: &str) -> String {
    let stripped_bytes = strip_ansi_escapes::strip(text.as_bytes());
    String::from_utf8(stripped_bytes).unwrap_or_else(|_| text.to_string())
}

pub(super) fn clear_previous_frame(lines: usize) {
    if lines == 0 {
        return;
    }
    let _ = std::io::stdout().write_all("\u{1b}[2K\r".as_bytes());
    for _ in 1..lines {
        let _ = std::io::stdout().write_all("\u{1b}[1A\u{1b}[2K\r".as_bytes());
    }
}
