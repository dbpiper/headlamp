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
    let total_width = 100usize;

    let rows_avail: u32 = if print_opts.page_fit { 39 } else { 40 };
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
            .chain([String::from("─").repeat(total_width)])
        })
        .collect::<Vec<_>>();

    out.push(render_istanbul_text_report(&files_for_text));
    out.push(String::new());
    out.push(render_istanbul_text_summary(&files_for_text));

    out.join("\n").trim_end().to_string()
}
