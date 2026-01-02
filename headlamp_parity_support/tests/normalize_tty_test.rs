use std::path::Path;

#[test]
fn normalize_tty_ui_drops_live_progress_lines_and_cursor_controls() {
    let root = Path::new("/");

    let raw = [
        "\u{1b}[2KRUN (+10s) (1/4) cargo test",
        "idle 5s | stderr: Compiling parity_real v0.1.2 (/tmp/wt-",
        "1)\u{1b}[2K",
        "123-1)",
        "os/057093c2092a/wt-187-0)",
        "\u{1b}[1A",
        "\u{1b}[1A",
        "\u{1b}[2K\r",
        "\u{1b}[97mRUN\u{1b}[0m \u{1b}[2m<ROOT>\u{1b}[22m",
        "",
        "tests/sum_fail_test.rs (1)",
        "  × test_sum_fails",
    ]
    .join("\n");

    let (normalized, meta) =
        headlamp_parity_support::normalize::normalize_tty_ui_with_meta(raw, root);

    assert!(!meta.used_fallback, "normalizer should not fall back");
    assert!(
        !normalized.contains("RUN (+"),
        "should drop live progress header lines"
    );
    assert!(
        !normalized.contains("idle "),
        "should drop live progress idle lines"
    );
    assert!(
        !normalized.contains("/057093c2092a/wt-"),
        "should drop wrapped worktree path continuation lines"
    );
    assert!(
        !normalized.contains("1)"),
        "should drop line fragments from live progress redraws"
    );
    assert!(
        !normalized.contains("\u{1b}[1A"),
        "should drop cursor-control sequences"
    );
    assert!(
        normalized.contains("<ROOT>"),
        "should keep the stable runner header"
    );
}

#[test]
fn normalize_tty_ui_runner_parity_keeps_suite_listing_and_footer() {
    let root = Path::new("/repo");

    let raw = [
        "\u{1b}[97mRUN\u{1b}[0m \u{1b}[2m<ROOT>\u{1b}[22m",
        "",
        "\u{1b}[35mtests/a_test.rs\u{1b}[39m \u{1b}[2m(1)\u{1b}[22m",
        "  \u{1b}[32m✓\u{1b}[0m \u{1b}[2mtest_a\u{1b}[22m",
        "",
        "\u{1b}[32m\u{1b}[97m PASS \u{1b}[39m\u{1b}[0m \u{1b}[97mtests/a_test.rs\u{1b}[39m",
        "\u{1b}[35mtests/sum_fail_test.rs\u{1b}[39m \u{1b}[2m(1)\u{1b}[22m",
        "  \u{1b}[31m×\u{1b}[0m \u{1b}[97mtest_sum_fails\u{1b}[39m",
        "",
        "\u{1b}[31m\u{1b}[97m FAIL \u{1b}[39m\u{1b}[0m \u{1b}[97mtests/sum_fail_test.rs\u{1b}[39m",
        "\u{1b}[2m───────────────────────────────────────────────────────────────────────────────────────────────────────\u{1b}[22m \u{1b}[31m\u{1b}[97m Failed Tests 1 \u{1b}[39m\u{1b}[0m",
        "",
        "\u{1b}[1mTest Files\u{1b}[22m \u{1b}[31m1 failed\u{1b}[0m\u{1b}[2m | \u{1b}[22m\u{1b}[32m4 passed\u{1b}[0m \u{1b}[2m(5)\u{1b}[22m",
        "\u{1b}[1mTests\u{1b}[22m     \u{1b}[31m1 failed\u{1b}[0m\u{1b}[2m | \u{1b}[22m\u{1b}[32m4 passed\u{1b}[0m \u{1b}[2m(5)\u{1b}[22m",
        "\u{1b}[1mTime\u{1b}[22m      1.23s",
    ]
    .join("\n");

    let normalized = headlamp_parity_support::normalize::normalize_tty_ui_runner_parity(raw, root);

    assert!(
        normalized.contains("tests/a_test.<EXT>"),
        "runner parity normalization should keep suite listing"
    );
    assert!(
        normalized.contains("PASS"),
        "runner parity normalization should keep per-suite PASS/FAIL summary lines"
    );
    assert!(
        normalized.contains("Failed Tests"),
        "runner parity normalization should keep the stable footer"
    );
    assert!(
        normalized.contains("Test Files"),
        "runner parity normalization should keep vitest footer lines"
    );
}

#[test]
fn normalize_tty_ui_does_not_drop_box_table_border_when_concatenated_onto_time_line() {
    let root = Path::new("/repo");
    let raw = [
        "\u{1b}[97mRUN\u{1b}[0m \u{1b}[2m<ROOT>\u{1b}[22m",
        "",
        "\u{1b}[1mTest Files\u{1b}[22m  (1)",
        "\u{1b}[1mTests\u{1b}[22m      (1)",
        // Simulate a TTY capture artifact where the next line starts immediately after the
        // `Time ...` duration (missing newline).
        "\u{1b}[1mTime\u{1b}[22m      1ms┌──────────────────────────────────────────────────┬────────┬──────────┬──────┬──────┬──────┬───────┬──────────────────┐",
        "│\u{1b}[1mFile\u{1b}[22m                                              │\u{1b}[1mSection\u{1b}[22m │\u{1b}[1mWhere\u{1b}[22m     │\u{1b}[1mLines%\u{1b}[22m│\u{1b}[1mBar\u{1b}[22m   │\u{1b}[1mFuncs%\u{1b}[22m│\u{1b}[1mBranch%\u{1b}[22m│\u{1b}[1mDetail\u{1b}[22m            │",
        "┼──────────────────────────────────────────────────┼────────┼──────────┼──────┼──────┼──────┼───────┼──────────────────┼",
        "│src/a.<EXT>                                          │Summary │—         │100.0%│██████│100.0%│    N/A│                  │",
        "└──────────────────────────────────────────────────┴────────┴──────────┴──────┴──────┴──────┴───────┴──────────────────┘",
    ]
    .join("\n");

    let normalized = headlamp_parity_support::normalize::normalize_tty_ui(raw, root);
    assert!(
        normalized.contains("\n┌────────────────"),
        "expected normalization to preserve the box-table border by splitting it off the time line"
    );
}
