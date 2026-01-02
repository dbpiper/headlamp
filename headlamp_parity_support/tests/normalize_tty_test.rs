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
