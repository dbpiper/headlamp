use std::collections::BTreeSet;
use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::coverage::model::CoverageReport;
use crate::coverage::print::PrintOpts;
use crate::format::ansi;

use super::istanbul_text::{
    render_istanbul_text_report_with_totals_from_summaries,
    render_istanbul_text_summary_from_totals,
};
use super::merge::read_and_merge_coverage_final_json;
use super::model::FullFileCoverage;
use super::per_file_table::{build_per_file_table_layout, write_per_file_composite_table};

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
        merged,
        print_opts,
        selection_paths_abs,
        include_globs,
        exclude_globs,
        coverage_detail,
    ))
}

pub fn format_istanbul_pretty_from_lcov_report(
    repo_root: &Path,
    report: CoverageReport,
    print_opts: &PrintOpts,
    selection_paths_abs: &[String],
    include_globs: &[String],
    exclude_globs: &[String],
    coverage_detail: Option<crate::args::CoverageDetail>,
) -> String {
    let files = lcov_report_to_full_file_coverage(repo_root, report);

    render_pretty_output(
        files,
        print_opts,
        selection_paths_abs,
        include_globs,
        exclude_globs,
        coverage_detail,
    )
}

pub(super) fn lcov_report_to_full_file_coverage(
    repo_root: &Path,
    report: CoverageReport,
) -> Vec<FullFileCoverage> {
    report
        .files
        .into_iter()
        .map(|file| {
            let abs_path = file.path.replace('\\', "/");
            let rel_path = Path::new(&abs_path)
                .strip_prefix(repo_root)
                .ok()
                .and_then(|p| p.to_str())
                .unwrap_or(abs_path.as_str())
                .replace('\\', "/");

            // LCOV doesn't have Istanbul statement ranges; keep this empty and fall back to parsing
            // statement IDs (or line hits) when computing hotspots.
            let statement_map = std::collections::BTreeMap::new();

            FullFileCoverage {
                abs_path,
                rel_path,
                statement_hits: file.statement_hits.unwrap_or_default(),
                statement_map,
                function_hits: file.function_hits,
                function_map: file.function_map,
                branch_hits: file.branch_hits,
                branch_map: file.branch_map,
                line_hits: file.line_hits,
            }
        })
        .collect::<Vec<_>>()
}

fn render_pretty_output(
    mut files: Vec<super::model::FullFileCoverage>,
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

    files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

    let selection_set: BTreeSet<String> = selection_paths_abs
        .iter()
        .map(|p| p.replace('\\', "/"))
        .collect();
    if !selection_set.is_empty() {
        files.retain(|file| selection_set.contains(&file.abs_path));
    }
    let include_set = build_globset(include_globs);
    let exclude_set = build_globset(exclude_globs);
    files.retain(|file| {
        let included = include_set.as_ref().is_none()
            || include_set
                .as_ref()
                .is_some_and(|s| s.is_match(&file.rel_path));
        let excluded = exclude_set
            .as_ref()
            .is_some_and(|s| s.is_match(&file.rel_path));
        included && !excluded
    });

    let separator = crate::format::ansi::gray(&"─".repeat(sep_len));
    let per_file_layout = build_per_file_table_layout(total_width);

    // Build output as a single buffer. Pre-allocate using the size of the first rendered table
    // (all per-file tables have the same geometry for a given terminal size + print opts).
    let approx_visible_line_len =
        per_file_layout.widths.iter().sum::<usize>() + per_file_layout.widths.len() + 1 + 80; // ANSI escape overhead (conservative).
    let approx_lines_per_table = per_file_rows.saturating_add(8);
    let approx_bytes_per_file =
        (approx_visible_line_len + 1).saturating_mul(approx_lines_per_table) + separator.len() + 2;
    let mut out = String::with_capacity(approx_bytes_per_file.saturating_mul(files.len()) + 4096);

    let precomputed = files
        .iter()
        .map(super::analysis::file_summary)
        .collect::<Vec<_>>();

    let precomputed_blocks = files
        .iter()
        .map(super::analysis::compute_uncovered_blocks)
        .collect::<Vec<_>>();
    let precomputed_missed_functions = files
        .iter()
        .map(super::analysis::missed_functions)
        .collect::<Vec<_>>();
    let precomputed_missed_branches = files
        .iter()
        .map(super::analysis::missed_branches)
        .collect::<Vec<_>>();

    for (index, file) in files.iter().enumerate().rev() {
        let table_input = super::per_file_table::PerFileCompositeTableInput {
            file,
            summary: precomputed.get(index).unwrap(),
            blocks: precomputed_blocks.get(index).unwrap(),
            missed_functions: precomputed_missed_functions.get(index).unwrap(),
            missed_branches: precomputed_missed_branches.get(index).unwrap(),
            max_rows: per_file_rows,
            layout: &per_file_layout,
            max_hotspots: print_opts.max_hotspots,
            tty: print_opts.tty,
        };
        write_per_file_composite_table(&mut out, &table_input);
        out.push('\n');
        out.push_str(&separator);
        out.push('\n');
    }

    // Istanbul's text reporter uses a stable layout even on wide terminals; cap width for parity.
    let istanbul_width = total_width.min(60);
    let (istanbul_report, istanbul_totals) = render_istanbul_text_report_with_totals_from_summaries(
        &files,
        &precomputed,
        istanbul_width,
    );
    out.push_str(&istanbul_report);
    out.push('\n');
    out.push('\n');
    out.push_str(&render_istanbul_text_summary_from_totals(istanbul_totals));

    if let Some(detail) = coverage_detail
        && detail != crate::args::CoverageDetail::Auto
    {
        let detail_blocks = render_detail_blocks(&files, print_opts);
        if !detail_blocks.is_empty() {
            out.push('\n');
            out.push('\n');
            out.push_str(&detail_blocks);
        }
    };

    let trimmed_len = out.trim_end().len();
    out.truncate(trimmed_len);
    out
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
    let mut files_sorted = files.iter().collect::<Vec<_>>();
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
        .filter_map(|file| render_detail_block(file, print_opts))
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
