use regex::Regex;
use std::sync::LazyLock;

static ANSI_LIKE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1b\[[^m]*m").unwrap());

fn strip_ansi_like_sequences(text: &str) -> String {
    ANSI_LIKE_RE.replace_all(text, "").to_string()
}

pub(super) fn pick_final_render_block_tty(text: &str) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return String::new();
    }
    let last_box_table_top = find_last_box_table_top(&lines);
    let last_test_files = lines
        .iter()
        .rposition(|line| strip_ansi_like_sequences(line).starts_with("Test Files "))
        .unwrap_or_else(|| lines.len().saturating_sub(1));

    let last_failed_tests = (0..=last_test_files)
        .rev()
        .find(|&i| strip_ansi_like_sequences(lines[i]).contains("Failed Tests"));

    let start = last_failed_tests
        .and_then(|failed_i| find_render_block_start_tty(&lines, failed_i))
        .or_else(|| {
            last_box_table_top.and_then(|box_top| find_render_block_start_tty(&lines, box_top))
        })
        .or(last_box_table_top)
        .unwrap_or(0);
    lines[start..].join("\n")
}

fn find_render_block_start_tty(lines: &[&str], failed_i: usize) -> Option<usize> {
    // Prefer anchoring at the last stable `RUN <ROOT>` header so we keep the whole per-suite
    // listing. If we anchor at the last PASS/FAIL line instead, we can accidentally drop the
    // suite listing for some runners depending on output shape.
    let last_run = (0..=failed_i).rev().find(|&i| {
        strip_ansi_like_sequences(lines[i])
            .trim_start()
            .starts_with("RUN ")
    });
    if last_run.is_some() {
        return last_run;
    }
    (0..=failed_i).rev().find(|&i| {
        let ln = strip_ansi_like_sequences(lines[i]);
        let ln = ln.trim_start();
        ln.starts_with("FAIL ") || ln.starts_with("PASS ")
    })
}

fn find_last_box_table_top(lines: &[&str]) -> Option<usize> {
    (0..lines.len()).rev().find(|&i| {
        let stripped = strip_ansi_like_sequences(lines[i]);
        if !stripped.trim_start().starts_with('┌') {
            return false;
        }
        let maybe_header_idx = lines
            .iter()
            .enumerate()
            .skip(i.saturating_add(1))
            .take(8)
            .find_map(|(j, l)| {
                let s = strip_ansi_like_sequences(l);
                if s.trim().is_empty() {
                    return None;
                }
                Some((j, s))
            });
        let Some((_header_j, header_line)) = maybe_header_idx else {
            return false;
        };
        header_line.contains("│File") || header_line.contains("File ")
    })
}
