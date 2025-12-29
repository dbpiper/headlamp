pub fn extract_coverage_ui_block(text: &str) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    let start = lines
        .iter()
        .rposition(|ln| ln.trim_start().starts_with('┌') && ln.contains('┬'))
        .or_else(|| {
            lines
                .iter()
                .rposition(|ln| ln.contains('┌') && ln.contains('┬'))
        })
        .unwrap_or(0);

    let end = lines
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(index, ln)| {
            ln.starts_with(
                "================================================================================",
            )
            .then_some(index)
        })
        .unwrap_or(lines.len().saturating_sub(1));

    lines.get(start..=end).unwrap_or(&lines[..]).join("\n")
}

pub fn extract_istanbul_text_table_block(text: &str) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    let header_idx = lines
        .iter()
        .rposition(|line| {
            headlamp::format::stacks::strip_ansi_simple(line).contains("Uncovered Line #s")
        })
        .unwrap_or(0);

    let start = (0..=header_idx)
        .rev()
        .find(|&index| is_istanbul_dash_line(lines[index]))
        .unwrap_or(header_idx);

    let end = (header_idx..lines.len())
        .rev()
        .find(|&index| is_istanbul_dash_line(lines[index]))
        .unwrap_or(lines.len().saturating_sub(1));

    lines.get(start..=end).unwrap_or(&lines[..]).join("\n")
}

fn is_istanbul_dash_line(line: &str) -> bool {
    let stripped = headlamp::format::stacks::strip_ansi_simple(line);
    stripped.contains("|---------|") && stripped.chars().all(|c| c == '-' || c == '|')
}
