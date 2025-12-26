use headlamp::coverage::lcov::parse_lcov_text;
use headlamp::coverage::print::{
    PrintOpts, filter_report, format_compact, format_hotspots, format_summary,
};

#[test]
fn parses_lcov_da_lines() {
    let text = r#"
SF:/repo/src/a.ts
DA:1,1
DA:2,0
DA:3,4
end_of_record
"#;
    let report = parse_lcov_text(text);
    assert_eq!(report.files.len(), 1);
    let f = &report.files[0];
    assert_eq!(f.lines_total, 3);
    assert_eq!(f.lines_covered, 2);
    assert!(f.uncovered_lines.contains(&2));
}

#[test]
fn formats_summary_and_hotspots() {
    let text = r#"
SF:/repo/src/a.ts
DA:10,0
DA:11,0
DA:12,1
end_of_record
"#;
    let report = parse_lcov_text(text);
    let opts = PrintOpts {
        max_files: None,
        max_hotspots: Some(2),
        page_fit: true,
        tty: false,
        editor_cmd: None,
    };
    let root = std::path::Path::new("/repo");
    let filtered = filter_report(report, root, &["**/*.ts".to_string()], &[]);
    let summary = format_summary(&filtered);
    assert!(summary.contains("Lines:"));
    let compact = format_compact(&filtered, &opts, root);
    assert!(compact.contains("a.ts"));
    let hotspots = format_hotspots(&filtered, &opts, root);
    assert!(hotspots.contains("10"));
    assert!(hotspots.contains("11"));
}
