use std::path::Path;

use headlamp_parity_support::normalize_tty_ui;

#[test]
fn normalize_tty_ui_removes_optional_blank_line_between_tests_and_time() {
    let input = [
        "\u{1b}[2m────────────────────────────────────────────────────────────────────\u{1b}[22m \u{1b}[35m\u{1b}[97m Failed Tests 1 \u{1b}[39m\u{1b}[0m",
        "",
        "\u{1b}[1mTest Files\u{1b}[22m \u{1b}[35m1 failed\u{1b}[0m\u{1b}[2m | \u{1b}[22m\u{1b}[94m4 passed\u{1b}[0m \u{1b}[2m(5)\u{1b}[22m",
        "\u{1b}[1mTests\u{1b}[22m     \u{1b}[35m1 failed\u{1b}[0m\u{1b}[2m | \u{1b}[22m\u{1b}[94m4 passed\u{1b}[0m \u{1b}[2m(5)\u{1b}[22m",
        "",
        "\u{1b}[1mTime\u{1b}[22m      0.12s",
    ]
    .join("\n");

    let normalized = normalize_tty_ui(input, Path::new("/repo"));

    assert!(
        !normalized.contains("\n\n\u{1b}[1mTime"),
        "expected the blank line immediately before the Time line to be removed; got:\n{normalized}"
    );
}

#[test]
fn normalize_tty_ui_keeps_compact_footer_when_blank_line_is_absent() {
    let input = [
        "\u{1b}[1mTest Files\u{1b}[22m \u{1b}[35m1 failed\u{1b}[0m\u{1b}[2m | \u{1b}[22m\u{1b}[94m4 passed\u{1b}[0m \u{1b}[2m(5)\u{1b}[22m",
        "\u{1b}[1mTests\u{1b}[22m     \u{1b}[35m1 failed\u{1b}[0m\u{1b}[2m | \u{1b}[22m\u{1b}[94m4 passed\u{1b}[0m \u{1b}[2m(5)\u{1b}[22m",
        "\u{1b}[1mTime\u{1b}[22m      0.12s",
    ]
    .join("\n");

    let normalized = normalize_tty_ui(input, Path::new("/repo"));

    assert!(
        normalized.contains("\u{1b}[1mTests\u{1b}[22m")
            && normalized.contains("\n\u{1b}[1mTime\u{1b}[22m"),
        "expected footer to remain contiguous; got:\n{normalized}"
    );
}
