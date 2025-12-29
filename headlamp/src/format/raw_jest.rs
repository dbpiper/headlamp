use std::path::Path;

use json5;
use regex::Regex;

use crate::format::{ansi, codeframe, colors, ctx::Ctx, fns, stacks};

#[derive(Debug, Clone)]
pub enum Chunk {
    FailureBlock { title: String, lines: Vec<String> },
    PassFail { badge: String, rel: String },
    Summary { line: String },
    Stack { line: String },
    Other { line: String },
}

pub fn format_jest_output_vitest(raw: &str, ctx: &Ctx, only_failures: bool) -> String {
    let chunks = parse_chunks(raw);
    let (native, _had_parsed) = render_chunks(&chunks, ctx, only_failures);
    let bridge = try_bridge_fallback(raw, ctx, only_failures);
    merge_unique_blocks(&native, bridge.as_deref())
}

fn merge_unique_blocks(native: &str, bridge: Option<&str>) -> String {
    let mut out: Vec<String> = vec![];
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    push_unique_lines(&mut out, &mut seen, native);
    if let Some(b) = bridge.filter(|s| !s.trim().is_empty()) {
        if !out.is_empty() {
            out.push(String::new());
        }
        push_unique_lines(&mut out, &mut seen, b);
    }
    out.join("\n")
}

fn push_unique_lines(
    out: &mut Vec<String>,
    seen: &mut std::collections::BTreeSet<String>,
    text: &str,
) {
    for line in text.split('\n') {
        let key = stacks::strip_ansi_simple(line);
        if seen.insert(key) {
            out.push(line.to_string());
        }
    }
}

fn parse_chunks(raw: &str) -> Vec<Chunk> {
    let lines = raw.split("\n").collect::<Vec<_>>();
    let is_failure_start = Regex::new(r"^\s*●\s+").unwrap();
    let is_suite_line = Regex::new(r"^\s*(PASS|FAIL)\s+").unwrap();
    let is_summary_line =
        Regex::new(r"^\s*(Test Suites:|Tests:|Snapshots:|Time:|Ran all)").unwrap();

    let mut out: Vec<Chunk> = vec![];
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i].to_string();
        let simple = stacks::strip_ansi_simple(&line);
        if is_failure_start.is_match(&simple) {
            let (chunk, next) = collect_failure(
                &lines,
                i,
                &is_failure_start,
                &is_suite_line,
                &is_summary_line,
            );
            out.push(chunk);
            i = next;
            continue;
        }
        if is_suite_line.is_match(&simple) {
            if let Some((badge, rel)) = parse_suite(&simple) {
                out.push(Chunk::PassFail { badge, rel });
            } else {
                out.push(Chunk::Other { line });
            }
            i += 1;
            continue;
        }
        if is_summary_line.is_match(&simple) {
            out.push(Chunk::Summary { line });
            i += 1;
            continue;
        }
        if stacks::is_stack_line(&simple) {
            out.push(Chunk::Stack { line });
            i += 1;
            continue;
        }
        out.push(Chunk::Other { line });
        i += 1;
    }
    out
}

fn collect_failure(
    all_lines: &[&str],
    start_index: usize,
    is_failure_start: &Regex,
    is_suite_line: &Regex,
    is_summary_line: &Regex,
) -> (Chunk, usize) {
    let title = stacks::strip_ansi_simple(all_lines[start_index])
        .trim_start()
        .trim_start_matches('●')
        .trim()
        .to_string();
    let mut buf: Vec<String> = vec![all_lines[start_index].to_string()];
    let mut i = start_index + 1;
    while i < all_lines.len() {
        let simple = stacks::strip_ansi_simple(all_lines[i]);
        let next_is_start = is_failure_start.is_match(&simple)
            || is_suite_line.is_match(&simple)
            || is_summary_line.is_match(&simple);
        let prev_blank =
            stacks::strip_ansi_simple(all_lines.get(i.wrapping_sub(1)).copied().unwrap_or(""))
                .trim()
                .is_empty();
        if next_is_start && prev_blank {
            break;
        }
        buf.push(all_lines[i].to_string());
        i += 1;
    }
    (Chunk::FailureBlock { title, lines: buf }, i)
}

