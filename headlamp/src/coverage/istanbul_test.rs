use crate::coverage::istanbul::{merge_istanbul_reports, parse_istanbul_coverage_text};

#[test]
fn parse_istanbul_coverage_text_preserves_statement_totals_even_when_multiple_statements_share_a_line()
 {
    let input = r#"
{
  "/repo/src/a.ts": {
    "path": "/repo/src/a.ts",
    "s": { "0": 1, "1": 0 },
    "statementMap": {
      "0": { "start": { "line": 10 } },
      "1": { "start": { "line": 10 } }
    }
  }
}
"#;

    let report = parse_istanbul_coverage_text(input).expect("should parse istanbul json");
    assert_eq!(report.files.len(), 1);
    let file = &report.files[0];

    assert_eq!(file.lines_total, 1);
    assert_eq!(file.lines_covered, 1);

    assert_eq!(file.statements_total, Some(2));
    assert_eq!(file.statements_covered, Some(1));
}

#[test]
fn merge_istanbul_reports_merges_statement_hits_by_id_instead_of_double_counting_totals() {
    let input_a = r#"
{
  "/repo/src/a.ts": {
    "path": "/repo/src/a.ts",
    "s": { "0": 1, "1": 0 },
    "statementMap": {
      "0": { "start": { "line": 10 } },
      "1": { "start": { "line": 10 } }
    }
  }
}
"#;
    let input_b = r#"
{
  "/repo/src/a.ts": {
    "path": "/repo/src/a.ts",
    "s": { "0": 0, "1": 1 },
    "statementMap": {
      "0": { "start": { "line": 10 } },
      "1": { "start": { "line": 10 } }
    }
  }
}
"#;

    let report_a = parse_istanbul_coverage_text(input_a).expect("should parse istanbul json");
    let report_b = parse_istanbul_coverage_text(input_b).expect("should parse istanbul json");

    let merged = merge_istanbul_reports(&[report_a, report_b], std::path::Path::new("/repo"));
    assert_eq!(merged.files.len(), 1);
    let file = &merged.files[0];

    assert_eq!(file.statements_total, Some(2));
    assert_eq!(file.statements_covered, Some(2));
}
