use std::path::Path;

use headlamp::format::cargo_test::CargoTestStreamParser;

#[test]
fn cargo_test_parser_records_duration_from_report_time_suffix() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    std::fs::create_dir_all(repo_root.join("tests")).expect("create tests/");
    std::fs::write(repo_root.join("tests").join("nohang.rs"), "").expect("write suite file");

    let mut parser = CargoTestStreamParser::new(repo_root);
    let _ = parser.push_line("Running tests/nohang.rs");
    let _ = parser.push_line("running 1 test");
    let _ = parser.push_line("test nohang ... ok (3.21s)");
    let _ = parser.push_line(
        "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.21s",
    );

    let model = parser.finalize().expect("model");
    let suite = model.test_results.first().expect("suite");
    let test_case = suite.test_results.first().expect("test case");

    assert_eq!(test_case.full_name, "nohang");
    assert_eq!(test_case.status, "passed");
    assert!(
        test_case.duration >= 3_000,
        "expected duration_ms to be seconds-ish, got {}ms",
        test_case.duration
    );
}

#[test]
fn cargo_test_parser_does_not_invent_duration_when_report_time_missing() {
    let repo_root = Path::new("/repo");
    let mut parser = CargoTestStreamParser::new(repo_root);
    let _ = parser.push_line("Running tests/basic.rs");
    let _ = parser.push_line("test fast ... ok");
    let _ = parser.push_line("test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s");

    let model = parser.finalize().expect("model");
    let suite = model.test_results.first().expect("suite");
    let test_case = suite.test_results.first().expect("test case");
    assert_eq!(test_case.full_name, "fast");
    assert_eq!(test_case.duration, 0);
}
