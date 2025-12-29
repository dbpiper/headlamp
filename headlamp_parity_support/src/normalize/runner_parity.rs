pub(super) fn strip_failure_details(text: &str) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return String::new();
    }
    let stripped = lines
        .iter()
        .map(|l| strip_all_ansi_like_sequences(l))
        .collect::<Vec<_>>();

    let fail_line = stripped
        .iter()
        .position(|l| l.trim_start().starts_with("FAIL "));
    let summary_line = stripped.iter().position(|l| l.contains("Failed Tests"));
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
    let re = regex::Regex::new(r"\x1b\[[^m]*m").unwrap();
    re.replace_all(text, "").to_string()
}
