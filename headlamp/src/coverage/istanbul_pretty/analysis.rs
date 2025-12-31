use super::model::{
    Counts, FileSummary, FullFileCoverage, MissedBranch, MissedFunction, UncoveredRange,
};

use crate::coverage::statement_id::statement_id_line;

fn format_coverage_function_name(raw: &str) -> String {
    fn strip_bracketed_disambiguators(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut skipping = false;
        for character in text.chars() {
            if skipping {
                if character == ']' {
                    skipping = false;
                }
                continue;
            }
            if character == '[' {
                skipping = true;
                continue;
            }
            out.push(character);
        }
        out
    }

    fn keep_last_segments(demangled: &str, keep: usize) -> String {
        let segments = demangled.split("::").collect::<Vec<_>>();
        let tail = segments
            .len()
            .saturating_sub(keep.max(1))
            .min(segments.len());
        segments[tail..].join("::")
    }

    if !raw.starts_with("_R") {
        return raw.to_string();
    }

    let demangled = rustc_demangle::demangle(raw).to_string();
    if demangled == raw {
        return raw.to_string();
    }
    let demangled = strip_bracketed_disambiguators(&demangled);
    keep_last_segments(&demangled, 3)
}

pub(super) fn file_summary(file: &FullFileCoverage) -> FileSummary {
    let (statement_total, statement_covered) = if file.statement_hits.is_empty() {
        let total = file.line_hits.len() as u32;
        let covered = file.line_hits.values().filter(|v| **v > 0).count() as u32;
        (total, covered)
    } else {
        let total = file.statement_hits.len() as u32;
        let covered = file.statement_hits.values().filter(|v| **v > 0).count() as u32;
        (total, covered)
    };

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
    let mut lines = if file.statement_hits.is_empty() {
        file.line_hits
            .iter()
            .filter_map(|(line, hit)| (*hit == 0).then_some(*line))
            .collect::<Vec<_>>()
    } else {
        let mut out: Vec<u32> = Vec::with_capacity(file.statement_hits.len().min(4096));
        let mut single_line: Option<u32> = None;
        let mut has_multiple_lines = false;
        for (id, hit) in &file.statement_hits {
            if *hit != 0 {
                continue;
            }
            if let Some((start, end)) = file.statement_map.get(id).copied() {
                let from = start.max(1);
                let to = end.max(from);
                for line in from..=to {
                    match single_line {
                        None => {
                            single_line = Some(line);
                            out.push(line);
                        }
                        Some(existing) if existing == line => {}
                        Some(_existing) => {
                            has_multiple_lines = true;
                            out.push(line);
                        }
                    }
                }
                continue;
            }
            let line = statement_id_line(*id);
            if line > 0 {
                match single_line {
                    None => {
                        single_line = Some(line);
                        out.push(line);
                    }
                    Some(existing) if existing == line => {}
                    Some(_existing) => {
                        has_multiple_lines = true;
                        out.push(line);
                    }
                }
            }
        }
        if !out.is_empty() && !has_multiple_lines {
            let line = out[0];
            return vec![UncoveredRange {
                start: line,
                end: line,
            }];
        }
        out
    };
    lines.sort_unstable();
    lines.dedup();

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
            MissedFunction {
                name: format_coverage_function_name(&name),
                line,
            }
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
    let penalty = if total_lines > 0 && !hotspots.is_empty() {
        {
            let largest = hotspots
                .iter()
                .map(|r| r.end - r.start + 1)
                .max()
                .unwrap_or(0) as f64;
            let concentration = largest / (total_lines as f64);
            ((concentration * 100.0 * 0.5).round() as i64).min(15)
        }
    } else {
        0
    };
    (base - (penalty as f64)).clamp(0.0, 100.0)
}
