use std::collections::BTreeSet;
use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::coverage::print::PrintOpts;
use crate::format::ansi;

use super::istanbul_text::{render_istanbul_text_report, render_istanbul_text_summary};
use super::merge::read_and_merge_coverage_final_json;
use super::per_file_table::render_per_file_composite_table;

pub fn format_istanbul_pretty(
    repo_root: &Path,
    coverage_root: &Path,
    print_opts: &PrintOpts,
    selection_paths_abs: &[String],
    include_globs: &[String],
    exclude_globs: &[String],
    coverage_detail: Option<crate::args::CoverageDetail>,
) -> Option<String> {
    let merged = read_and_merge_coverage_final_json(coverage_root, repo_root)?;
    Some(render_pretty_output(
        &merged,
        print_opts,
        selection_paths_abs,
        include_globs,
        exclude_globs,
        coverage_detail,
    ))
}

fn render_pretty_output(
    files: &[super::model::FullFileCoverage],
    print_opts: &PrintOpts,
    selection_paths_abs: &[String],
    include_globs: &[String],
    exclude_globs: &[String],
    coverage_detail: Option<crate::args::CoverageDetail>,
) -> String {
    let total_width = detect_columns();
    let sep_len = std::cmp::max(20, detect_columns_raw().unwrap_or(100));

    let detected_rows: u32 = detect_rows().unwrap_or(40);
    let rows_avail: u32 = if print_opts.page_fit {
        detected_rows.min(39)
    } else {
        detected_rows
    };
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
    let include_set = build_globset(include_globs);
    let exclude_set = build_globset(exclude_globs);
    sorted = sorted
        .into_iter()
        .filter(|file| {
            let rel = file.rel_path.replace('\\', "/");
            let included = include_set.as_ref().is_none()
                || include_set.as_ref().is_some_and(|s| s.is_match(&rel));
            let excluded = exclude_set.as_ref().is_some_and(|s| s.is_match(&rel));
            included && !excluded
        })
        .collect::<Vec<_>>();

    let files_for_text = sorted.clone();

    let mut out: Vec<String> = sorted
        .into_iter()
        .rev()
        .flat_map(|file| {
            render_per_file_composite_table(
                &file,
                per_file_rows,
                total_width,
                print_opts.max_hotspots,
                print_opts.tty,
            )
            .into_iter()
            .chain([crate::format::ansi::gray(&"─".repeat(sep_len))])
        })
        .collect::<Vec<_>>();

    // Istanbul's text reporter uses a stable layout even on wide terminals; cap width for parity.
    let istanbul_width = total_width.min(60);
    out.push(render_istanbul_text_report(&files_for_text, istanbul_width));
    out.push(String::new());
    out.push(render_istanbul_text_summary(&files_for_text));
    if let Some(detail) = coverage_detail
        && detail != crate::args::CoverageDetail::Auto
    {
        let detail_blocks = render_detail_blocks(&files_for_text, print_opts);
        if !detail_blocks.is_empty() {
            out.push(detail_blocks);
        }
    };

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

fn build_globset(globs: &[String]) -> Option<GlobSet> {
    if globs.is_empty() {
        return None;
    }
    let mut builder = GlobSetBuilder::new();
    globs
        .iter()
        .filter_map(|g| Glob::new(g).ok())
        .for_each(|glob| {
            builder.add(glob);
        });
    builder.build().ok()
}

fn render_detail_blocks(
    files: &[super::model::FullFileCoverage],
    print_opts: &PrintOpts,
) -> String {
    let mut files_sorted = files.to_vec();
    files_sorted.sort_by(|a, b| {
        let a_pct = super::analysis::file_summary(a).lines.pct();
        let b_pct = super::analysis::file_summary(b).lines.pct();
        a_pct
            .partial_cmp(&b_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.rel_path.cmp(&b.rel_path))
    });
    let joiner = "\n\n";
    files_sorted
        .into_iter()
        .filter_map(|file| render_detail_block(&file, print_opts))
        .collect::<Vec<_>>()
        .join(joiner)
}

fn render_detail_block(
    file: &super::model::FullFileCoverage,
    print_opts: &PrintOpts,
) -> Option<String> {
    let summary = super::analysis::file_summary(file);
    let hotspots = super::analysis::compute_uncovered_blocks(file);
    let missed_functions = super::analysis::missed_functions(file);
    if hotspots.is_empty() && missed_functions.is_empty() {
        return None;
    }

    let rel = file.rel_path.replace('\\', "/");
    let l_pct = summary.lines.pct();
    let f_pct = summary.functions.pct();
    let b_pct = summary.branches.pct();
    let bar = detail_bar(l_pct, print_opts.tty);

    let header = format!(
        "{}  lines {} {}  funcs {}  branches {}",
        ansi::bold(&rel),
        super::bars::tint_pct(l_pct, &format!("{l_pct:.1}%")),
        bar,
        super::bars::tint_pct(f_pct, &format!("{f_pct:.1}%")),
        super::bars::tint_pct(b_pct, &format!("{b_pct:.1}%")),
    );

    let max_hotspots = print_opts.max_hotspots.unwrap_or(5).max(1) as usize;
    Some(render_detail_block_lines(
        file,
        &header,
        &hotspots,
        max_hotspots,
        &missed_functions,
        print_opts,
    ))
}

fn render_detail_block_lines(
    file: &super::model::FullFileCoverage,
    header: &str,
    hotspots: &[super::model::UncoveredRange],
    max_hotspots: usize,
    missed_functions: &[super::model::MissedFunction],
    print_opts: &PrintOpts,
) -> String {
    if print_opts.tty {
        return render_detail_block_lines_tty(
            file,
            header,
            hotspots,
            max_hotspots,
            missed_functions,
            print_opts,
        );
    }
    render_detail_block_lines_plain(
        file,
        header,
        hotspots,
        max_hotspots,
        missed_functions,
        print_opts,
    )
}

fn render_detail_block_lines_tty(
    file: &super::model::FullFileCoverage,
    header: &str,
    hotspots: &[super::model::UncoveredRange],
    max_hotspots: usize,
    missed_functions: &[super::model::MissedFunction],
    print_opts: &PrintOpts,
) -> String {
    let mut out: Vec<String> = vec![header.to_string(), ansi::bold("  Hotspots:")];
    hotspots.iter().take(max_hotspots).for_each(|range| {
        let line_count = range.end - range.start + 1;
        let href = format_editor_link(file, range.start, print_opts);
        out.push(format!(
            "    - L{}–L{} ({} lines)  {}",
            range.start, range.end, line_count, href
        ));
    });
    out.push(ansi::bold("  Uncovered functions:"));
    missed_functions.iter().for_each(|missed| {
        let href = format_editor_link(file, missed.line, print_opts);
        out.push(format!("    - {} @ {}", missed.name, href));
    });
    out.join("\n")
}

fn render_detail_block_lines_plain(
    file: &super::model::FullFileCoverage,
    header: &str,
    hotspots: &[super::model::UncoveredRange],
    max_hotspots: usize,
    missed_functions: &[super::model::MissedFunction],
    print_opts: &PrintOpts,
) -> String {
    let mut out: Vec<String> = vec![header.to_string(), "  Hotspots:".to_string()];
    hotspots.iter().take(max_hotspots).for_each(|range| {
        let line_count = range.end - range.start + 1;
        let href = format_editor_link(file, range.start, print_opts);
        out.push(format!(
            "    - L{}–L{} ({} lines)  {}",
            range.start, range.end, line_count, href
        ));
    });
    out.push("  Uncovered functions:".to_string());
    missed_functions.iter().for_each(|missed| {
        let href = format_editor_link(file, missed.line, print_opts);
        out.push(format!("    - {} @ {}", missed.name, href));
    });
    out.join("\n")
}

fn format_editor_link(
    file: &super::model::FullFileCoverage,
    line: u32,
    print_opts: &PrintOpts,
) -> String {
    let rel_label = std::path::Path::new(&file.rel_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(file.rel_path.as_str())
        .to_string();
    let label = format!("{rel_label}:{line}");
    let Some(cmd) = resolve_editor_cmd(print_opts) else {
        return label;
    };
    let url = cmd
        .replace("{file}", &file.abs_path)
        .replace("{path}", &file.abs_path)
        .replace("{line}", &line.to_string());
    if !print_opts.tty {
        return format!("{label}<{url}>");
    }
    ansi::osc8(&label, &url)
}

fn resolve_editor_cmd(print_opts: &PrintOpts) -> Option<&str> {
    print_opts
        .editor_cmd
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .or_else(|| print_opts.tty.then_some("vscode://file/{file}:{line}"))
}

fn detail_bar(pct: f64, tty: bool) -> String {
    let total = 14usize;
    let filled = ((pct / 10.0).floor() as isize).clamp(0, total as isize) as usize;
    if !tty {
        let solid = "#".repeat(filled);
        let empty = "-".repeat(total.saturating_sub(filled));
        return format!("{solid}{empty}");
    }
    let solid_text = "█".repeat(filled);
    let empty_text = "░".repeat(total.saturating_sub(filled));
    format!(
        "{}{}",
        super::bars::tint_pct(pct, &solid_text),
        ansi::gray(&empty_text)
    )
}
