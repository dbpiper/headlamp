use std::fs;
use std::path::Path;
use std::sync::Arc;

use dashmap::DashMap;
use once_cell::sync::Lazy;
use path_slash::PathExt;
use regex::Regex;

use crate::format::colors;
use crate::format::{ansi, stacks};

static SOURCE_CACHE: Lazy<DashMap<String, Arc<Vec<String>>>> = Lazy::new(DashMap::new);

pub fn find_code_frame_start(lines: &[String]) -> Option<usize> {
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*(>?\s*\d+\s*\|)").unwrap());
    lines
        .iter()
        .position(|line| RE.is_match(&stacks::strip_ansi_simple(line)))
}

fn read_source(file: &str) -> Arc<Vec<String>> {
    let normalized = Path::new(file).to_slash_lossy().to_string();
    if let Some(hit) = SOURCE_CACHE.get(&normalized) {
        return Arc::clone(hit.value());
    }
    let read = fs::read_to_string(&normalized)
        .map(|txt| {
            txt.split('\n')
                .map(|line| line.trim_end_matches('\r').to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let read = Arc::new(read);
    SOURCE_CACHE.insert(normalized, Arc::clone(&read));
    read
}

fn render_inline_code_frame(lines: &[String], start: usize) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    static CARET_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*\^+\s*$").unwrap());
    static PTR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*>(\s*\d+)\s*\|\s?(.*)$").unwrap());
    static NOR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*(\d+)\s*\|\s?(.*)$").unwrap());
    for i in start..lines.len() {
        let raw = lines
            .get(i)
            .map(|s| stacks::strip_ansi_simple(s))
            .unwrap_or_default();
        if raw.trim().is_empty() {
            break;
        }
        if CARET_RE.is_match(&raw) {
            out.push(format!("    {}", ansi::red(raw.trim_end())));
        } else if let Some(caps) = PTR_RE.captures(&raw) {
            let num = ansi::dim(caps.get(1).map(|m| m.as_str()).unwrap_or("").trim());
            let code = ansi::yellow(caps.get(2).map(|m| m.as_str()).unwrap_or(""));
            out.push(format!(
                "    {} {} {} {}",
                colors::failure(">"),
                num,
                ansi::dim("|"),
                code
            ));
        } else if let Some(caps) = NOR_RE.captures(&raw) {
            let num = ansi::dim(caps.get(1).map(|m| m.as_str()).unwrap_or(""));
            let code = ansi::dim(caps.get(2).map(|m| m.as_str()).unwrap_or(""));
            out.push(format!("      {} {} {}", num, ansi::dim("|"), code));
        } else {
            out.push(format!("    {raw}"));
        }
    }
    out
}

fn render_source_code_frame(
    file: &str,
    line: i64,
    _column: Option<i64>,
    context: i64,
) -> Vec<String> {
    let lines = read_source(file);
    if lines.is_empty() || line <= 0 {
        return vec![];
    }
    let idx = (line as usize).clamp(1, lines.len());
    let start = (idx as i64 - context).max(1) as usize;
    let end = (idx as i64 + context).min(lines.len() as i64) as usize;
    let mut out: Vec<String> = vec![];
    for current in start..=end {
        let num = ansi::dim(&format!("{current}"));
        let raw_line = lines
            .get(current.saturating_sub(1))
            .map(|s| s.as_str())
            .unwrap_or("");
        let code = if current == idx {
            ansi::yellow(raw_line)
        } else {
            ansi::dim(raw_line)
        };
        if current == idx {
            out.push(format!(
                "    {} {} {} {}",
                colors::failure(">"),
                num,
                ansi::dim("|"),
                code
            ));
        } else {
            out.push(format!("      {} {} {}", num, ansi::dim("|"), code));
        }
    }
    out.push(format!("    {}", colors::failure("^")));
    out
}

#[derive(Debug, Clone)]
pub struct Loc {
    pub file: String,
    pub line: i64,
    pub column: Option<i64>,
}

pub fn build_code_frame_section(
    message_lines: &[String],
    show_stacks: bool,
    synth_loc: Option<&Loc>,
) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    if let Some(start) = find_code_frame_start(message_lines) {
        out.extend(render_inline_code_frame(message_lines, start));
        out.push(String::new());
        return out;
    }
    if show_stacks {
        if let Some(loc) = synth_loc {
            if Path::new(&loc.file).exists() {
                out.extend(render_source_code_frame(&loc.file, loc.line, loc.column, 3));
                out.push(String::new());
            }
        }
    }
    out
}
