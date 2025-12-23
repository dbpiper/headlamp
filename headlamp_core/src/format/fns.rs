use once_cell::sync::Lazy;
use regex::Regex;

use path_slash::PathExt;

use crate::format::{ansi, colors, stacks};

static STACK_LOC_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\(?([^\s()]+):(\d+):(\d+)\)?$").unwrap());

pub fn draw_rule(width: usize, label: Option<&str>) -> String {
    let w = width.max(40);
    match label {
        None => ansi::dim(&"─".repeat(w)),
        Some(l) => {
            let plain = stacks::strip_ansi_simple(l);
            let pad = (w as isize - plain.len() as isize - 1).max(1) as usize;
            format!("{} {}", ansi::dim(&"─".repeat(pad)), l)
        }
    }
}

pub fn draw_fail_line(width: usize) -> String {
    let w = width.max(40);
    colors::failure(&"─".repeat(w))
}

pub fn render_run_line(cwd: &str) -> String {
    format!(
        "{} {}",
        colors::bg_run(&ansi::white(" RUN ")),
        ansi::dim(cwd)
    )
}

pub fn build_file_badge_line(rel: &str, failed_count: usize) -> String {
    if failed_count > 0 {
        format!(
            "{} {}",
            colors::bg_failure(&ansi::white(" FAIL ")),
            ansi::white(rel)
        )
    } else {
        format!(
            "{} {}",
            colors::bg_success(&ansi::white(" PASS ")),
            ansi::white(rel)
        )
    }
}

pub fn build_per_file_overview(rel: &str, assertions: &[(String, String)]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    out.push(format!(
        "{} {}",
        ansi::magenta(rel),
        ansi::dim(&format!("({})", assertions.len()))
    ));
    for (full_name, status) in assertions {
        let line = match status.as_str() {
            "passed" => format!("  {} {}", colors::success("✓"), ansi::dim(full_name)),
            "todo" => format!(
                "  {} {} {}",
                colors::todo("☐"),
                ansi::dim(full_name),
                colors::todo("[todo]")
            ),
            "pending" => format!(
                "  {} {} {}",
                colors::skip("↓"),
                ansi::dim(full_name),
                colors::skip("[skipped]")
            ),
            _ => format!("  {} {}", colors::failure("×"), ansi::white(full_name)),
        };
        out.push(line);
    }
    out.push(String::new());
    out
}

pub fn color_stack_line(line: &str, project_hint: &Regex) -> String {
    let plain = stacks::strip_ansi_simple(line);
    if !Regex::new(r"\s+at\s+").unwrap().is_match(&plain) {
        return plain;
    }
    let Some(caps) = STACK_LOC_RE.captures(&plain) else {
        return ansi::dim(&plain);
    };
    let file = caps.get(1).map(|m| m.as_str()).unwrap_or("");
    let file = std::path::Path::new(file).to_slash_lossy().to_string();
    let line_number = caps.get(2).map(|m| m.as_str()).unwrap_or("0");
    let col_number = caps.get(3).map(|m| m.as_str()).unwrap_or("0");
    let colored_path = if project_hint.is_match(&file) {
        ansi::cyan(&file)
    } else {
        ansi::dim(&file)
    };
    let repl = format!(
        "({}{}{})",
        colored_path,
        ansi::dim(":"),
        ansi::white(&format!("{line_number}:{col_number}"))
    );
    STACK_LOC_RE.replace(&plain, repl).to_string()
}

fn stack_location(line: &str) -> Option<(String, i64)> {
    let simple = stacks::strip_ansi_simple(line);
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\(?([^\s()]+):(\d+):\d+\)?$").unwrap());
    let caps = RE.captures(&simple)?;
    let file = std::path::Path::new(caps.get(1)?.as_str())
        .to_slash_lossy()
        .to_string();
    let ln = caps.get(2)?.as_str().parse::<i64>().ok()?;
    Some((file, ln))
}

pub fn deepest_project_loc(stack_lines: &[String], project_hint: &Regex) -> Option<(String, i64)> {
    let noisy = Regex::new(r"node_modules|vitest|jest").unwrap();
    for i in (0..stack_lines.len()).rev() {
        let simple = stacks::strip_ansi_simple(&stack_lines[i]);
        if stacks::is_stack_line(&simple)
            && project_hint.is_match(&simple)
            && !noisy.is_match(&simple)
        {
            return stack_location(&stack_lines[i]);
        }
    }
    None
}
