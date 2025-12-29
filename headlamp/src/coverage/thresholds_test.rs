use std::collections::BTreeMap;

use crate::config::CoverageThresholds;
use crate::coverage::model::{CoverageReport, FileCoverage};
use crate::coverage::thresholds::{compute_totals_from_report, threshold_failure_lines};

#[test]
fn threshold_failure_lines_emits_lines_functions_and_branches_when_short() {
    let mut function_hits: BTreeMap<String, u32> = BTreeMap::new();
    function_hits.insert("1:f".to_string(), 0);
    function_hits.insert("2:g".to_string(), 1);

    let mut function_map: BTreeMap<String, (String, u32)> = BTreeMap::new();
    function_map.insert("1:f".to_string(), ("f".to_string(), 1));
    function_map.insert("2:g".to_string(), ("g".to_string(), 2));

    let mut branch_hits: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    branch_hits.insert("10:0".to_string(), vec![0, 1]);

    let mut branch_map: BTreeMap<String, u32> = BTreeMap::new();
    branch_map.insert("10:0".to_string(), 10);

    let report = CoverageReport {
        files: vec![FileCoverage {
            path: "/repo/src/a.rs".to_string(),
            lines_total: 2,
            lines_covered: 1,
            statements_total: None,
            statements_covered: None,
            statement_hits: None,
            uncovered_lines: vec![1],
            line_hits: [(1u32, 0u32), (2u32, 1u32)].into_iter().collect(),
            function_hits,
            function_map,
            branch_hits,
            branch_map,
        }],
    };

    let totals = compute_totals_from_report(&report);
    assert_eq!(totals.lines.total, 2);
    assert_eq!(totals.lines.covered, 1);
    assert_eq!(totals.functions.total, 2);
    assert_eq!(totals.functions.covered, 1);
    assert_eq!(totals.branches.total, 2);
    assert_eq!(totals.branches.covered, 1);

    let thresholds = CoverageThresholds {
        lines: Some(100.0),
        functions: Some(100.0),
        branches: Some(100.0),
        statements: None,
    };
    let lines = threshold_failure_lines(&thresholds, totals);
    assert!(lines.iter().any(|l| l.starts_with("Lines:")));
    assert!(lines.iter().any(|l| l.starts_with("Functions:")));
    assert!(lines.iter().any(|l| l.starts_with("Branches:")));
}

#[test]
fn compute_totals_from_report_uses_statement_totals_when_present() {
    let report = CoverageReport {
        files: vec![FileCoverage {
            path: "/repo/src/a.rs".to_string(),
            lines_total: 1,
            lines_covered: 1,
            statements_total: Some(2),
            statements_covered: Some(1),
            statement_hits: None,
            uncovered_lines: vec![],
            line_hits: [(1u32, 1u32)].into_iter().collect(),
            function_hits: BTreeMap::new(),
            function_map: BTreeMap::new(),
            branch_hits: BTreeMap::new(),
            branch_map: BTreeMap::new(),
        }],
    };

    let totals = compute_totals_from_report(&report);
    assert_eq!(totals.lines.total, 1);
    assert_eq!(totals.lines.covered, 1);

    assert_eq!(totals.statements.total, 2);
    assert_eq!(totals.statements.covered, 1);
}
