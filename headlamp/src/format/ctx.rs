use std::path::Path;

use path_slash::PathExt;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct Ctx {
    pub cwd: String,
    pub width: usize,
    pub show_stacks: bool,
    pub show_logs: bool,
    pub project_hint: Regex,
    pub editor_cmd: Option<String>,
}

pub fn make_ctx(
    cwd: &Path,
    width: Option<usize>,
    show_stacks: bool,
    show_logs: bool,
    editor_cmd: Option<String>,
) -> Ctx {
    let cwd_s = dunce::canonicalize(cwd)
        .unwrap_or_else(|_| cwd.to_path_buf())
        .to_slash_lossy()
        .to_string();
    let w = detect_width(width).max(40);
    let escaped = regex::escape(&cwd_s);
    let hint = Regex::new(&format!("({escaped})|(/gigworx-node/)")).unwrap();
    Ctx {
        cwd: cwd_s,
        width: w,
        show_stacks,
        show_logs,
        project_hint: hint,
        editor_cmd,
    }
}

fn detect_width(width: Option<usize>) -> usize {
    width
        .or_else(|| {
            std::env::var("COLUMNS")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
        })
        .or_else(|| crate::format::terminal::detect_terminal_size_cols_rows().map(|(w, _h)| w))
        .unwrap_or(80)
}
