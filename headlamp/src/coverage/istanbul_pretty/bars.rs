use std::io::IsTerminal;

use crate::format::ansi;

const SUCCESS_THRESHOLD: f64 = 85.0;
const WARNING_THRESHOLD: f64 = 60.0;

pub fn tint_pct(pct: f64, text: &str) -> String {
    if pct >= SUCCESS_THRESHOLD {
        ansi_rgb("#22c55e", text)
    } else if pct >= WARNING_THRESHOLD {
        ansi_rgb("#eab308", text)
    } else {
        ansi_rgb("#ff2323", text)
    }
}

pub fn bar(pct: f64, width: usize) -> String {
    let w = width.max(0);
    let filled = (((pct / 100.0) * (w as f64)).round() as isize).clamp(0, w as isize) as usize;
    let solid = if supports_unicode() { "█" } else { "#" };
    let empty = if supports_unicode() { "░" } else { "-" };
    let solid_text = solid.repeat(filled);
    let empty_text = empty.repeat(w.saturating_sub(filled));
    format!("{}{}", tint_pct(pct, &solid_text), ansi::gray(&empty_text))
}

fn supports_unicode() -> bool {
    let term = std::env::var("TERM").unwrap_or_default().to_lowercase();
    !term.is_empty() && term != "dumb"
}

fn ansi_rgb(hex: &str, text: &str) -> String {
    let no_color = !std::env::var("NO_COLOR")
        .ok()
        .unwrap_or_default()
        .trim()
        .is_empty();
    if no_color {
        return text.to_string();
    }
    let forced = !std::env::var("FORCE_COLOR")
        .ok()
        .unwrap_or_default()
        .trim()
        .is_empty();
    let use_color = std::io::stdout().is_terminal() || forced;
    if !use_color {
        return text.to_string();
    }
    let (r, g, b) = parse_hex_rgb(hex).unwrap_or((255, 255, 255));
    format!("\u{1b}[38;2;{r};{g};{b}m{text}\u{1b}[0m")
}

fn parse_hex_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let normalized = hex.trim().trim_start_matches('#');
    let full = match normalized.len() {
        3 => normalized.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 => normalized.to_string(),
        _ => return None,
    };
    let r = u8::from_str_radix(&full[0..2], 16).ok()?;
    let g = u8::from_str_radix(&full[2..4], 16).ok()?;
    let b = u8::from_str_radix(&full[4..6], 16).ok()?;
    Some((r, g, b))
}
