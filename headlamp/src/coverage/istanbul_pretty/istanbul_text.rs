use std::collections::BTreeMap;

use super::bars::tint_pct;
use super::model::{Counts, FileSummary, FullFileCoverage};
use super::path_shorten::shorten_path_preserving_filename;

pub(super) fn render_istanbul_text_report_with_totals_from_summaries(
    files: &[FullFileCoverage],
    summaries: &[FileSummary],
    max_cols: usize,
) -> (String, FileSummary) {
    let rows: Vec<(String, FileSummary, String)> = files
        .iter()
        .zip(summaries.iter())
        .map(|(file, summary)| {
            let uncovered = render_uncovered_line_numbers(&file.line_hits);
            let rel: std::borrow::Cow<'_, str> = if file.rel_path.contains('\\') {
                std::borrow::Cow::Owned(file.rel_path.replace('\\', "/"))
            } else {
                std::borrow::Cow::Borrowed(file.rel_path.as_str())
            };
            (rel.to_string(), summary.clone(), uncovered)
        })
        .collect();
    render_istanbul_text_report_with_totals_from_rows(rows, max_cols)
}

#[cfg(test)]
pub(super) fn render_istanbul_text_report_with_totals(
    files: &[FullFileCoverage],
    max_cols: usize,
) -> (String, FileSummary) {
    use std::path::Path;

    use path_slash::PathExt;

    use super::analysis::file_summary;

    let rows: Vec<(String, FileSummary, String)> = files
        .iter()
        .map(|file| {
            let summary = file_summary(file);
            let uncovered = render_uncovered_line_numbers(&file.line_hits);
            let rel = Path::new(&file.rel_path).to_slash_lossy().to_string();
            (rel, summary, uncovered)
        })
        .collect();
    render_istanbul_text_report_with_totals_from_rows(rows, max_cols)
}

fn render_istanbul_text_report_with_totals_from_rows(
    mut rows: Vec<(String, FileSummary, String)>,
    max_cols: usize,
) -> (String, FileSummary) {
    let total_rows = rows.len();
    rows.sort_by(|a, b| a.0.cmp(&b.0));

    let max_name_len = rows
        .iter()
        .map(|(name, _s, _u)| name.chars().count().saturating_add(1))
        .max()
        .unwrap_or(0);
    let (file_width, missing_width) = compute_table_widths(max_name_len, max_cols);
    let header_file_width = file_width.saturating_sub(1);

    let totals = rows.iter().fold(
        FileSummary {
            statements: Counts {
                covered: 0,
                total: 0,
            },
            branches: Counts {
                covered: 0,
                total: 0,
            },
            functions: Counts {
                covered: 0,
                total: 0,
            },
            lines: Counts {
                covered: 0,
                total: 0,
            },
        },
        |mut acc, (_name, s, _u)| {
            acc.statements.covered = acc.statements.covered.saturating_add(s.statements.covered);
            acc.statements.total = acc.statements.total.saturating_add(s.statements.total);
            acc.branches.covered = acc.branches.covered.saturating_add(s.branches.covered);
            acc.branches.total = acc.branches.total.saturating_add(s.branches.total);
            acc.functions.covered = acc.functions.covered.saturating_add(s.functions.covered);
            acc.functions.total = acc.functions.total.saturating_add(s.functions.total);
            acc.lines.covered = acc.lines.covered.saturating_add(s.lines.covered);
            acc.lines.total = acc.lines.total.saturating_add(s.lines.total);
            acc
        },
    );

    // Match Istanbul text reporter formatting (as used by headlamp-original).
    let dash = format!(
        "{}|---------|----------|---------|---------|{}",
        "-".repeat(file_width),
        "-".repeat(missing_width)
    );
    let uncovered_header_cell = istanbul_fill("Uncovered Line #s", missing_width, false, 1);
    let header = format!(
        "{:<header_file_width$} | % Stmts | % Branch | % Funcs | % Lines |{uncovered_header_cell}",
        "File"
    );
    let approx_line_width = file_width + 1 + 9 + 1 + 10 + 1 + 9 + 1 + 9 + 1 + missing_width;
    let mut report = String::with_capacity(approx_line_width.saturating_mul(rows.len() + 6));

    report.push_str(&dash);
    report.push('\n');
    report.push_str(&header);
    report.push('\n');
    report.push_str(&dash);
    report.push('\n');

    report.push_str(&render_istanbul_text_row(
        "All files",
        totals.statements,
        totals.branches,
        totals.functions,
        totals.lines,
        "",
        IstanbulTextRowLayout {
            indent_file: false,
            file_width,
            missing_width,
        },
    ));
    report.push('\n');

    for (index, (name, summary, uncovered)) in rows.into_iter().enumerate() {
        report.push_str(&render_istanbul_text_row(
            &name,
            summary.statements,
            summary.branches,
            summary.functions,
            summary.lines,
            &uncovered,
            IstanbulTextRowLayout {
                indent_file: true,
                file_width,
                missing_width,
            },
        ));
        if index + 1 < total_rows {
            report.push('\n');
        }
    }
    report.push('\n');
    report.push_str(&dash);
    (report, totals)
}

pub(super) fn render_istanbul_text_summary_from_totals(totals: FileSummary) -> String {
    let top = "=============================== Coverage summary ===============================";
    let bot = "================================================================================";
    [
        top.to_string(),
        format_summary_line("Statements", totals.statements),
        format_summary_line("Branches", totals.branches),
        format_summary_line("Functions", totals.functions),
        format_summary_line("Lines", totals.lines),
        bot.to_string(),
    ]
    .join("\n")
}

