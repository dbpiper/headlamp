use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageReport {
    pub files: Vec<FileCoverage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileCoverage {
    pub path: String,
    pub lines_total: u32,
    pub lines_covered: u32,
    pub uncovered_lines: Vec<u32>,
    pub line_hits: BTreeMap<u32, u32>,
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
