use std::path::Path;

use crate::rust_runner::libtest_parser::parse_libtest_output_for_suite;

#[test]
fn parses_basic_suite_with_pass_and_fail() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    std::fs::create_dir_all(repo_root.join("tests")).expect("create tests/");
    std::fs::write(repo_root.join("tests").join("basic.rs"), "").expect("write suite file");

    let output = r#"
running 2 tests
test passes ... ok
test fails ... FAILED

failures:

---- fails stdout ----
thread 'fails' panicked at tests/basic.rs:3:1:
boom

failures:
    fails

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
"#;

    let model = parse_libtest_output_for_suite(repo_root, "tests/basic.rs", output)
        .expect("expected parsed model");
    assert_eq!(model.test_results.len(), 1);
    let suite = &model.test_results[0];
    assert!(suite.test_file_path.contains("tests/basic.rs"));
    assert_eq!(suite.test_results.len(), 2);
    assert!(
        suite
            .test_results
            .iter()
            .any(|t| t.full_name == "passes" && t.status == "passed")
    );
    assert!(
        suite
            .test_results
            .iter()
            .any(|t| t.full_name == "fails" && t.status == "failed")
    );
}

#[test]
fn parses_assert_message_body_lines_from_panic_block() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    std::fs::create_dir_all(repo_root.join("tests")).expect("create tests/");
    std::fs::write(repo_root.join("tests").join("guard.rs"), "").expect("write suite file");

    let output = r#"
running 1 test
test rust_files_do_not_exceed_max_physical_lines ... FAILED

failures:

---- rust_files_do_not_exceed_max_physical_lines stdout ----
thread 'rust_files_do_not_exceed_max_physical_lines' panicked at tests/guard.rs:13:5:
found 2 files over limit (500):
600 lines -> /repo/a.rs
700 lines -> /repo/b.rs

failures:
    rust_files_do_not_exceed_max_physical_lines

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
"#;

    let model = parse_libtest_output_for_suite(repo_root, "tests/guard.rs", output)
        .expect("expected parsed model");
    let suite = &model.test_results[0];
    let failed = suite
        .test_results
        .iter()
        .find(|t| t.full_name == "rust_files_do_not_exceed_max_physical_lines")
        .expect("expected failed test case");
    let msg = failed
        .failure_messages
        .first()
        .map(|s| s.as_str())
        .unwrap_or("");
    assert!(
        msg.contains("found 2 files over limit"),
        "expected panic message body to be preserved, got:\n{msg}"
    );
    assert!(msg.contains("600 lines ->"));
    assert!(msg.contains("700 lines ->"));
}

#[test]
fn parses_ignored_test() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    std::fs::create_dir_all(repo_root.join("tests")).expect("create tests/");
    std::fs::write(repo_root.join("tests").join("ignored.rs"), "").expect("write suite file");

    let output = r#"
running 1 test
test ignored_one ... ignored

test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
"#;

    let model = parse_libtest_output_for_suite(repo_root, "tests/ignored.rs", output)
        .expect("expected parsed model");
    let suite = &model.test_results[0];
    assert_eq!(suite.test_results.len(), 1);
    assert_eq!(suite.test_results[0].full_name, "ignored_one");
    assert_eq!(suite.test_results[0].status, "pending");
}

#[test]
fn parser_is_linearish_for_large_output() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    std::fs::create_dir_all(repo_root.join("tests")).expect("create tests/");
    std::fs::write(repo_root.join("tests").join("perf.rs"), "").expect("write suite file");

    let mut output_buffer = String::with_capacity(2_000_000);
    output_buffer.push_str("running 50000 tests\n");
    for index in 0..50_000u32 {
        output_buffer.push_str("test t");
        output_buffer.push_str(index.to_string().as_str());
        output_buffer.push_str(" ... ok\n");
    }
    output_buffer.push_str("test result: ok. 50000 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n");

    let started_at = std::time::Instant::now();
    let model = parse_libtest_output_for_suite(repo_root, "tests/perf.rs", &output_buffer)
        .expect("expected parsed model");
    assert_eq!(model.test_results.len(), 1);
    assert!(
        started_at.elapsed() < std::time::Duration::from_millis(2500),
        "parse took too long: {:?}",
        started_at.elapsed()
    );
}

fn _typecheck_path(_path: &Path) {}
