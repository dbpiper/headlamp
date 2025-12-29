use headlamp::format::stacks::{first_test_location, is_stack_line};
use regex::Regex;

#[test]
fn stack_detection_recognizes_python_traceback_frames() {
    assert!(is_stack_line(
        "  File \"/repo/tests/test_sum.py\", line 12, in test_sum"
    ));
    assert!(is_stack_line("Traceback (most recent call last):"));
}

#[test]
fn stack_detection_recognizes_rust_backtrace_frames() {
    assert!(is_stack_line("stack backtrace:"));
    assert!(is_stack_line("   0: rust_begin_unwind"));
    assert!(is_stack_line("             at /repo/src/lib.rs:10:3"));
}

#[test]
fn first_test_location_finds_python_file_line() {
    let project_hint = Regex::new(r"/repo/").unwrap();
    let lines = vec![
        "Traceback (most recent call last):".to_string(),
        "  File \"/repo/tests/test_sum.py\", line 12, in test_sum".to_string(),
        "    assert 1 == 2".to_string(),
    ];
    assert_eq!(
        first_test_location(&lines, &project_hint),
        Some("/repo/tests/test_sum.py:12".to_string())
    );
}
