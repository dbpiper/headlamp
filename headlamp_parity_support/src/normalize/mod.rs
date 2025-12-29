use std::path::Path;

use crate::parity_meta::NormalizationMeta;

mod blocks;
mod common;
mod filters;
mod paths;
mod runner_parity;
mod tty_blocks;

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

pub fn normalize_tty_ui_with_meta(text: String, root: &Path) -> (String, NormalizationMeta) {
    let normalized_paths = paths::normalize_paths(text, root);
    let no_osc8 = filters::strip_osc8_sequences(&normalized_paths);
    let normalized_cr = no_osc8
        .replace("\u{1b}[2K\r", "\n")
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let filtered = normalized_cr
        .lines()
        .filter_map(|raw_line| {
            let without_profile = filters::strip_headlamp_profile_suffix(raw_line);
            filters::should_keep_line_tty(raw_line).then(|| without_profile.to_string())
        })
        .collect::<Vec<_>>()
        .join("\n");
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
