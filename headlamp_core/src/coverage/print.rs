use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};
use path_slash::PathExt;

use crate::coverage::model::{CoverageReport, FileCoverage};

#[derive(Debug, Clone)]
pub struct PrintOpts {
    pub max_files: Option<u32>,
    pub max_hotspots: Option<u32>,
    pub page_fit: bool,
    pub tty: bool,
    pub editor_cmd: Option<String>,
}

pub fn filter_report(
    report: CoverageReport,
    root: &Path,
    includes: &[String],
    excludes: &[String],
) -> CoverageReport {
    let include_set = build_globset(includes);
    let exclude_set = build_globset(excludes);
    let files = report
        .files
        .into_iter()
        .filter(|file| {
            let rel = path_rel_posix(&file.path, root);
            let included = include_set.as_ref().map_or(true, |s| s.is_match(&rel));
            let excluded = exclude_set.as_ref().map_or(false, |s| s.is_match(&rel));
            included && !excluded
        })
        .collect::<Vec<_>>();
    CoverageReport { files }
}

pub fn format_compact(report: &CoverageReport, opts: &PrintOpts, root: &Path) -> String {
    let mut files = report.files.clone();
    files.sort_by(|a, b| {
        a.pct()
            .partial_cmp(&b.pct())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let files = apply_max_files(files, opts.max_files);

    let header = format!("{:<6}  {:<8}  {}", "%Lines", "Uncov", "File");
    let mut lines = vec![header];

    for file in files {
        let rel = path_rel_posix(&file.path, root);
        let pct = file.pct();
        let uncov = file.lines_total.saturating_sub(file.lines_covered);
        lines.push(format!("{:>5.1}%  {:>8}  {}", pct, uncov, rel));
    }

    lines.join("\n")
}

pub fn format_hotspots(report: &CoverageReport, opts: &PrintOpts, root: &Path) -> String {
    let max = opts.max_hotspots.unwrap_or(5).max(1) as usize;
    let mut out: Vec<String> = vec![];

    let mut files = report.files.clone();
    files.sort_by(|a, b| {
        a.pct()
            .partial_cmp(&b.pct())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let files = apply_max_files(files, opts.max_files);

    for file in files {
        let rel = path_rel_posix(&file.path, root);
        let uncovered = file
            .uncovered_lines
            .iter()
            .copied()
            .take(max)
            .collect::<Vec<_>>();
        if uncovered.is_empty() {
            continue;
        }
        let mut line = format!("{rel}: ");
        let joined = uncovered
            .into_iter()
            .map(|ln| format_line_link(&file.path, ln, opts))
            .collect::<Vec<_>>()
            .join(", ");
        line.push_str(&joined);
        out.push(line);
    }

    out.join("\n")
}

pub fn format_summary(report: &CoverageReport) -> String {
    let totals = report.totals();
    format!(
        "Lines: {:.1}% ({}/{})",
        totals.pct(),
        totals.lines_covered,
        totals.lines_total
    )
}

fn apply_max_files(mut files: Vec<FileCoverage>, max: Option<u32>) -> Vec<FileCoverage> {
    let Some(m) = max else {
        return files;
    };
    let take = (m.max(1)) as usize;
    files.truncate(take);
    files
}

fn build_globset(globs: &[String]) -> Option<GlobSet> {
    if globs.is_empty() {
        return None;
    }
    let mut builder = GlobSetBuilder::new();
    for g in globs {
        if let Ok(glob) = Glob::new(g) {
            builder.add(glob);
        }
    }
    builder.build().ok()
}

fn path_rel_posix(abs_or_rel: &str, root: &Path) -> String {
    let p = Path::new(abs_or_rel);
    let rel = p
        .strip_prefix(root)
        .ok()
        .and_then(|x| x.to_str())
        .unwrap_or(abs_or_rel);
    Path::new(rel).to_slash_lossy().to_string()
}

fn format_line_link(file: &str, line: u32, opts: &PrintOpts) -> String {
    let label = format!("{line}");
    let Some(cmd) = opts
        .editor_cmd
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    else {
        return label;
    };
    let url = cmd
        .replace("{file}", file)
        .replace("{path}", file)
        .replace("{line}", &line.to_string());
    if !opts.tty {
        return format!("{label}<{url}>");
    }
    format!("\x1b]8;;{url}\x1b\\{label}\x1b]8;;\x1b\\")
}
