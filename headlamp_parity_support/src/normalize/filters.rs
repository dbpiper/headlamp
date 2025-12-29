use crate::normalize::paths::regex_replace;

pub(super) fn should_keep_line_tty(raw_line: &str) -> bool {
    let without_profile = strip_headlamp_profile_suffix(raw_line);
    let stripped = headlamp::format::stacks::strip_ansi_simple(without_profile);
    let is_ansi_only_line = stripped.trim().is_empty() && !without_profile.trim().is_empty();
    let is_profile_only_line =
        raw_line.contains("[headlamp-profile]") && stripped.trim().is_empty();
    !is_profile_only_line && !is_ansi_only_line && should_keep_line(&stripped)
}

pub(super) fn drop_box_table_interior_blank_lines(text: &str) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    let is_table_line = |s: &str| {
        let stripped = headlamp::format::stacks::strip_ansi_simple(s);
        let trimmed = stripped.trim_start();
        trimmed.starts_with('│')
            || trimmed.starts_with('┌')
            || trimmed.starts_with('└')
            || trimmed.starts_with('┼')
            || trimmed.chars().all(|c| c == '─')
    };
    let mut kept: Vec<&str> = vec![];
    for (index, line) in lines.iter().enumerate() {
        let stripped = headlamp::format::stacks::strip_ansi_simple(line);
        if !stripped.trim().is_empty() {
            kept.push(line);
            continue;
        }
        let prev_is_table = (0..index).rev().find_map(|i| {
            let s = headlamp::format::stacks::strip_ansi_simple(lines[i]);
            (!s.trim().is_empty()).then_some(is_table_line(lines[i]))
        });
        let next_is_table = (index + 1..lines.len()).find_map(|i| {
            let s = headlamp::format::stacks::strip_ansi_simple(lines[i]);
            (!s.trim().is_empty()).then_some(is_table_line(lines[i]))
        });
        if prev_is_table == Some(true) && next_is_table == Some(true) {
            continue;
        }
        kept.push(line);
    }
    kept.join("\n")
}

pub(super) fn drop_nondeterministic_lines(text: &str) -> String {
    text.lines()
        .filter_map(|line| {
            let stripped = strip_headlamp_profile_suffix(line);
            (!stripped.trim().is_empty() && should_keep_line(stripped))
                .then(|| stripped.to_string())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn strip_headlamp_profile_suffix(line: &str) -> &str {
    line.split_once("[headlamp-profile]")
        .map_or(line, |(prefix, _suffix)| prefix.trim_end())
}

fn should_keep_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.starts_with("RUN (+") {
        return false;
    }
    if trimmed.starts_with("idle ") {
        return false;
    }
    if (line.contains("\u{1b}[2K") || line.contains("\u{1b}[1A"))
        && !line.chars().any(|c| c.is_alphanumeric())
    {
        return false;
    }
    if line.contains("\u{1b}[2K") && line.contains("RUN [") {
        return false;
    }
    if line.contains("waiting for Jest") || line.contains("+<N>s]") {
        return false;
    }

    if line.contains("/headlamp-original/") {
        return false;
    }
    if line.contains("/dist/cli.cjs") {
        return false;
    }
    if line.contains("node:internal/") {
        return false;
    }
    if line.contains("(node:") || line.contains("node:events:") {
        return false;
    }
    if line.contains("internal/process/") {
        return false;
    }

    let discovery_prefixes = [
        "Selection classify",
        "Discovering →",
        "Discovering (",
        "rg related →",
        "rg candidates →",
        "http augmented candidates →",
        "fallback refine",
        "No matching tests were discovered",
        "Jest args:",
        "Tip:",
        "Selected files →",
        "Discovery results →",
        "Discovery →",
        "Run plan →",
        "Starting Jest",
        " - ",
    ];
    !discovery_prefixes
        .iter()
        .any(|prefix| line.starts_with(prefix))
}

pub(super) fn strip_terminal_sequences(text: &str) -> String {
    let no_osc8 = strip_osc8_sequences(text);
    regex_replace(&no_osc8, "\u{1b}\\[[0-9;]*m", "")
}

pub(super) fn strip_osc8_sequences(text: &str) -> String {
    let no_osc8 = regex_replace(text, "\u{1b}\\]8;;[^\\u{7}]*\\u{7}", "");
    regex_replace(&no_osc8, "\u{1b}\\]8;;\\u{7}", "")
}
