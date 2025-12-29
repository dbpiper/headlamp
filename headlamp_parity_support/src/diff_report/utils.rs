use std::cmp::min;

pub(super) fn strip_ansi(text: &str) -> String {
    headlamp::format::stacks::strip_ansi_simple(text)
}

pub(super) fn strip_osc8(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == 0x1b
            && bytes.get(index + 1) == Some(&b']')
            && bytes.get(index + 2) == Some(&b'8')
            && bytes.get(index + 3) == Some(&b';')
            && bytes.get(index + 4) == Some(&b';')
        {
            index += 5;
            while index < bytes.len() && bytes[index] != 0x07 {
                index += 1;
            }
            index = min(bytes.len(), index + 1);
            continue;
        }
        out.push(bytes[index] as char);
        index += 1;
    }
    out
}

pub(super) fn normalize_temp_paths(text: &str) -> String {
    let normalized_separators = text.replace('\\', "/");
    let without_private = normalized_separators.replace("/private/", "/");
    replace_prefix_runs(&without_private, "/var/folders/", "<TMP>/var/folders/")
}

fn replace_prefix_runs(text: &str, prefix: &str, replacement: &str) -> String {
    text.lines()
        .map(|line| {
            if let Some(idx) = line.find(prefix) {
                let before = &line[..idx];
                let after = &line[idx + prefix.len()..];
                let (hashy, rest) = after.split_once("/T/").unwrap_or((after, ""));
                if rest.is_empty() {
                    return line.to_string();
                }
                format!("{before}{replacement}{hashy}/T/{rest}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn trim_line_ends(text: &str) -> String {
    text.lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn collapse_blank_runs(text: &str) -> String {
    let mut out: Vec<&str> = vec![];
    let mut prev_blank = false;
    for line in text.lines() {
        let blank = line.trim().is_empty();
        if blank && prev_blank {
            continue;
        }
        out.push(line);
        prev_blank = blank;
    }
    out.join("\n")
}

pub(super) fn find_first_line_mismatch(ts: &[String], rs: &[String]) -> Option<usize> {
    let shared = min(ts.len(), rs.len());
    (0..shared).find(|&i| ts.get(i) != rs.get(i)).or({
        if ts.len() != rs.len() {
            Some(shared)
        } else {
            None
        }
    })
}
