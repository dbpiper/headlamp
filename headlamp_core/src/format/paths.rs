use std::env;
use std::path::{Path, PathBuf};

use path_slash::PathExt;

fn prefer_vscode(hint: Option<&str>) -> bool {
    let hint = hint
        .map(|s| s.to_string())
        .or_else(|| env::var("COVERAGE_EDITOR").ok())
        .unwrap_or_default();
    let hint_is_vscode = matches!(hint.to_lowercase().as_str(), "code" | "vscode");
    hint_is_vscode
        || env::var("TERM_PROGRAM")
            .ok()
            .is_some_and(|v| v.to_lowercase() == "vscode")
        || env::var("VSCODE_IPC_HOOK").is_ok()
}

pub fn preferred_editor_href(abs_path: &str, line: Option<i64>, hint: Option<&str>) -> String {
    let absolute: PathBuf = Path::new(abs_path).to_path_buf();
    let absolute = dunce::canonicalize(&absolute).unwrap_or(absolute);
    let absolute = absolute.to_slash_lossy();
    if prefer_vscode(hint) {
        match line {
            Some(l) => format!("vscode://file/{absolute}:{l}"),
            None => format!("vscode://file/{absolute}"),
        }
    } else {
        match line {
            Some(l) => format!("file://{absolute}#L{l}"),
            None => format!("file://{absolute}"),
        }
    }
}

pub fn linkify_padded(
    abs_path: &str,
    line: Option<i64>,
    hint: Option<&str>,
    padded: &str,
) -> String {
    let trimmed = padded.trim_end_matches(|c: char| c.is_whitespace());
    let pad = padded.len().saturating_sub(trimmed.len());
    let url = preferred_editor_href(abs_path, line, hint);
    format!(
        "\u{1b}]8;;{url}\u{7}{trimmed}\u{1b}]8;;\u{7}{}",
        " ".repeat(pad)
    )
}
