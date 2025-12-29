use std::path::Path;

use headlamp::coverage::lcov::read_repo_lcov_filtered;
use headlamp::coverage::print::{PrintOpts, render_report_text};

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

#[test]
fn renders_compact_and_optional_hotspots_from_repo_lcov() {
    let temp = tempfile::TempDir::new().unwrap();
    let repo_root = temp.path();

    let lcov_text = r#"
SF:/repo/src/a.ts
DA:10,0
DA:11,0
DA:12,1
end_of_record
SF:/repo/src/b.ts
DA:1,1
end_of_record
"#;
    write_file(&repo_root.join("coverage/lcov.info"), lcov_text);

    let report = read_repo_lcov_filtered(
        repo_root,
        &["**/*.ts".to_string()],
        &["**/node_modules/**".to_string()],
    )
    .unwrap();

    let opts = PrintOpts {
        max_files: None,
        max_hotspots: Some(2),
        page_fit: true,
        tty: false,
        editor_cmd: None,
    };

    let without_hotspots = render_report_text(&report, &opts, repo_root, false);
    assert!(without_hotspots.contains("Lines:"));
    assert!(without_hotspots.contains("src/a.ts"));
    assert!(without_hotspots.contains("src/b.ts"));
    assert!(!without_hotspots.contains("src/a.ts:"));

    let with_hotspots = render_report_text(&report, &opts, repo_root, true);
    assert!(with_hotspots.contains("src/a.ts:"));
    assert!(with_hotspots.contains("10"));
    assert!(with_hotspots.contains("11"));
}
