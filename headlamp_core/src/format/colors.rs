pub fn success(text: &str) -> String {
    color_hex("#22c55e", text)
}

pub fn warn(text: &str) -> String {
    color_hex("#eab308", text)
}

pub fn failure(text: &str) -> String {
    color_hex("#ff2323", text)
}

pub fn run(text: &str) -> String {
    color_hex("#3b82f6", text)
}

pub fn skip(text: &str) -> String {
    warn(text)
}

pub fn todo(text: &str) -> String {
    color_hex("#38bdf8", text)
}

pub fn bg_success(text: &str) -> String {
    bg_color_hex("#22c55e", text)
}

pub fn bg_failure(text: &str) -> String {
    bg_color_hex("#ff2323", text)
}

pub fn bg_run(text: &str) -> String {
    bg_color_hex("#3b82f6", text)
}

fn use_color() -> bool {
    let no_color = std::env::var("NO_COLOR")
        .ok()
        .is_some_and(|value| !value.trim().is_empty());
    if no_color {
        return false;
    }

    let clicolor_disabled = std::env::var("CLICOLOR")
        .ok()
        .is_some_and(|value| value.trim() == "0");
    if clicolor_disabled {
        return false;
    }

    let is_dumb_term = std::env::var("TERM")
        .ok()
        .is_some_and(|value| value.trim() == "dumb");

    let force_color = match std::env::var("FORCE_COLOR")
        .ok()
        .map(|s| s.trim().to_string())
    {
        None => None,
        Some(force_value) if force_value.is_empty() => None,
        Some(force_value) if force_value == "0" => Some(false),
        Some(_) => Some(true),
    };

    if is_dumb_term && force_color != Some(true) {
        return false;
    }

    force_color.unwrap_or_else(crate::format::terminal::is_output_terminal)
}

fn color_hex(hex: &str, text: &str) -> String {
    if !use_color() {
        return text.to_string();
    }
    let Some((r, g, b)) = parse_hex_rgb(hex) else {
        return text.to_string();
    };
    format!("\u{1b}[38;2;{r};{g};{b}m{text}\u{1b}[0m")
}

fn bg_color_hex(hex: &str, text: &str) -> String {
    if !use_color() {
        return text.to_string();
    }
    let Some((r, g, b)) = parse_hex_rgb(hex) else {
        return text.to_string();
    };
    format!("\u{1b}[48;2;{r};{g};{b}m{text}\u{1b}[0m")
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