fn parse_suite(line_text: &str) -> Option<(String, String)> {
    let caps = Regex::new(r"^\s*(PASS|FAIL)\s+(.+)$")
        .unwrap()
        .captures(line_text)?;
    Some((
        caps.get(1)?.as_str().to_string(),
        caps.get(2)?.as_str().to_string(),
    ))
}

fn render_chunks(chunks: &[Chunk], ctx: &Ctx, only_failures: bool) -> (String, bool) {
    let mut acc = RenderChunksAcc::default();
    chunks
        .iter()
        .for_each(|chunk| render_chunk(&mut acc, chunk, ctx, only_failures));
    let had_parsed = compute_had_parsed(&acc.out, &acc.seen_files, &acc.seen_failures);
    (acc.out.join("\n"), had_parsed)
}

#[derive(Debug, Default)]
struct RenderChunksAcc {
    out: Vec<String>,
    seen_files: std::collections::BTreeSet<String>,
    seen_failures: std::collections::BTreeSet<String>,
}

fn render_chunk(acc: &mut RenderChunksAcc, chunk: &Chunk, ctx: &Ctx, only_failures: bool) {
    match chunk {
        Chunk::PassFail { badge, rel } => render_pass_fail(acc, ctx, only_failures, badge, rel),
        Chunk::FailureBlock { title, lines } => render_failure_block(acc, ctx, title, lines),
        Chunk::Summary { line } => acc.out.push(line.clone()),
        Chunk::Stack { line } => {
            if ctx.show_stacks {
                acc.out.push(line.clone());
            }
        }
        Chunk::Other { line } => {
            if !only_failures {
                acc.out.push(line.clone());
            }
        }
    }
}

fn render_pass_fail(
    acc: &mut RenderChunksAcc,
    ctx: &Ctx,
    only_failures: bool,
    badge: &str,
    rel: &str,
) {
    let rel2 = rel_path(rel, &ctx.cwd);
    if !acc.seen_files.insert(rel2.clone()) {
        return;
    }
    if only_failures && badge == "PASS" {
        return;
    }
    acc.out.push(fns::build_file_badge_line(
        &rel2,
        (badge == "FAIL") as usize,
    ));
}

fn render_failure_block(acc: &mut RenderChunksAcc, ctx: &Ctx, title: &str, lines: &[String]) {
    acc.out.push(fns::draw_fail_line(ctx.width));
    let rel_file = rel_file_for_failure(lines, ctx);
    let header_text = build_failure_header_text(title, &rel_file);
    acc.out.push(format!(
        "{} {}",
        colors::failure("×"),
        ansi::white(&header_text)
    ));
    let collapsed = stacks::collapse_stacks(lines);
    let deepest = fns::deepest_project_loc_resolved(&collapsed, &ctx.project_hint, &ctx.cwd).map(
        |(file, line, _)| codeframe::Loc {
            file,
            line,
            column: None,
        },
    );
    acc.out.push(String::new());
    acc.out.extend(codeframe::build_code_frame_section(
        lines,
        ctx.show_stacks,
        deepest.as_ref(),
    ));
    push_failure_message_section(&mut acc.out, lines);
    push_console_errors_section(&mut acc.out, lines);
    push_stack_section(&mut acc.out, ctx, &collapsed);
    acc.out.push(fns::draw_fail_line(ctx.width));
    acc.out.push(String::new());
    if !rel_file.is_empty() {
        let _ = acc.seen_failures.insert(format!("{rel_file}|{title}"));
    }
}

fn rel_file_for_failure(lines: &[String], ctx: &Ctx) -> String {
    stacks::first_test_location(lines, &ctx.project_hint)
        .as_ref()
        .and_then(|loc| loc.split(':').next())
        .map(|p| rel_path(p, &ctx.cwd))
        .unwrap_or_default()
}

fn build_failure_header_text(title: &str, rel_file: &str) -> String {
    if !rel_file.is_empty() {
        format!("{rel_file} > {title}")
    } else {
        title.to_string()
    }
}

fn push_failure_message_section(out: &mut Vec<String>, lines: &[String]) {
    let minimal = minimal_message_lines(lines);
    if minimal.is_empty() {
        return;
    }
    out.push(ansi::dim("    Message:"));
    minimal
        .iter()
        .for_each(|line| out.push(format!("      {line}")));
    out.push(String::new());
}

