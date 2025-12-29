use std::path::Path;

use crate::coverage::istanbul_pretty::analysis::file_summary;
use crate::coverage::istanbul_pretty::api::lcov_report_to_full_file_coverage;
use crate::coverage::lcov::parse_lcov_text;
use crate::coverage::model::{CoverageReport, FileCoverage};
use std::collections::BTreeMap;

#[test]
fn lcov_to_full_file_coverage_uses_real_function_and_branch_maps() {
    let input = "\
TN:
SF:/repo/src/foo.rs
FN:10,foo
FNDA:0,foo
FN:20,bar
FNDA:3,bar
BRDA:15,0,0,-
BRDA:15,0,1,0
BRDA:15,0,2,5
DA:10,0
DA:11,1
end_of_record
";
    let report = parse_lcov_text(input);
    let files = lcov_report_to_full_file_coverage(Path::new("/repo"), &report);
    assert_eq!(files.len(), 1);
    let file = &files[0];

    assert_eq!(file.rel_path, "src/foo.rs");

    assert_eq!(file.function_hits.get("10:foo").copied(), Some(0));
    assert_eq!(file.function_hits.get("20:bar").copied(), Some(3));
    assert_eq!(
        file.function_map.get("10:foo").cloned(),
        Some(("foo".to_string(), 10))
    );
    assert_eq!(
        file.function_map.get("20:bar").cloned(),
        Some(("bar".to_string(), 20))
    );

    assert_eq!(file.branch_map.get("15:0").copied(), Some(15));
    assert_eq!(file.branch_hits.get("15:0").cloned(), Some(vec![0, 0, 5]));

    let summary = file_summary(file);
    assert_eq!(summary.functions.total, 2);
    assert_eq!(summary.functions.covered, 1);
    assert_eq!(summary.branches.total, 3);
    assert_eq!(summary.branches.covered, 1);
    assert_eq!(summary.lines.total, 2);
    assert_eq!(summary.lines.covered, 1);
}

#[test]
fn lcov_to_full_file_coverage_uses_statement_hits_when_present() {
    let report = CoverageReport {
        files: vec![FileCoverage {
            path: "/repo/src/foo.rs".to_string(),
            lines_total: 1,
            lines_covered: 1,
            statements_total: Some(2),
            statements_covered: Some(1),
            statement_hits: Some(BTreeMap::from([
                ("10:1".to_string(), 0),
                ("10:2".to_string(), 1),
            ])),
            uncovered_lines: vec![],
            line_hits: BTreeMap::from([(10, 1)]),
            function_hits: BTreeMap::new(),
            function_map: BTreeMap::new(),
            branch_hits: BTreeMap::new(),
            branch_map: BTreeMap::new(),
        }],
    };
    let files = lcov_report_to_full_file_coverage(Path::new("/repo"), &report);
    assert_eq!(files.len(), 1);
    let file = &files[0];

    let summary = file_summary(file);
    assert_eq!(summary.statements.total, 2);
    assert_eq!(summary.statements.covered, 1);
    assert_eq!(summary.lines.total, 1);
    assert_eq!(summary.lines.covered, 1);
}
