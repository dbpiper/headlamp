use std::path::Path;

use path_slash::PathExt;
use regex::Regex;

pub(super) fn normalize_paths(mut text: String, root: &Path) -> String {
    let root_s = root.to_slash_lossy().to_string();
    text = text.replace('\\', "/");
    text = text.replace(&root_s, "<ROOT>");
    text = regex_replace(&text, r"jest-bridge-[0-9]+\.json", "jest-bridge-<PID>.json");
    text = regex_replace(&text, r"\+[0-9]+s\]", "+<N>s]");
    text = regex_replace(&text, r"\b[0-9]{1,5}ms\b", "<N>ms");
    text = regex_replace(
        &text,
        r"((?:\x1b\[[0-9;]*m)*Time(?:\x1b\[[0-9;]*m)*)\s+<N>ms\b\s*",
        "$1 <N>ms",
    );
    text = regex_replace(
        &text,
        r"(?m)((?:\x1b\[[0-9;]*m)*Time(?:\x1b\[[0-9;]*m)*)\s*$",
        "$1 <N>ms",
    );
    text = regex_replace(
        &text,
        r"((?:\x1b\[[0-9;]*m)*Time(?:\x1b\[[0-9;]*m)*)\s+((?:\x1b\[[0-9;]*m)*)\(in thread",
        "$1 $2(in thread",
    );
    text = regex_replace(&text, r":\d+:\d+\b", ":<LINE>:<COL>");
    text = regex_replace(&text, r":\d+\b", ":<LINE>");
    normalize_fixture_exts(&text)
}

fn normalize_fixture_exts(text: &str) -> String {
    let test_ext = Regex::new(r"\.test\.(js|rs|py)").unwrap();
    let test_suffix_ext = Regex::new(r#"(?m)(tests/[^ \t\r\n:'"]+_test)\.(js|rs|py)"#).unwrap();
    let src_ext = Regex::new(r#"(?m)(src/[^ \t\r\n:'"]+)\.(js|rs|py)"#).unwrap();
    let tests_ext = Regex::new(r#"(?m)(tests/[^ \t\r\n:'"]+)\.(js|rs|py)"#).unwrap();
    let istanbul_table_basename_ext =
        Regex::new(r#"(?m)^((?:\x1b\[[0-9;]*m)*\s*)([A-Za-z0-9_.-]+)\.(js|rs|py)"#).unwrap();

    let with_tests = test_ext.replace_all(text, ".test.<EXT>").to_string();
    let with_test_suffix = test_suffix_ext
        .replace_all(&with_tests, "$1.<EXT>")
        .to_string();
    let with_src = src_ext
        .replace_all(&with_test_suffix, "$1.<EXT>")
        .to_string();
    let with_tests_any = tests_ext.replace_all(&with_src, "$1.<EXT>").to_string();
    let with_istanbul_basenames = istanbul_table_basename_ext
        .replace_all(&with_tests_any, "$1$2.<EXT>")
        .to_string();
    normalize_coverage_numbers(&with_istanbul_basenames)
}

fn normalize_coverage_numbers(text: &str) -> String {
    let summary = Regex::new(r"Lines: [0-9]+(\.[0-9]+)?% \([0-9]+/[0-9]+\)").unwrap();
    let pct = Regex::new(r"\b[0-9]+(\.[0-9]+)?%").unwrap();
    let count = Regex::new(r"\b[0-9]{1,6}\b").unwrap();

    let with_summary = summary
        .replace_all(text, "Lines: <PCT>% (<N>/<N>)")
        .to_string();
    let with_pct = pct.replace_all(&with_summary, "<PCT>%").to_string();
    count.replace_all(&with_pct, "<N>").to_string()
}

pub(super) fn regex_replace(text: &str, pat: &str, repl: &str) -> String {
    let re = regex::Regex::new(pat).unwrap();
    re.replace_all(text, repl).to_string()
}
