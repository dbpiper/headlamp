use std::collections::BTreeMap;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageReport {
    pub files: Vec<FileCoverage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileCoverage {
    pub path: String,
    pub lines_total: u32,
    pub lines_covered: u32,
    pub statements_total: Option<u32>,
    pub statements_covered: Option<u32>,
    pub statement_hits: Option<HashMap<u64, u32>>,
    pub uncovered_lines: Vec<u32>,
    pub line_hits: std::collections::BTreeMap<u32, u32>,
    pub function_hits: std::collections::BTreeMap<String, u32>,
    pub function_map: std::collections::BTreeMap<String, (String, u32)>,
    pub branch_hits: std::collections::BTreeMap<String, Vec<u32>>,
    pub branch_map: std::collections::BTreeMap<String, u32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Totals {
    pub lines_total: u32,
    pub lines_covered: u32,
}

impl Totals {
    pub fn pct(self) -> f64 {
        if self.lines_total == 0 {
            100.0
        } else {
            (self.lines_covered as f64 / self.lines_total as f64) * 100.0
        }
    }
}

impl CoverageReport {
    pub fn totals(&self) -> Totals {
        self.files.iter().fold(
            Totals {
                lines_total: 0,
                lines_covered: 0,
            },
            |acc, f| Totals {
                lines_total: acc.lines_total.saturating_add(f.lines_total),
                lines_covered: acc.lines_covered.saturating_add(f.lines_covered),
            },
        )
    }
}

impl FileCoverage {
    pub fn pct(&self) -> f64 {
        Totals {
            lines_total: self.lines_total,
            lines_covered: self.lines_covered,
        }
        .pct()
    }
}

pub fn apply_statement_totals_to_report(
    report: CoverageReport,
    statement_totals_by_path: &BTreeMap<String, (u32, u32)>,
) -> CoverageReport {
    let files = report
        .files
        .into_iter()
        .map(
            |file| match statement_totals_by_path.get(&file.path).copied() {
                Some((total, covered)) => FileCoverage {
                    statements_total: Some(total),
                    statements_covered: Some(covered),
                    ..file
                },
                None => file,
            },
        )
        .collect::<Vec<_>>();
    CoverageReport { files }
}

pub fn apply_statement_hits_to_report(
    report: CoverageReport,
    mut statement_hits_by_path: HashMap<String, HashMap<u64, u32>>,
) -> CoverageReport {
    let files = report
        .files
        .into_iter()
        .map(|file| match statement_hits_by_path.remove(&file.path) {
            Some(hits) => {
                let total = (hits.len() as u64).min(u64::from(u32::MAX)) as u32;
                let covered = (hits.values().filter(|hit| **hit > 0).count() as u64)
                    .min(u64::from(u32::MAX)) as u32;
                FileCoverage {
                    statements_total: Some(total),
                    statements_covered: Some(covered),
                    statement_hits: Some(hits),
                    ..file
                }
            }
            None => file,
        })
        .collect::<Vec<_>>();
    CoverageReport { files }
}
