use std::path::Path;

use crate::coverage::lcov::parse_lcov_text;

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
