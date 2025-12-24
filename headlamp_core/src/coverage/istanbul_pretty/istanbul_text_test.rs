use std::collections::BTreeMap;

use crate::format::stacks::strip_ansi_simple;

use super::istanbul_text::render_istanbul_text_report;
use super::model::FullFileCoverage;

fn make_file_with_long_uncovered_list() -> FullFileCoverage {
    let line_hits = (1u32..=220)
        .map(|line_number| {
            let hits = (line_number % 2 == 1).then_some(0).unwrap_or(1);
            (line_number, hits)
        })
        .collect::<BTreeMap<_, _>>();

    FullFileCoverage {
        abs_path: "/abs/path/to/src/utils/logging/httpLog.ts".to_string(),
        rel_path: "src/utils/logging/httpLog.ts".to_string(),
        statement_hits: BTreeMap::new(),
        statement_map: BTreeMap::new(),
        function_hits: BTreeMap::new(),
        function_map: BTreeMap::new(),
        branch_hits: BTreeMap::new(),
        branch_map: BTreeMap::new(),
        line_hits,
    }
}

#[test]
fn istanbul_text_report_truncates_uncovered_column_to_avoid_overflow() {
    let report = render_istanbul_text_report(&[make_file_with_long_uncovered_list()], 95);
    let lines = report.lines().map(strip_ansi_simple).collect::<Vec<_>>();
    let max_width = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);

    // All rows should have the same width as the header/dash lines, not grow due to uncovered lines.
    assert!(
        lines.iter().all(|line| line.chars().count() == max_width),
        "expected all lines to be fixed-width, but got:\n{report}"
    );

    // Ensure we did truncate with an ellipsis when it doesn't fit the fixed 17-col missing field.
    assert!(
        report.contains("..."),
        "expected ellipsis truncation in uncovered column, but got:\n{report}"
    );
}