fn format_summary_line(label: &str, counts: Counts) -> String {
    let pct = if counts.total == 0 {
        "N/A".to_string()
    } else {
        fmt_pct(counts.pct())
    };
    let label_pad = 13usize.saturating_sub(label.chars().count());
    let pct_for_label = if counts.total == 0 {
        100.0
    } else {
        counts.pct()
    };
    let pct_for_counts = if counts.total == 0 { 0.0 } else { counts.pct() };
    let label_colored = tint_pct(pct_for_label, label);
    let pct_str = if counts.total == 0 {
        pct
    } else {
        format!("{pct}%")
    };
    let pct_colored = tint_pct(pct_for_label, &pct_str);
    let counts_colored = tint_pct(
        pct_for_counts,
        &format!("( {}/{} )", counts.covered, counts.total),
    );
    format!(
        "{label_colored}{}: {pct_colored} {counts_colored}",
        " ".repeat(label_pad)
    )
}

fn render_uncovered_line_numbers(line_hits: &BTreeMap<u32, u32>) -> String {
    let any_uncovered = line_hits.values().any(|hit| *hit == 0);
    if !any_uncovered {
        return String::new();
    }
    let mut uncovered_lines = line_hits
        .iter()
        .filter_map(|(ln, hit)| (*hit == 0).then_some(*ln))
        .collect::<Vec<_>>();
    uncovered_lines.sort();
    if uncovered_lines.is_empty() {
        return String::new();
    }

    let is_all_uncovered = line_hits.values().all(|hit| *hit == 0);
    if is_all_uncovered {
        let start = uncovered_lines.first().copied().unwrap_or(0);
        let end = uncovered_lines.last().copied().unwrap_or(start);
        if start == end {
            return start.to_string();
        }
        return format!("{start}-{end}");
    }
    let mut parts: Vec<String> = vec![];
    let mut i = 0usize;
    while i < uncovered_lines.len() {
        let start = uncovered_lines[i];
        let mut end = start;
        while i + 1 < uncovered_lines.len() && uncovered_lines[i + 1] == end + 1 {
            i += 1;
            end = uncovered_lines[i];
        }
        if start == end {
            parts.push(format!("{start}"));
        } else {
            parts.push(format!("{start}-{end}"));
        }
        i += 1;
    }
    parts.join(",")
}

#[derive(Debug, Clone, Copy)]
struct IstanbulTextRowLayout {
    indent_file: bool,
    file_width: usize,
    missing_width: usize,
}

fn render_istanbul_text_row(
    file_label: &str,
    stmts: Counts,
    branches: Counts,
    funcs: Counts,
    lines: Counts,
    uncovered: &str,
    layout: IstanbulTextRowLayout,
) -> String {
    let file_cell = {
        let leader_spaces = if layout.indent_file { 1 } else { 0 };
        let remaining = layout.file_width.saturating_sub(leader_spaces);
        let shortened = shorten_path_preserving_filename(file_label, remaining.max(1));
        istanbul_fill(&shortened, layout.file_width, false, leader_spaces)
    };

    let stmts_pct = fmt_pct(stmts.pct());
    let branches_pct = if branches.total == 0 {
        "N/A".to_string()
    } else {
        fmt_pct(branches.pct())
    };
    let funcs_pct = fmt_pct(funcs.pct());
    let lines_pct = fmt_pct(lines.pct());
    let row_min = stmts
        .pct()
        .min(branches.pct())
        .min(funcs.pct())
        .min(lines.pct());

    let file_cell_colored = tint_pct(row_min, &file_cell);
    let stmts_cell = tint_pct(stmts.pct(), &format!(" {stmts_pct:>7} "));
    let branches_cell = tint_pct(branches.pct(), &format!(" {branches_pct:>8} "));
    let funcs_cell = tint_pct(funcs.pct(), &format!(" {funcs_pct:>7} "));
    let lines_cell = tint_pct(lines.pct(), &format!(" {lines_pct:>7} "));
    let uncovered_cell = tint_pct(
        row_min,
        &istanbul_fill(uncovered, layout.missing_width, false, 1),
    );

    format!(
        "{file_cell_colored}|{stmts_cell}|{branches_cell}|{funcs_cell}|{lines_cell}|{uncovered_cell}"
    )
}

fn compute_table_widths(max_name_len: usize, max_cols: usize) -> (usize, usize) {
    let file_width = max_name_len.saturating_add(1).max(9 + 1);
    let fixed = 9usize + 10usize + 9usize + 9usize + 5usize;
    let min_missing = 19usize;

    if max_cols > fixed + min_missing {
        let desired_missing = max_cols.saturating_sub(fixed + file_width);
        let missing_width = desired_missing.max(min_missing);
        (file_width, missing_width)
    } else {
        (file_width, min_missing)
    }
}

fn istanbul_fill(text: &str, width: usize, align_right: bool, leading_spaces: usize) -> String {
    let leader = " ".repeat(leading_spaces.min(width));
    let remaining = width.saturating_sub(leader.chars().count());
    if remaining == 0 {
        return leader;
    }

    let text_len = text.chars().count();
    if text_len <= remaining {
        let pad = " ".repeat(remaining - text_len);
        return if align_right {
            format!("{leader}{pad}{text}")
        } else {
            format!("{leader}{text}{pad}")
        };
    }

    let ellipsis = "...";
    let tail_len = remaining.saturating_sub(ellipsis.chars().count());
    let tail = text
        .chars()
        .rev()
        .take(tail_len)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("{leader}{ellipsis}{tail}")
}

fn fmt_pct(pct: f64) -> String {
    let v = if pct.is_finite() { pct } else { 0.0 };
    let floored = (v * 100.0).floor() / 100.0;
    let fixed = format!("{floored:.2}");
    fixed
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}
