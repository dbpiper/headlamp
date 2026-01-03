use std::path::Path;

use crate::coverage::lcov::normalize_lcov_path;
use crate::coverage::lcov::parse_lcov_text;
use crate::coverage::model::{CoverageReport, FileCoverage};
use crate::coverage::print::filter_report;

#[test]
fn parse_lcov_text_parses_function_and_branch_data() {
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
    assert_eq!(report.files.len(), 1);
    let file = &report.files[0];

    assert_eq!(file.path, "/repo/src/foo.rs");

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

    // Smoke-check that we still parse line hits.
    assert_eq!(file.line_hits.get(&10).copied(), Some(0));
    assert_eq!(file.line_hits.get(&11).copied(), Some(1));

    // And that totals still reflect line coverage.
    assert_eq!(file.lines_total, 2);
    assert_eq!(file.lines_covered, 1);

    // Ensure paths are usable for later formatting.
    assert!(Path::new(&file.path).is_absolute());
}

#[test]
fn parse_lcov_text_parses_coveragepy_branch_data_with_string_branch_ids() {
    // coverage.py's lcov export can emit BRDA records where the "branch" field is a descriptive
    // string like "jump to line 104" instead of a numeric branch id. We still want to surface
    // branch coverage in the UI for these reports.
    let input = "\
TN:
SF:/repo/src/foo.py
BRDA:15,0,jump to line 16,0
BRDA:15,0,jump to line 17,5
DA:15,1
end_of_record
";

    let report = parse_lcov_text(input);
    assert_eq!(report.files.len(), 1);
    let file = &report.files[0];

    assert_eq!(file.path, "/repo/src/foo.py");
    assert_eq!(file.branch_map.get("15:0").copied(), Some(15));
    assert_eq!(file.branch_hits.get("15:0").cloned(), Some(vec![0, 5]));
}

#[test]
fn parse_lcov_text_dedupes_rust_v0_mangled_function_hash_prefixes() {
    let input = "\
TN:
SF:/repo/src/a.rs
FN:2,_RNvNtCsAAAAAA_foo
FN:2,_RNvNtCsBBBBBB_foo
FNDA:2,_RNvNtCsAAAAAA_foo
FNDA:0,_RNvNtCsBBBBBB_foo
DA:2,1
end_of_record
";

    let report = parse_lcov_text(input);
    assert_eq!(report.files.len(), 1);
    let file = &report.files[0];

    // The two rust-v0 function entries differ only by the `Cs<hash>_` segment; we expect to
    // collapse them into a single logical function.
    assert_eq!(file.function_hits.len(), 1);
    assert_eq!(file.function_hits.get("2:foo").copied(), Some(2));
    assert_eq!(
        file.function_map.get("2:foo").cloned(),
        Some(("foo".to_string(), 2))
    );
}

#[test]
fn normalize_lcov_path_does_not_collapse_external_crate_sources_into_repo_root() {
    let repo_root = Path::new("/repo");
    let external = "/Users/me/.cargo/registry/src/abcd1234/some_crate/src/op.rs";
    let normalized = normalize_lcov_path(external, repo_root);
    assert_eq!(normalized, external);
}

#[test]
fn filter_report_excludes_files_outside_repo_root_by_default() {
    let repo_root = Path::new("/repo");
    let report = CoverageReport {
        files: vec![
            FileCoverage {
                path: "/repo/src/in_repo.rs".to_string(),
                lines_total: 1,
                lines_covered: 1,
                statements_total: None,
                statements_covered: None,
                statement_hits: None,
                uncovered_lines: vec![],
                line_hits: std::collections::BTreeMap::new(),
                function_hits: std::collections::BTreeMap::new(),
                function_map: std::collections::BTreeMap::new(),
                branch_hits: std::collections::BTreeMap::new(),
                branch_map: std::collections::BTreeMap::new(),
            },
            FileCoverage {
                path: "/Users/me/.cargo/registry/src/abcd1234/some_crate/src/op.rs".to_string(),
                lines_total: 1,
                lines_covered: 0,
                statements_total: None,
                statements_covered: None,
                statement_hits: None,
                uncovered_lines: vec![1],
                line_hits: std::collections::BTreeMap::new(),
                function_hits: std::collections::BTreeMap::new(),
                function_map: std::collections::BTreeMap::new(),
                branch_hits: std::collections::BTreeMap::new(),
                branch_map: std::collections::BTreeMap::new(),
            },
        ],
    };

    // Even with include globs present (the default path for most runs), we should never include
    // files outside the repo root like Cargo registry sources.
    let filtered = filter_report(report, repo_root, &["**/*.rs".to_string()], &[]);
    assert_eq!(filtered.files.len(), 1);
    assert_eq!(filtered.files[0].path, "/repo/src/in_repo.rs");
}
