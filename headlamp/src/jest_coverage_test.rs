use std::collections::BTreeMap;

use headlamp_core::coverage::model::{CoverageReport, FileCoverage};
use headlamp_core::jest::build_jest_threshold_report;

#[test]
fn build_jest_threshold_report_prefers_istanbul_statement_totals_over_lcov_line_based_statements() {
    let lcov = CoverageReport {
        files: vec![FileCoverage {
            path: "/repo/src/a.ts".to_string(),
            lines_total: 1,
            lines_covered: 1,
            statements_total: None,
            statements_covered: None,
            statement_hits: None,
            uncovered_lines: vec![],
            line_hits: [(10u32, 1u32)].into_iter().collect(),
            function_hits: BTreeMap::new(),
            function_map: BTreeMap::new(),
            branch_hits: BTreeMap::new(),
            branch_map: BTreeMap::new(),
        }],
    };

    let istanbul = CoverageReport {
        files: vec![FileCoverage {
            path: "/repo/src/a.ts".to_string(),
            lines_total: 1,
            lines_covered: 1,
            statements_total: Some(2),
            statements_covered: Some(1),
            statement_hits: Some([(0u64, 1u32), (1u64, 0u32)].into_iter().collect()),
            uncovered_lines: vec![],
            line_hits: [(10u32, 1u32)].into_iter().collect(),
            function_hits: BTreeMap::new(),
            function_map: BTreeMap::new(),
            branch_hits: BTreeMap::new(),
            branch_map: BTreeMap::new(),
        }],
    };

    let report =
        build_jest_threshold_report(Some(lcov), Some(istanbul)).expect("should build report");
    assert_eq!(report.files.len(), 1);
    let file = &report.files[0];
    assert_eq!(file.statements_total, Some(2));
    assert_eq!(file.statements_covered, Some(1));
}
