use std::path::Path;

use crate::parity_meta::NormalizationMeta;

mod blocks;
mod common;
mod filters;
mod paths;
mod runner_parity;
mod tty_blocks;

use regex::Regex;
use std::sync::LazyLock;

static BOX_CHAR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[┌│┼└]").unwrap());

pub fn normalize(text: String, root: &Path) -> String {
    normalize_with_meta(text, root).0
}

pub fn normalize_tty_ui(text: String, root: &Path) -> String {
    normalize_tty_ui_with_meta(text, root).0
}

pub fn normalize_tty_ui_runner_parity(text: String, root: &Path) -> String {
    let (normalized, _meta) = normalize_tty_ui_with_meta(text, root);
    runner_parity::strip_failure_details(&normalized)
}

pub fn normalize_tty_ui_runner_parity_with_meta(
    text: String,
    root: &Path,
) -> (String, NormalizationMeta) {
    let (normalized, meta) = normalize_tty_ui_with_meta(text, root);
    (runner_parity::strip_failure_details(&normalized), meta)
}

pub fn normalize_with_meta(text: String, root: &Path) -> (String, NormalizationMeta) {
    let normalized_paths = paths::normalize_paths(text, root);
    let filtered = filters::drop_nondeterministic_lines(&normalized_paths);
    let stripped = filters::strip_terminal_sequences(&filtered);
    let final_block = blocks::pick_final_render_block(&stripped);
    let normalized =
        common::trim_leading_blank_lines(&blocks::normalize_render_block(&final_block));

    let (last_failed_tests_line, last_test_files_line, last_box_table_top_line) =
        common::compute_render_indices(&stripped);
    let stages = vec![
        common::stage_stats("normalized_paths", &normalized_paths),
        common::stage_stats("filtered", &filtered),
        common::stage_stats("stripped", &stripped),
        common::stage_stats("final_block", &final_block),
        common::stage_stats("normalized", &normalized),
    ];
    let meta = NormalizationMeta {
        normalizer: crate::parity_meta::NormalizerKind::NonTty,
        used_fallback: false,
        last_failed_tests_line,
        last_test_files_line,
        last_box_table_top_line,
        stages,
    };
    (normalized, meta)
}

#[derive(Clone, Copy)]
struct DropLiveProgressState {
    is_dropping: bool,
}