fn push_console_errors_section(out: &mut Vec<String>, lines: &[String]) {
    let console_inline = extract_console_inline(lines);
    if console_inline.is_empty() {
        return;
    }
    out.push(ansi::dim("    Console errors:"));
    console_inline
        .iter()
        .for_each(|line| out.push(format!("      {line}")));
    out.push(String::new());
}

fn push_stack_section(out: &mut Vec<String>, ctx: &Ctx, collapsed: &[String]) {
    if !ctx.show_stacks {
        return;
    }
    let stack_lines = collapsed
        .iter()
        .map(|ln| stacks::strip_ansi_simple(ln))
        .filter(|ln| stacks::is_stack_line(ln))
        .filter(|ln| ctx.project_hint.is_match(ln))
        .take(6)
        .collect::<Vec<_>>();
    if stack_lines.is_empty() {
        return;
    }
    out.push(ansi::dim("    Stack:"));
    stack_lines
        .iter()
        .for_each(|line| out.push(format!("      {line}")));
    out.push(String::new());
}

fn compute_had_parsed(
    out: &[String],
    seen_files: &std::collections::BTreeSet<String>,
    seen_failures: &std::collections::BTreeSet<String>,
) -> bool {
    !seen_files.is_empty()
        || !seen_failures.is_empty()
        || out.iter().any(|line| {
            let simple = stacks::strip_ansi_simple(line);
            simple.trim_start().starts_with("PASS ") || simple.trim_start().starts_with("FAIL ")
        })
}

fn rel_path(abs: &str, cwd: &str) -> String {
    abs.replace('\\', "/").replace(&format!("{cwd}/"), "")
}

fn minimal_message_lines(lines: &[String]) -> Vec<String> {
    let plain = lines
        .iter()
        .map(|ln| stacks::strip_ansi_simple(ln))
        .collect::<Vec<_>>();
    let hint = plain.iter().position(|line_text| {
        Regex::new(r"expect\(.+?\)\.(?:to|not\.)")
            .unwrap()
            .is_match(line_text)
            || Regex::new(r"\bError:?\b").unwrap().is_match(line_text)
    });
    let start = hint.unwrap_or(0);
    let mut acc: Vec<String> = vec![];
    for ln in plain.into_iter().skip(start) {
        if ln.trim().is_empty() {
            break;
        }
        if stacks::is_stack_line(&ln) {
            break;
        }
        acc.push(ln);
    }
    acc
}

fn extract_console_inline(lines: &[String]) -> Vec<String> {
    let mut cand = lines
        .iter()
        .map(|ln| stacks::strip_ansi_simple(ln))
        .filter(|ln| {
            Regex::new(r"\bconsole\.(error|warn)\s*\(")
                .unwrap()
                .is_match(ln)
                || ln.trim_start().starts_with("Error:")
        })
        .map(|ln| ln.trim().to_string())
        .filter(|ln| !ln.is_empty())
        .collect::<Vec<_>>();
    cand.sort_by_key(|b| std::cmp::Reverse(b.len()));
    cand.truncate(3);
    cand
}

fn try_bridge_fallback(raw: &str, ctx: &Ctx, only_failures: bool) -> Option<String> {
    let bridge_path = extract_bridge_path(raw, &ctx.cwd).or_else(|| {
        let def = Path::new(&ctx.cwd).join("coverage").join("jest-run.json");
        def.exists().then(|| def.to_string_lossy().to_string())
    })?;

    let text = std::fs::read_to_string(&bridge_path).ok()?;
    let bridge_json: crate::format::bridge::BridgeJson =
        serde_json::from_str(&text).ok().or_else(|| {
            json5::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| serde_json::from_value::<crate::format::bridge::BridgeJson>(v).ok())
        })?;

    Some(crate::format::vitest::render_vitest_from_test_model(
        &bridge_json,
        ctx,
        only_failures,
    ))
}

fn extract_bridge_path(raw: &str, cwd: &str) -> Option<String> {
    let re =
        Regex::new(r#"Test results written to:\s+([^\n\r]+jest-bridge-[^\s'"]+\.json)"#).ok()?;
    let matches = re.captures_iter(raw).collect::<Vec<_>>();
    let last = matches.last()?;
    let json_path = last
        .get(1)?
        .as_str()
        .trim()
        .trim_matches(&['"', '\'', '`'][..]);
    if json_path.starts_with('/') {
        return Some(json_path.to_string());
    }
    Some(format!("{}/{}", cwd.replace('\\', "/"), json_path))
}
