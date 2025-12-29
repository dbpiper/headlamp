use crate::coverage::coveragepy_json::parse_coveragepy_json_statement_totals;

#[test]
fn parse_coveragepy_json_statement_totals_uses_num_statements_and_covered_lines() {
    let input = r#"
{
  "files": {
    "/repo/src/a.py": {
      "summary": {
        "num_statements": 10,
        "covered_lines": 7
      }
    }
  }
}
"#;

    let totals = parse_coveragepy_json_statement_totals(input, std::path::Path::new("/repo"))
        .expect("should parse coverage.py json");
    let (total, covered) = totals.get("/repo/src/a.py").copied().expect("file present");
    assert_eq!(total, 10);
    assert_eq!(covered, 7);
}
