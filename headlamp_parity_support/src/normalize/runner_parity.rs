use regex::Regex;
use std::sync::LazyLock;

static ANSI_LIKE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1b\[[^m]*m").unwrap());

pub(super) fn strip_failure_details(text: &str) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return String::new();
    }
    let stripped = lines
        .iter()
        .map(|line| strip_all_ansi_like_sequences(line))
        .collect::<Vec<_>>();

    let fail_line = stripped
        .iter()
        .position(|line| line.trim_start().starts_with("FAIL "));
    let summary_line = stripped
        .iter()
        .position(|line| line.contains("Failed Tests"));
    let (Some(fail_i), Some(summary_i)) = (fail_line, summary_line) else {
        return text.to_string();
    };
    if summary_i <= fail_i {
        return text.to_string();
    }

    lines
        .iter()
        .take(fail_i + 1)
        .chain(lines.iter().skip(summary_i.saturating_sub(1)))
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
}

fn strip_all_ansi_like_sequences(text: &str) -> String {
    ANSI_LIKE_RE.replace_all(text, "").to_string()
}
