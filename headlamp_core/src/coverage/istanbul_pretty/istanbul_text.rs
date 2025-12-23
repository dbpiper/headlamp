use std::collections::BTreeMap;
use std::path::Path;

use path_slash::PathExt;

use super::analysis::file_summary;
use super::model::{Counts, FileSummary, FullFileCoverage};

pub(super) fn render_istanbul_text_report(files: &[FullFileCoverage]) -> String {
    let mut rows: Vec<(String, FileSummary, String)> = files
        .iter()
        .map(|file| {
            let summary = file_summary(file);
            let uncovered = render_uncovered_line_numbers(&file.line_hits);
            let rel = Path::new(&file.rel_path).to_slash_lossy().to_string();
            let base = Path::new(&rel)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(rel.as_str())
                .to_string();
            (base, summary, uncovered)
        })
        .collect();

    rows.sort_by(|a, b| a.0.cmp(&b.0));

    let max_name_len = rows
        .iter()
        .map(|(name, _s, _u)| name.len())
        .max()
        .unwrap_or(0);
    let name_width = (max_name_len + 1).max(9);

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
        "{}|---------|----------|---------|---------|-------------------",
        "-".repeat(name_width + 1)
    );
    let header = format!(
        "{:<name_width$} | % Stmts | % Branch | % Funcs | % Lines | Uncovered Line #s ",
        "File"
    );
    let mut out: Vec<String> = vec![dash.to_string(), header.to_string(), dash.to_string()];

    out.push(render_istanbul_text_row(
        "All files",
        totals.statements,
        totals.branches,
        totals.functions,
        totals.lines,
        "",
        false,
        name_width,
    ));

    for (name, summary, uncovered) in rows {
        out.push(render_istanbul_text_row(
            &name,
            summary.statements,
            summary.branches,
            summary.functions,
            summary.lines,
            &uncovered,
            true,
            name_width,
        ));
    }

    out.push(dash.to_string());
    out.join("\n")
}

pub(super) fn render_istanbul_text_summary(files: &[FullFileCoverage]) -> String {
    let totals = files.iter().fold(
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
        |mut acc, file| {
            let s = file_summary(file);
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
    let pct = fmt_pct(counts.pct());
    format!(
        "{label:<13}: {pct}% ( {}/{} )",
        counts.covered, counts.total
    )
}

fn render_uncovered_line_numbers(line_hits: &BTreeMap<u32, u32>) -> String {
    let mut lines = line_hits
        .iter()
        .filter_map(|(ln, hit)| (*hit == 0).then_some(*ln))
        .collect::<Vec<_>>();
    lines.sort();
    if lines.is_empty() {
        return String::new();
    }
    let mut parts: Vec<String> = vec![];
    let mut i = 0usize;
    while i < lines.len() {
        let start = lines[i];
        let mut end = start;
        while i + 1 < lines.len() && lines[i + 1] == end + 1 {
            i += 1;
            end = lines[i];
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

fn render_istanbul_text_row(
    file_label: &str,
    stmts: Counts,
    branches: Counts,
    funcs: Counts,
    lines: Counts,
    uncovered: &str,
    indent_file: bool,
    name_width: usize,
) -> String {
    let file_cell = if indent_file {
        let inner = name_width.saturating_sub(1);
        format!(" {file_label:<inner$}")
    } else {
        format!("{file_label:<name_width$}")
    };

    let stmts_pct = fmt_pct(stmts.pct());
    let branches_pct = fmt_pct(branches.pct());
    let funcs_pct = fmt_pct(funcs.pct());
    let lines_pct = fmt_pct(lines.pct());
    format!(
        "{file_cell} | {stmts_pct:>7} | {branches_pct:>8} | {funcs_pct:>7} | {lines_pct:>7} | {uncovered:<17} "
    )
}

fn fmt_pct(pct: f64) -> String {
    let v = if pct.is_finite() { pct } else { 0.0 };
    let fixed = format!("{v:.2}");
    fixed
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}
