use crate::coverage::llvm_cov_json::parse_llvm_cov_json_statement_totals;

#[test]
fn parse_llvm_cov_json_statement_totals_counts_region_entries_as_statements() {
    let input = r#"
{
  "data": [
    {
      "files": [
        {
          "filename": "/repo/src/a.rs",
          "segments": [
            [1, 1, 0, true, true, false],
            [1, 2, 0, false, false, false],
            [2, 1, 3, true, true, false],
            [2, 2, 0, false, false, false],
            [3, 1, 0, true, true, true]
          ]
        }
      ]
    }
  ]
}
"#;

    let totals = parse_llvm_cov_json_statement_totals(input, std::path::Path::new("/repo"))
        .expect("should parse llvm-cov json");
    let (total, covered) = totals.get("/repo/src/a.rs").copied().expect("file present");
    assert_eq!(total, 2);
    assert_eq!(covered, 1);
}

#[test]
fn parse_llvm_cov_json_statement_totals_unions_region_entries_across_data_sections() {
    let input = r#"
{
  "data": [
    {
      "files": [
        {
          "filename": "/repo/src/a.rs",
          "segments": [
            [1, 1, 0, true, true, false],
            [1, 2, 0, false, false, false]
          ]
        }
      ]
    },
    {
      "files": [
        {
          "filename": "/repo/src/a.rs",
          "segments": [
            [2, 1, 4, true, true, false],
            [2, 2, 0, false, false, false]
          ]
        }
      ]
    }
  ]
}
"#;

    let totals = parse_llvm_cov_json_statement_totals(input, std::path::Path::new("/repo"))
        .expect("should parse llvm-cov json");
    let (total, covered) = totals.get("/repo/src/a.rs").copied().expect("file present");
    assert_eq!(total, 2);
    assert_eq!(covered, 1);
}

#[test]
fn parse_llvm_cov_json_statement_hits_creates_stable_ids_from_line_and_col() {
    let input = r#"
{
  "data": [
    {
      "files": [
        {
          "filename": "/repo/src/a.rs",
          "segments": [
            [10, 1, 0, true, true, false],
            [10, 2, 3, true, true, false]
          ]
        }
      ]
    }
  ]
}
"#;
    let hits = crate::coverage::llvm_cov_json::parse_llvm_cov_json_statement_hits(
        input,
        std::path::Path::new("/repo"),
    )
    .expect("hits");
    let by_id = hits.get("/repo/src/a.rs").expect("file present");
    assert_eq!(by_id.get("10:1").copied(), Some(0));
    assert_eq!(by_id.get("10:2").copied(), Some(3));
}
