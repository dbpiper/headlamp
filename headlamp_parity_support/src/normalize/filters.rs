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
    // Worktree paths can wrap in a narrow TTY, producing a continuation line like:
    // `s/057093c2092a/wt-187-0)` (from `repos/<hash>/wt-...` split after `repo`).
    if is_wrapped_worktree_path_continuation(trimmed) {
        return false;
    }
    // If a live-progress line was wrapped/split, we can get a deterministic "tail fragment"
    // like `1163-1)` or `<N>-<N>)`. Drop these unconditionally.
    if is_live_progress_tail_fragment(trimmed) {
        return false;
    }
    // Drop terminal cursor-control / line-erasing sequences unconditionally. These come from the
    // live progress renderer and can appear alongside partial text when a line is redrawn.
    if line.contains("\u{1b}[2K") || line.contains("\u{1b}[1A") {
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

fn is_wrapped_worktree_path_continuation(trimmed: &str) -> bool {
    // Example continuation when `repos/<hash>/wt-123-0)` wraps after `rep`:
    // `os/057093c2092a/wt-123-0)` (or `s/...` etc).
    if trimmed.contains(char::is_whitespace) {
        return false;
    }
    let parts = trimmed.split('/').collect::<Vec<_>>();
    if parts.len() != 3 {
        return false;
    }
    let [prefix, repo_hash, wt] = [parts[0], parts[1], parts[2]];
    let prefix_ok =
        (1..=4).contains(&prefix.len()) && prefix.chars().all(|c| c.is_ascii_alphabetic());
    let hash_ok = repo_hash.len() == 12 && repo_hash.chars().all(|c| c.is_ascii_hexdigit());
    let wt_ok = wt.starts_with("wt-")
        && wt.ends_with(')')
        && wt
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ')' | '<' | '>'));
    prefix_ok && hash_ok && wt_ok
}

fn is_live_progress_tail_fragment(trimmed: &str) -> bool {
    if !trimmed.ends_with(')') {
        return false;
    }
    if !trimmed.contains('-') {
        return false;
    }
    trimmed
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '-' | ')' | '<' | '>' | 'N'))
}

pub(super) fn strip_terminal_sequences(text: &str) -> String {
    let no_osc8 = strip_osc8_sequences(text);
    regex_replace(&no_osc8, "\u{1b}\\[[0-9;]*m", "")
}

pub(super) fn strip_osc8_sequences(text: &str) -> String {
    let no_osc8 = regex_replace(text, "\u{1b}\\]8;;[^\\u{7}]*\\u{7}", "");
    regex_replace(&no_osc8, "\u{1b}\\]8;;\\u{7}", "")
}
