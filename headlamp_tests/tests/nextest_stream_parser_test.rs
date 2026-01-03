use std::path::Path;

use headlamp::format::nextest::NextestStreamParser;

#[test]
fn nextest_stream_parser_emits_updates_and_finalizes() {
    let repo_root = Path::new("/repo");
    let mut parser = NextestStreamParser::new(repo_root);

    let lines = [
        r#"{"type":"suite","event":"started","test_count":2,"nextest":{"crate":"parity_sum","test_binary":"sum_test","kind":"test"}}"#,
        r#"{"type":"test","event":"started","name":"parity_sum::sum_test$sum_passes"}"#,
        r#"{"type":"test","event":"started","name":"parity_sum::sum_test$sum_fails"}"#,
        r#"{"type":"test","event":"failed","name":"parity_sum::sum_test$sum_fails","exec_time":0.01,"stdout":"fail"}"#,
        r#"{"type":"suite","event":"failed","passed":1,"failed":1,"ignored":0,"measured":0,"filtered_out":0,"exec_time":0.02,"nextest":{"crate":"parity_sum","test_binary":"sum_test","kind":"test"}}"#,
        r#"{"type":"test","event":"ok","name":"parity_sum::sum_test$sum_passes","exec_time":0.01}"#,
    ];

    let updates = lines
        .iter()
        .filter_map(|line| parser.push_line(line))
        .collect::<Vec<_>>();

    assert!(updates.iter().any(|u| u.status == "failed"));
    assert!(updates.iter().any(|u| u.status == "passed"));
    assert!(
        updates
            .iter()
            .filter(|u| u.test_name == "sum_passes")
            .any(|u| u
                .duration
                .is_some_and(|duration| duration > std::time::Duration::ZERO)),
        "expected a non-zero duration for at least one test update"
    );

    let model = parser.finalize().expect("model");
    assert_eq!(model.test_results.len(), 1);
    assert_eq!(model.test_results[0].test_results.len(), 2);
}

#[test]
fn nextest_stream_parser_preserves_submillisecond_exec_time_in_update_duration() {
    let repo_root = Path::new("/repo");
    let mut parser = NextestStreamParser::new(repo_root);
    let lines = [
        r#"{"type":"suite","event":"started","test_count":1,"nextest":{"crate":"parity_sum","test_binary":"sum_test","kind":"test"}}"#,
        r#"{"type":"test","event":"ok","name":"parity_sum::sum_test$sum_passes","exec_time":0.0004}"#,
        r#"{"type":"suite","event":"ok","passed":1,"failed":0,"ignored":0,"measured":0,"filtered_out":0,"exec_time":0.0004,"nextest":{"crate":"parity_sum","test_binary":"sum_test","kind":"test"}}"#,
    ];

    let updates = lines
        .iter()
        .filter_map(|line| parser.push_line(line))
        .collect::<Vec<_>>();

    let update = updates
        .iter()
        .find(|u| u.test_name == "sum_passes")
        .expect("sum_passes update");
    let duration = update.duration.expect("duration present");
    assert!(duration > std::time::Duration::ZERO);
    assert!(duration < std::time::Duration::from_millis(1));
}

#[test]
fn nextest_parser_sets_location_when_panic_location_matches_suite_file() {
    let repo_root = std::env::temp_dir()
        .join("headlamp-tests")
        .join("nextest-parser-location");
    let _ = std::fs::remove_dir_all(&repo_root);
    std::fs::create_dir_all(repo_root.join("tests")).unwrap();
    std::fs::write(
        repo_root.join("tests").join("sum_test.rs"),
        (1..=20)
            .map(|i| format!("const LINE_{i} = {i};"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();

    let mut parser = NextestStreamParser::new(repo_root.as_path());
    let lines = [
        r#"{"type":"suite","event":"started","test_count":1,"nextest":{"crate":"parity_sum","test_binary":"sum_test","kind":"test"}}"#,
        r#"{"type":"test","event":"failed","name":"parity_sum::sum_test$sum_fails","exec_time":0.01,"stdout":"thread 'sum_fails' panicked at tests/sum_test.rs:7:2:\nassertion `left == right` failed\n  left: 1\n right: 2\n"}"#,
        r#"{"type":"suite","event":"failed","passed":0,"failed":1,"ignored":0,"measured":0,"filtered_out":0,"exec_time":0.02,"nextest":{"crate":"parity_sum","test_binary":"sum_test","kind":"test"}}"#,
    ];
    lines.iter().for_each(|line| {
        let _ = parser.push_line(line);
    });

    let model = parser.finalize().expect("model");
    let suite = model.test_results.first().expect("suite");
    let failed = suite
        .test_results
        .iter()
        .find(|t| t.status == "failed")
        .expect("failed test present");
    let loc = failed.location.as_ref().expect("location inferred");
    assert_eq!(loc.line, 7);
    assert_eq!(loc.column, 2);
}
