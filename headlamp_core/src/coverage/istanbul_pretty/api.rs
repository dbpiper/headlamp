use std::collections::BTreeSet;
use std::path::Path;

use crate::coverage::print::PrintOpts;

use super::istanbul_text::{render_istanbul_text_report, render_istanbul_text_summary};
use super::merge::read_and_merge_coverage_final_json;
use super::per_file_table::render_per_file_composite_table;

pub fn format_istanbul_pretty(
    repo_root: &Path,
    coverage_root: &Path,
    print_opts: &PrintOpts,
    selection_paths_abs: &[String],
) -> Option<String> {
    let merged = read_and_merge_coverage_final_json(coverage_root, repo_root)?;
    Some(render_pretty_output(
        &merged,
        print_opts,
        selection_paths_abs,
    ))
}

fn render_pretty_output(
    files: &[super::model::FullFileCoverage],
    print_opts: &PrintOpts,
    selection_paths_abs: &[String],
) -> String {
    let total_width = detect_columns();
    let sep_len = std::cmp::max(20, detect_columns_raw().unwrap_or(100));

    let rows_avail: u32 = detect_rows().unwrap_or(if print_opts.page_fit { 39 } else { 40 });
    let per_file_rows = if print_opts.page_fit {
        (rows_avail.saturating_sub(1)).max(14)
    } else {
        rows_avail.saturating_add(8)
    } as usize;

    let mut sorted = files.to_vec();
    sorted.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

    let selection_set: BTreeSet<String> = selection_paths_abs
        .iter()
        .map(|p| p.replace('\\', "/"))
        .collect();
    if !selection_set.is_empty() {
        sorted = sorted
            .into_iter()
            .filter(|file| selection_set.contains(&file.abs_path))
            .collect::<Vec<_>>();
    }
    let files_for_text = if selection_set.is_empty() {
        files.to_vec()
    } else {
        sorted.clone()
    };

    let mut out: Vec<String> = sorted
        .into_iter()
        .rev()
        .flat_map(|file| {
            render_per_file_composite_table(
                &file,
                per_file_rows,
                total_width,
                print_opts.max_hotspots,
            )
            .into_iter()
            .chain([crate::format::ansi::gray(&"─".repeat(sep_len))])
        })
        .collect::<Vec<_>>();

    // Keep Istanbul text table width aligned with the detected terminal width (TS parity).
    out.push(render_istanbul_text_report(&files_for_text, total_width));
    out.push(String::new());
    out.push(render_istanbul_text_summary(&files_for_text));

    out.join("\n").trim_end().to_string()
}

fn detect_columns() -> usize {
    let cols = detect_columns_raw().unwrap_or(0);
    if cols > 20 { cols.max(60) } else { 100 }
}

fn detect_columns_raw() -> Option<usize> {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .or_else(|| crate::format::terminal::detect_terminal_size_cols_rows().map(|(w, _)| w))
}

fn detect_rows() -> Option<u32> {
    crate::format::terminal::detect_terminal_size_cols_rows().map(|(_w, h)| h as u32)
}
