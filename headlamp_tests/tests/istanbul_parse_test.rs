use headlamp::coverage::istanbul::parse_istanbul_coverage_text;

#[test]
fn parses_istanbul_line_map_when_present() {
    let json = r#"
{
  "/repo/src/a.ts": {
    "path": "/repo/src/a.ts",
    "l": { "10": 1, "11": 0, "12": 3 }
  }
}
"#;
    let report = parse_istanbul_coverage_text(json).unwrap();
    assert_eq!(report.files.len(), 1);
    let f = &report.files[0];
    assert_eq!(f.lines_total, 3);
    assert_eq!(f.lines_covered, 2);
    assert!(f.uncovered_lines.contains(&11));
}

#[test]
fn parses_istanbul_statement_map_fallback() {
    let json = r#"
{
  "/repo/src/a.ts": {
    "path": "/repo/src/a.ts",
    "statementMap": {
      "0": { "start": { "line": 5, "column": 0 }, "end": { "line": 5, "column": 1 } },
      "1": { "start": { "line": 6, "column": 0 }, "end": { "line": 6, "column": 1 } }
    },
    "s": { "0": 1, "1": 0 }
  }
}
"#;
    let report = parse_istanbul_coverage_text(json).unwrap();
    let f = &report.files[0];
    assert_eq!(f.lines_total, 2);
    assert_eq!(f.lines_covered, 1);
    assert!(f.uncovered_lines.contains(&6));
}
