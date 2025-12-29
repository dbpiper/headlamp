use crate::pytest::infer_test_location_from_pytest_longrepr;

#[test]
fn infers_test_location_from_pytest_longrepr_when_frame_matches_nodeid_file() {
    let nodeid_file = "headlamp_tests/tests/test_sum.py";
    let longrepr = r#"Traceback (most recent call last):
  File "/repo/headlamp_tests/tests/test_sum.py", line 12, in test_sum
    assert 1 == 2
E   AssertionError: assert 1 == 2
"#;
    let loc = infer_test_location_from_pytest_longrepr(nodeid_file, longrepr)
        .expect("location inferred");
    assert_eq!(loc.line, 12);
    assert_eq!(loc.column, 1);
}