fn normalize_crlf_and_carriage_returns(text: &str) -> String {
    text.replace("\u{1b}[2K\r", "\n")
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

fn normalize_tty_line(raw_line: &str, state: &mut DropLiveProgressState) -> Option<String> {
    let without_profile = filters::strip_headlamp_profile_suffix(raw_line);
    let stripped = headlamp::format::stacks::strip_ansi_simple(without_profile);
    let trimmed = stripped.trim_start();

    if trimmed.starts_with("RUN (+") {
        state.is_dropping = true;
        return None;
    }

    if state.is_dropping {
        let is_stable_header = trimmed.starts_with("RUN ")
            && !trimmed.starts_with("RUN (+")
            && !trimmed.starts_with("RUN [");
        // Some outputs report timings inline (e.g. `FAIL [123ms] ...`), so accept both
        // whitespace and bracket forms as stable status lines.
        let is_stable_status = trimmed.starts_with("FAIL ")
            || trimmed.starts_with("PASS ")
            || trimmed.starts_with("FAIL [")
            || trimmed.starts_with("PASS [");
        if !is_stable_header && !is_stable_status {
            return None;
        }
        state.is_dropping = false;
    }

    filters::should_keep_line_tty(raw_line).then(|| normalize_time_line_tty(without_profile))
}

pub fn normalize_tty_ui_with_meta(text: String, root: &Path) -> (String, NormalizationMeta) {
    let normalized_paths = paths::normalize_paths(text, root);
    let no_osc8 = filters::strip_osc8_sequences(&normalized_paths);
    let normalized_cr = normalize_crlf_and_carriage_returns(&no_osc8);

    let filtered_lines = normalized_cr
        .lines()
        .scan(
            DropLiveProgressState { is_dropping: false },
            |state, raw_line| Some(normalize_tty_line(raw_line, state)),
        )
        .flatten()
        .collect::<Vec<_>>();

    let filtered = filtered_lines.join("\n");
    let filtered = filters::drop_box_table_interior_blank_lines(&filtered);

    let final_block = tty_blocks::pick_final_render_block_tty(&filtered);
    let (normalized, used_fallback) = if final_block.trim().is_empty() {
        (
            common::trim_leading_blank_lines(&tty_blocks::pick_final_render_block_tty(
                &normalized_cr,
            )),
            true,
        )
    } else {
        (common::trim_leading_blank_lines(&final_block), false)
    };
    // Even if we had to fall back to a less-structured block picker, always:
    // - drop live-progress noise (RUN(+...), idle..., cursor controls)
    // - normalize the footer time line
    // so runner parity compares stable content.
    let normalized = normalized
        .lines()
        .filter_map(|raw_line| {
            let without_profile = filters::strip_headlamp_profile_suffix(raw_line);
            filters::should_keep_line_tty(raw_line)
                .then(|| normalize_time_line_tty(without_profile))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let normalized = normalize_tty_footer_spacing(&normalized);

    let (last_failed_tests_line, last_test_files_line, last_box_table_top_line) =
        common::compute_render_indices(&normalized);
    let stages = vec![
        common::stage_stats("normalized_paths", &normalized_paths),
        common::stage_stats("no_osc8", &no_osc8),
        common::stage_stats("normalized_cr", &normalized_cr),
        common::stage_stats("filtered", &filtered),
        common::stage_stats("normalized", &normalized),
    ];
    let meta = NormalizationMeta {
        normalizer: crate::parity_meta::NormalizerKind::TtyUi,
        used_fallback,
        last_failed_tests_line,
        last_test_files_line,
        last_box_table_top_line,
        stages,
    };
    (normalized, meta)
}

fn normalize_time_line_tty(raw: &str) -> String {
    let stripped = headlamp::format::stacks::strip_ansi_simple(raw);
    if !stripped.trim_start().starts_with("Time ") {
        return raw.to_string();
    }
    // Root cause fix: sometimes the following box-table border gets concatenated onto the `Time`
    // line by the TTY capture. If we normalize the whole `Time` line, we'd drop that border.
    // Instead, split at the first box-drawing character and keep it as its own line.
    if let Some(split_at) = BOX_CHAR_RE.find(&stripped).map(|m| m.start()) {
        let leading_ws = raw
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect::<String>();
        let bold_time = headlamp::format::ansi::bold("Time");
        let time_line = if raw.contains(&bold_time) {
            format!("{leading_ws}{bold_time}      <DURATION>")
        } else {
            format!("{leading_ws}Time      <DURATION>")
        };
        let tail = stripped[split_at..].to_string();
        return format!("{time_line}\n{tail}");
    }
    let leading_ws = raw
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect::<String>();
    let bold_time = headlamp::format::ansi::bold("Time");
    if raw.contains(&bold_time) {
        format!("{leading_ws}{bold_time}      <DURATION>")
    } else {
        format!("{leading_ws}Time      <DURATION>")
    }
}

fn normalize_tty_footer_spacing(text: &str) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());

    let mut index = 0usize;
    while index < lines.len() {
        if index > 0 && index + 1 < lines.len() {
            let current_is_blank = headlamp::format::stacks::strip_ansi_simple(lines[index])
                .trim()
                .is_empty();
            if current_is_blank {
                let prev = headlamp::format::stacks::strip_ansi_simple(lines[index - 1]);
                let next = headlamp::format::stacks::strip_ansi_simple(lines[index + 1]);
                let prev_is_tests = prev.trim_start().starts_with("Tests");
                let next_is_time = next.trim_start().starts_with("Time ");
                if prev_is_tests && next_is_time {
                    index += 1;
                    continue;
                }
            }
        }

        out.push(lines[index]);
        index += 1;
    }

    out.join("\n")
}
