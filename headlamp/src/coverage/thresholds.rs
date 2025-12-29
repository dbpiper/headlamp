use std::cmp::Ordering;

use indexmap::IndexSet;

use crate::config::CoverageThresholds;
use crate::coverage::model::CoverageReport;

#[derive(Debug, Clone, Copy)]
pub struct MetricCounts {
    pub covered: u32,
    pub total: u32,
}

impl MetricCounts {
    pub fn pct(self) -> f64 {
        if self.total == 0 {
            100.0
        } else {
            (self.covered as f64 / self.total as f64) * 100.0
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CoverageTotals {
    pub statements: MetricCounts,
    pub branches: MetricCounts,
    pub functions: MetricCounts,
    pub lines: MetricCounts,
}

pub fn compute_totals_from_report(report: &CoverageReport) -> CoverageTotals {
    let lines_total = report.files.iter().map(|f| f.lines_total).sum::<u32>();
    let lines_covered = report.files.iter().map(|f| f.lines_covered).sum::<u32>();

    let functions_total = report
        .files
        .iter()
        .map(|f| f.function_hits.len() as u32)
        .sum::<u32>();
    let functions_covered = report
        .files
        .iter()
        .map(|f| f.function_hits.values().filter(|h| **h > 0).count() as u32)
        .sum::<u32>();

    let (branches_total, branches_covered) =
        report
            .files
            .iter()
            .fold((0u32, 0u32), |(total, covered), f| {
                let (t, c) = f.branch_hits.values().fold((0u32, 0u32), |acc, hits| {
                    let (t0, c0) = acc;
                    let t1 = t0.saturating_add(hits.len() as u32);
                    let c1 = c0.saturating_add(hits.iter().filter(|h| **h > 0).count() as u32);
                    (t1, c1)
                });
                (total.saturating_add(t), covered.saturating_add(c))
            });

    let (statements_total, statements_covered) =
        report
            .files
            .iter()
            .fold((0u32, 0u32), |(total, covered), file| {
                match (file.statements_total, file.statements_covered) {
                    (Some(statements_total), Some(statements_covered)) => (
                        total.saturating_add(statements_total),
                        covered.saturating_add(statements_covered),
                    ),
                    _ => (
                        total.saturating_add(file.lines_total),
                        covered.saturating_add(file.lines_covered),
                    ),
                }
            });

    CoverageTotals {
        statements: MetricCounts {
            covered: statements_covered,
            total: statements_total,
        },
        branches: MetricCounts {
            covered: branches_covered,
            total: branches_total,
        },
        functions: MetricCounts {
            covered: functions_covered,
            total: functions_total,
        },
        lines: MetricCounts {
            covered: lines_covered,
            total: lines_total,
        },
    }
}

pub fn threshold_failure_lines(
    thresholds: &CoverageThresholds,
    totals: CoverageTotals,
) -> IndexSet<String> {
    let mut out: IndexSet<String> = IndexSet::new();

    push_if_short(
        &mut out,
        "Statements",
        thresholds.statements,
        totals.statements,
    );
    push_if_short(&mut out, "Branches", thresholds.branches, totals.branches);
    push_if_short(
        &mut out,
        "Functions",
        thresholds.functions,
        totals.functions,
    );
    push_if_short(&mut out, "Lines", thresholds.lines, totals.lines);

    out
}

pub fn print_threshold_failure_summary(lines: &IndexSet<String>) {
    println!();
    println!("Coverage thresholds not met");
    lines.iter().for_each(|line| println!(" {line}"));
}

pub fn compare_thresholds_and_print_if_needed(
    thresholds: Option<&CoverageThresholds>,
    report: Option<&CoverageReport>,
) -> bool {
    let Some(thresholds) = thresholds else {
        return false;
    };
    let Some(report) = report else {
        return false;
    };

    let totals = compute_totals_from_report(report);
    let lines = threshold_failure_lines(thresholds, totals);
    if lines.is_empty() {
        return false;
    }
    print_threshold_failure_summary(&lines);
    true
}

fn push_if_short(
    out: &mut IndexSet<String>,
    label: &str,
    threshold: Option<f64>,
    counts: MetricCounts,
) {
    let Some(expected) = threshold else {
        return;
    };
    let actual = counts.pct();
    if actual.partial_cmp(&expected).unwrap_or(Ordering::Equal) != Ordering::Less {
        return;
    }
    let short = (expected - actual).max(0.0);
    out.insert(format!(
        "{label}: {actual:.2}% < {expected:.0}% (short {short:.2}%)"
    ));
}
