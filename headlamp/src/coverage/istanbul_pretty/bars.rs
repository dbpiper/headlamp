use std::io::IsTerminal;

use std::sync::LazyLock;

const SUCCESS_THRESHOLD: f64 = 85.0;
const WARNING_THRESHOLD: f64 = 60.0;
const RGB_RESET: &str = "\u{1b}[0m";
const RGB_GREEN_PREFIX: &str = "\u{1b}[38;2;34;197;94m";
const RGB_YELLOW_PREFIX: &str = "\u{1b}[38;2;234;179;8m";
const RGB_RED_PREFIX: &str = "\u{1b}[38;2;255;35;35m";

#[derive(Debug, Clone, Copy)]
struct AnsiEnv {
    no_color_requested: bool,
    force_color_requested: bool,
    stdout_is_terminal: bool,
    supports_unicode: bool,
}

static ANSI_ENV: LazyLock<AnsiEnv> = LazyLock::new(|| {
    let no_color_requested = !std::env::var("NO_COLOR")
        .ok()
        .unwrap_or_default()
        .trim()
        .is_empty();
    let force_color_requested = !std::env::var("FORCE_COLOR")
        .ok()
        .unwrap_or_default()
        .trim()
        .is_empty();
    let stdout_is_terminal = std::io::stdout().is_terminal();
    let term = std::env::var("TERM").unwrap_or_default().to_lowercase();
    let supports_unicode = !term.is_empty() && term != "dumb";
    AnsiEnv {
        no_color_requested,
        force_color_requested,
        stdout_is_terminal,
        supports_unicode,
    }
});

pub fn tint_pct(pct: f64, text: &str) -> String {
    let mut out = String::with_capacity(text.len().saturating_add(32));
    write_tint_pct(&mut out, pct, text);
    out
}

pub fn bar(pct: f64, width: usize) -> String {
    let mut out = String::with_capacity(width.saturating_add(64));
    write_bar(&mut out, pct, width);
    out
}

pub fn write_tint_pct(out: &mut String, pct: f64, text: &str) {
    let maybe_prefix = rgb_prefix_for_pct(pct);
    if let Some(prefix) = maybe_prefix {
        out.push_str(prefix);
    }
    out.push_str(text);
    if maybe_prefix.is_some() {
        out.push_str(RGB_RESET);
    }
}

pub fn write_bar(out: &mut String, pct: f64, width: usize) {
    let filled =
        (((pct / 100.0) * (width as f64)).round() as isize).clamp(0, width as isize) as usize;
    let (solid_char, empty_char) = if ANSI_ENV.supports_unicode {
        ('█', '░')
    } else {
        ('#', '-')
    };

    // Match `format!("{}{}", tint_pct(pct, solid_text), ansi::gray(empty_text))` exactly.
    if filled > 0 {
        let maybe_prefix = rgb_prefix_for_pct(pct);
        if let Some(prefix) = maybe_prefix {
            out.push_str(prefix);
        }
        out.extend(std::iter::repeat_n(solid_char, filled));
        if maybe_prefix.is_some() {
            out.push_str(RGB_RESET);
        }
    }

    // Match `ansi::gray(&empty_text)` exactly, even when `empty_text` is empty.
    let remaining = width.saturating_sub(filled);
    out.push_str("\u{1b}[90m");
    out.extend(std::iter::repeat_n(empty_char, remaining));
    out.push_str("\u{1b}[39m");
}

fn should_use_rgb_color() -> bool {
    !ANSI_ENV.no_color_requested && (ANSI_ENV.stdout_is_terminal || ANSI_ENV.force_color_requested)
}

fn rgb_prefix_for_pct(pct: f64) -> Option<&'static str> {
    if !should_use_rgb_color() {
        return None;
    }
    if pct >= SUCCESS_THRESHOLD {
        Some(RGB_GREEN_PREFIX)
    } else if pct >= WARNING_THRESHOLD {
        Some(RGB_YELLOW_PREFIX)
    } else {
        Some(RGB_RED_PREFIX)
    }
}

// Exposed for the table writer to avoid allocating a padded intermediate string while still
// producing byte-for-byte identical ANSI sequences.
pub(crate) fn rgb_prefix_for_pct_for_table(pct: f64) -> Option<&'static str> {
    rgb_prefix_for_pct(pct)
}
