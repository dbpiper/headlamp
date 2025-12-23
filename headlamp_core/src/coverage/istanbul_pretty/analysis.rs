use std::collections::BTreeSet;

use super::model::{
    Counts, FileSummary, FullFileCoverage, MissedBranch, MissedFunction, UncoveredRange,
};

pub(super) fn file_summary(file: &FullFileCoverage) -> FileSummary {
    let statement_total = file.statement_hits.len() as u32;
    let statement_covered = file.statement_hits.values().filter(|v| **v > 0).count() as u32;

    let function_total = file.function_hits.len() as u32;
    let function_covered = file.function_hits.values().filter(|v| **v > 0).count() as u32;

    let (branch_total, branch_covered) =
        file.branch_hits.values().fold((0u32, 0u32), |acc, arr| {
            let (t, c) = acc;
            let total = t.saturating_add(arr.len() as u32);
            let covered = c.saturating_add(arr.iter().filter(|h| **h > 0).count() as u32);
            (total, covered)
        });

    let line_total = file.line_hits.len() as u32;
    let line_covered = file.line_hits.values().filter(|v| **v > 0).count() as u32;

    FileSummary {
        statements: Counts {
            covered: statement_covered,
            total: statement_total,
        },
        branches: Counts {
            covered: branch_covered,
            total: branch_total,
        },
        functions: Counts {
            covered: function_covered,
            total: function_total,
        },
        lines: Counts {
            covered: line_covered,
            total: line_total,
        },
    }
}

pub(super) fn compute_uncovered_blocks(file: &FullFileCoverage) -> Vec<UncoveredRange> {
    let missed_lines: BTreeSet<u32> = file
        .statement_hits
        .iter()
        .filter(|(_id, hit)| **hit == 0)
        .filter_map(|(id, _)| file.statement_map.get(id).copied())
        .flat_map(|(start, end)| {
            let from = start.max(1);
            let to = end.max(from);
            (from..=to).collect::<Vec<_>>()
        })
        .collect();

    let lines = missed_lines.into_iter().collect::<Vec<_>>();
    let mut ranges: Vec<UncoveredRange> = vec![];
    let mut index = 0usize;
    while index < lines.len() {
        let start = lines[index];
        let mut end = start;
        while index + 1 < lines.len() && lines[index + 1] == end + 1 {
            index += 1;
            end = lines[index];
        }
        ranges.push(UncoveredRange { start, end });
        index += 1;
    }
    ranges.sort_by(|a, b| {
        (b.end - b.start)
            .cmp(&(a.end - a.start))
            .then_with(|| a.start.cmp(&b.start))
    });
    ranges
}

pub(super) fn missed_functions(file: &FullFileCoverage) -> Vec<MissedFunction> {
    let mut out: Vec<MissedFunction> = file
        .function_hits
        .iter()
        .filter(|(_id, hit)| **hit == 0)
        .map(|(id, _)| {
            let (name, line) = file
                .function_map
                .get(id)
                .cloned()
                .unwrap_or_else(|| ("(anonymous)".to_string(), 0));
            MissedFunction { name, line }
        })
        .collect();
    out.sort_by(|a, b| a.line.cmp(&b.line));
    out
}

pub(super) fn missed_branches(file: &FullFileCoverage) -> Vec<MissedBranch> {
    let mut out: Vec<MissedBranch> = file
        .branch_hits
        .iter()
        .filter_map(|(id, hits)| {
            let zeros = hits
                .iter()
                .enumerate()
                .filter_map(|(index, hit)| (*hit == 0).then_some(index as u32))
                .collect::<Vec<_>>();
            (!zeros.is_empty()).then(|| MissedBranch {
                id: id.clone(),
                line: file.branch_map.get(id).copied().unwrap_or(0),
                zero_paths: zeros,
            })
        })
        .collect();
    out.sort_by(|a, b| a.line.cmp(&b.line));
    out
}

pub(super) fn composite_bar_pct(summary: &FileSummary, hotspots: &[UncoveredRange]) -> f64 {
    let base = summary
        .lines
        .pct()
        .min(summary.functions.pct())
        .min(summary.branches.pct());
    let total_lines = summary.lines.total;
    let penalty = (total_lines > 0 && !hotspots.is_empty())
        .then(|| {
            let largest = hotspots
                .iter()
                .map(|r| r.end - r.start + 1)
                .max()
                .unwrap_or(0) as f64;
            let concentration = largest / (total_lines as f64);
            ((concentration * 100.0 * 0.5).round() as i64).min(15)
        })
        .unwrap_or(0);
    (base - (penalty as f64)).clamp(0.0, 100.0)
}
