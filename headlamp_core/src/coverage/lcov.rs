use std::collections::BTreeMap;
use std::path::Path;

use lcov::Reader;
use lcov::Record;
use path_slash::PathExt;

use crate::coverage::model::{CoverageReport, FileCoverage};
use crate::error::HeadlampError;

#[derive(Debug, Clone)]
struct LcovFileBuf {
    path: String,
    hits: BTreeMap<u32, u32>,
}

#[derive(Debug, Default)]
struct LcovParseState {
    current: Option<LcovFileBuf>,
    files: Vec<FileCoverage>,
}

pub fn parse_lcov_text(text: &str) -> CoverageReport {
    let mut state = Reader::new(text.as_bytes()).filter_map(Result::ok).fold(
        LcovParseState::default(),
        |mut state, record| {
            match record {
                Record::SourceFile { path } => {
                    flush_current(&mut state);
                    state.current = Some(LcovFileBuf {
                        path: path.to_string_lossy().to_string(),
                        hits: BTreeMap::new(),
                    });
                }
                Record::LineData { line, count, .. } => {
                    let Some(buf) = state.current.as_mut() else {
                        return state;
                    };
                    let hit_count_u32 = count.min(u64::from(u32::MAX)) as u32;
                    let previous_hit_count = buf.hits.get(&line).copied().unwrap_or(0);
                    buf.hits
                        .insert(line, previous_hit_count.saturating_add(hit_count_u32));
                }
                Record::EndOfRecord => {
                    flush_current(&mut state);
                }
                _ => {}
            }
            state
        },
    );

    flush_current(&mut state);
    state.files.sort_by(|a, b| a.path.cmp(&b.path));
    CoverageReport { files: state.files }
}

pub fn read_lcov_file(path: &Path) -> Result<CoverageReport, HeadlampError> {
    let raw = std::fs::read_to_string(path).map_err(|source| HeadlampError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(parse_lcov_text(&raw))
}

fn flush_current(state: &mut LcovParseState) {
    let Some(buf) = state.current.take() else {
        return;
    };
    let (covered, total, uncovered) = compute_line_totals(&buf.hits);
    state.files.push(FileCoverage {
        path: buf.path,
        lines_total: total,
        lines_covered: covered,
        uncovered_lines: uncovered,
        line_hits: buf.hits,
    });
}

fn compute_line_totals(hits: &BTreeMap<u32, u32>) -> (u32, u32, Vec<u32>) {
    hits.iter()
        .fold((0u32, 0u32, Vec::<u32>::new()), |acc, (ln, h)| {
            let (covered, total, mut uncovered) = acc;
            let total2 = total.saturating_add(1);
            let covered2 = if *h > 0 {
                covered.saturating_add(1)
            } else {
                covered
            };
            if *h == 0 {
                uncovered.push(*ln);
            }
            (covered2, total2, uncovered)
        })
}

pub fn normalize_lcov_path(path: &str, root: &Path) -> String {
    let p = Path::new(path);
    if p.is_absolute() {
        return normalize_sep(p);
    }
    normalize_sep(&root.join(p))
}

fn normalize_sep(path: &Path) -> String {
    path.to_slash_lossy().to_string()
}

pub fn merge_reports(reports: &[CoverageReport], root: &Path) -> CoverageReport {
    let mut by_file: BTreeMap<String, BTreeMap<u32, u32>> = BTreeMap::new();
    for report in reports {
        for file in &report.files {
            let abs = normalize_lcov_path(&file.path, root);
            let entry = by_file.entry(abs).or_default();
            for (ln, hit) in &file.line_hits {
                let prev = entry.get(ln).copied().unwrap_or(0);
                entry.insert(*ln, prev.saturating_add(*hit));
            }
        }
    }

    let files = by_file
        .into_iter()
        .map(|(path, hits)| {
            let (covered, total, uncovered) = compute_line_totals(&hits);
            FileCoverage {
                path,
                lines_total: total,
                lines_covered: covered,
                uncovered_lines: uncovered,
                line_hits: hits,
            }
        })
        .collect::<Vec<_>>();

    CoverageReport { files }
}

pub fn resolve_lcov_paths_to_root(report: CoverageReport, root: &Path) -> CoverageReport {
    let files = report
        .files
        .into_iter()
        .map(|f| {
            let abs = normalize_lcov_path(&f.path, root);
            FileCoverage { path: abs, ..f }
        })
        .collect::<Vec<_>>();
    CoverageReport { files }
}
