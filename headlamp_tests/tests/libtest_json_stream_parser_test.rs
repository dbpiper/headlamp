use std::path::Path;
use std::time::Duration;

use headlamp::format::libtest_json::LibtestJsonStreamParser;

#[test]
fn libtest_json_stream_parser_records_exec_time_as_duration() {
    let repo_root = Path::new("/repo");
    let mut parser = LibtestJsonStreamParser::new(repo_root, "tests/nohang.rs");

    let lines = [
        r#"{"type":"suite","event":"started","test_count":1}"#,
        r#"{"type":"test","event":"started","name":"nohang"}"#,
        r#"{"type":"test","event":"ok","name":"nohang","exec_time":3.21}"#,
        r#"{"type":"suite","event":"ok","passed":1,"failed":0,"ignored":0,"measured":0,"filtered_out":0,"exec_time":3.21}"#,
    ];

    let updates = lines
        .iter()
        .filter_map(|line| parser.push_line(line))
        .collect::<Vec<_>>();

    let ok_update = updates
        .iter()
        .find(|u| u.test_name == "nohang")
        .expect("update");
    assert_eq!(ok_update.status, "passed");
    assert!(
        ok_update
            .duration
            .is_some_and(|d| d >= Duration::from_secs(3)),
        "expected ~seconds duration, got: {:?}",
        ok_update.duration
    );

    let model = parser.finalize().expect("model");
    let suite = model.test_results.first().expect("suite");
    let test_case = suite
        .test_results
        .iter()
        .find(|t| t.full_name == "nohang")
        .expect("test case");
    assert_eq!(test_case.status, "passed");
    assert!(
        test_case.duration >= 3_000,
        "expected duration_ms to be seconds-ish, got {}ms",
        test_case.duration
    );
}

#[test]
fn libtest_json_stream_parser_captures_failure_stdout() {
    let repo_root = Path::new("/repo");
    let mut parser = LibtestJsonStreamParser::new(repo_root, "tests/basic.rs");

    let lines = [
        r#"{"type":"suite","event":"started","test_count":1}"#,
        r#"{"type":"test","event":"failed","name":"boom","exec_time":0.01,"stdout":"thread 'boom' panicked at tests/basic.rs:7:2:\nboom\n"}"#,
        r#"{"type":"suite","event":"failed","passed":0,"failed":1,"ignored":0,"measured":0,"filtered_out":0,"exec_time":0.01}"#,
    ];

    lines.iter().for_each(|line| {
        let _ = parser.push_line(line);
    });

    let model = parser.finalize().expect("model");
    let suite = model.test_results.first().expect("suite");
    let failed = suite
        .test_results
        .iter()
        .find(|t| t.full_name == "boom")
        .expect("failed test case");
    assert_eq!(failed.status, "failed");
    let msg = failed
        .failure_messages
        .first()
        .map(String::as_str)
        .unwrap_or("");
    assert!(
        msg.contains("panicked at"),
        "expected panic text, got: {msg}"
    );
}
