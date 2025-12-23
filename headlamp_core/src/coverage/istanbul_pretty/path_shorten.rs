use std::path::Path;

pub fn shorten_path_preserving_filename(rel_path: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let normalized = rel_path.replace('\\', "/");
    if visible_width(&normalized) <= max_width {
        return normalized;
    }

    let ellipsis = "…";
    let parts = normalized.split('/').collect::<Vec<_>>();
    let base = parts.last().copied().unwrap_or("");
    let dirs = parts[..parts.len().saturating_sub(1)].to_vec();

    let (stem, ext) = split_multi_ext(base);
    let base_label = format!("{}{}", token_aware_middle(stem, max_width.saturating_sub(visible_width(&ext))), ext);
    if visible_width(&base_label) >= max_width {
        return slice_balanced(base, max_width, ellipsis);
    }

    let mut head_keep = 1usize;
    let mut tail_keep = 1usize;
    while head_keep + tail_keep <= dirs.len() + 2 {
        let label = join_parts(&dirs, head_keep, tail_keep, &base_label, ellipsis);
        if visible_width(&label) <= max_width {
            return label;
        }
        if head_keep <= tail_keep {
            head_keep += 1;
        } else {
            tail_keep += 1;
        }
    }
    slice_balanced(&base_label, max_width, ellipsis)
}

fn join_parts(dirs: &[&str], head_keep: usize, tail_keep: usize, base: &str, ellipsis: &str) -> String {
    let head = dirs.iter().take(head_keep).copied().collect::<Vec<_>>();
    let tail = dirs
        .iter()
        .rev()
        .take(tail_keep)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .copied()
        .collect::<Vec<_>>();
    let mut segs: Vec<String> = vec![];
    if !head.is_empty() {
        segs.push(head.join("/"));
    }
    if !dirs.is_empty() && (head_keep + tail_keep) < dirs.len() {
        segs.push(ellipsis.to_string());
    }
    if !tail.is_empty() {
        segs.push(tail.join("/"));
    }
    segs.push(base.to_string());
    segs.join("/")
}

fn visible_width(text: &str) -> usize {
    text.chars().count()
}

fn slice_balanced(input: &str, width: usize, ellipsis: &str) -> String {
    if visible_width(input) <= width {
        return input.to_string();
    }
    if width <= visible_width(ellipsis) {
        return ellipsis.chars().take(width).collect();
    }
    let keep = width - visible_width(ellipsis);
    let head = (keep + 1) / 2;
    let tail = keep / 2;
    let start = input.chars().take(head).collect::<String>();
    let end = input.chars().rev().take(tail).collect::<Vec<_>>().into_iter().rev().collect::<String>();
    format!("{start}{ellipsis}{end}")
}

fn split_multi_ext(base: &str) -> (String, String) {
    let endings = [
        ".test.ts",
        ".spec.ts",
        ".d.ts",
        ".schema.ts",
        ".schema.js",
        ".config.ts",
        ".config.js",
    ];
    for ending in endings {
        if base.ends_with(ending) {
            let stem = base.trim_end_matches(ending).to_string();
            return (stem, ending.to_string());
        }
    }
    let ext = Path::new(base)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| format!(".{s}"))
        .unwrap_or_default();
    let stem = base.trim_end_matches(&ext).to_string();
    (stem, ext)
}

fn token_aware_middle(stem: String, budget: usize) -> String {
    let ellipsis = "…";
    if budget == 0 {
        return String::new();
    }
    if visible_width(&stem) <= budget {
        return stem;
    }
    if budget <= visible_width(ellipsis) {
        return ellipsis.chars().take(budget).collect();
    }
    let tokens = stem
        .split_inclusive(|c: char| c == '.' || c == '_' || c == '-')
        .collect::<Vec<_>>();
    let mut left_index = 0usize;
    let mut right_index = tokens.len().saturating_sub(1);
    let mut left = String::new();
    let mut right = String::new();
    while left_index <= right_index {
        let try_l = format!("{left}{}", tokens[left_index]);
        let try_r = format!("{}{}", tokens[right_index], right);
        let cand_l = format!("{try_l}{ellipsis}{right}");
        let cand_r = format!("{left}{ellipsis}{try_r}");
        let can_l = visible_width(&cand_l) <= budget;
        let can_r = visible_width(&cand_r) <= budget;
        if can_l && (!can_r || visible_width(&cand_l) >= visible_width(&cand_r)) {
            left = try_l;
            left_index += 1;
        } else if can_r {
            right = try_r;
            right_index = right_index.saturating_sub(1);
        } else {
            break;
        }
    }
    let glued = format!("{left}{ellipsis}{right}");
    if visible_width(&glued) <= budget {
        glued
    } else {
        slice_balanced(&stem, budget, ellipsis)
    }
}


