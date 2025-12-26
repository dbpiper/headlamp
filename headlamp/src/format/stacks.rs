use regex::Regex;

use crate::format::ansi;

pub fn strip_ansi_simple(text: &str) -> String {
    String::from_utf8_lossy(&strip_ansi_escapes::strip(text.as_bytes())).to_string()
}

pub fn is_stack_line(line: &str) -> bool {
    Regex::new(r"\s+at\s+").unwrap().is_match(line)
}

pub fn first_test_location(lines: &[String], project_hint: &Regex) -> Option<String> {
    let re1 = Regex::new(r"\(([^()]+?:\d+:\d+)\)").unwrap();
    let re2 = Regex::new(r"\s([\w./-]+?:\d+:\d+)\s*$").unwrap();
    for ln in lines {
        if let Some(c) = re1.captures(ln).and_then(|c| c.get(1)).map(|m| m.as_str())
            && project_hint.is_match(c)
        {
            return Some(c.to_string());
        }
        if let Some(c) = re2.captures(ln).and_then(|c| c.get(1)).map(|m| m.as_str())
            && project_hint.is_match(c)
        {
            return Some(c.to_string());
        }
    }
    None
}

pub fn collapse_stacks(lines: &[String]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let mut hidden = 0usize;
    let flush = |out: &mut Vec<String>, hidden: &mut usize| {
        if *hidden > 0 {
            out.push(ansi::gray(&format!(
                "      … {} stack frame{} hidden",
                *hidden,
                if *hidden == 1 { "" } else { "s" }
            )));
            *hidden = 0;
        }
    };

    for raw in lines {
        let ln = strip_ansi_simple(raw);
        if is_stack_line(&ln) {
            let noisy = ln.contains("node_modules/") || ln.contains(" at node:");
            if noisy {
                hidden += 1;
                continue;
            }
            flush(&mut out, &mut hidden);
            out.push(raw.clone());
        } else {
            flush(&mut out, &mut hidden);
            out.push(raw.clone());
        }
    }
    flush(&mut out, &mut hidden);
    out
}
