use std::cmp::min;

use super::parity_meta::ParityCompareInput;
use super::token_ast;

pub fn build_parity_report_with_meta(compare: &ParityCompareInput) -> String {
    let mut sections: Vec<String> = vec![];
    sections.push(build_artifact_summary(compare));
    sections.push(build_token_ast_summary(compare));
    sections.push(build_block_order_summary(compare));
    sections.push(build_classification_section(
        &compare.normalized_ts,
        &compare.normalized_rs,
    ));
    sections.push(build_first_mismatch_section(
        &compare.normalized_ts,
        &compare.normalized_rs,
    ));
    sections.push(build_blank_runs_section(
        &compare.normalized_ts,
        &compare.normalized_rs,
    ));
    sections.push(build_table_section(
        &compare.normalized_ts,
        &compare.normalized_rs,
    ));
    sections.push(build_istanbul_table_section(
        &compare.normalized_ts,
        &compare.normalized_rs,
    ));
    sections.push(build_counts_section(
        &compare.normalized_ts,
        &compare.normalized_rs,
    ));
    sections
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn build_token_ast_summary(compare: &ParityCompareInput) -> String {
    let ts_raw = token_ast::build_token_stream(&compare.raw_ts);
    let rs_raw = token_ast::build_token_stream(&compare.raw_rs);
    let ts_norm = token_ast::build_token_stream(&compare.normalized_ts);
    let rs_norm = token_ast::build_token_stream(&compare.normalized_rs);

    let mut lines: Vec<String> = vec!["Token stats".to_string()];
    lines.extend(render_token_stats_side("ts_raw", &ts_raw.stats));
    lines.extend(render_token_stats_side("rs_raw", &rs_raw.stats));
    lines.extend(render_token_stats_side("ts_norm", &ts_norm.stats));
    lines.extend(render_token_stats_side("rs_norm", &rs_norm.stats));
    lines.extend(render_token_delta(&ts_norm.stats, &rs_norm.stats));
    lines.join("\n")
}

fn render_token_stats_side(label: &str, stats: &token_ast::TokenStats) -> Vec<String> {
    let counts = stats
        .counts_by_kind
        .iter()
        .map(|(k, v)| format!("{k:?}={v}"))
        .collect::<Vec<_>>()
        .join(" ");
    vec![format!(
        "- {label}: tokens={} visible_width_total={} {counts}",
        stats.token_count, stats.visible_width_total
    )]
}

fn render_token_delta(ts: &token_ast::TokenStats, rs: &token_ast::TokenStats) -> Vec<String> {
    let mut kinds = ts
        .counts_by_kind
        .keys()
        .chain(rs.counts_by_kind.keys())
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    kinds.sort();
    let by_kind = kinds
        .into_iter()
        .map(|k| {
            let a = ts.counts_by_kind.get(&k).copied().unwrap_or(0);
            let b = rs.counts_by_kind.get(&k).copied().unwrap_or(0);
            format!("{k:?}:{a}->{b}")
        })
        .collect::<Vec<_>>()
        .join(" ");
    vec![format!("- norm_token_delta: {by_kind}")]
}

fn build_block_order_summary(compare: &ParityCompareInput) -> String {
    let ts = token_ast::build_document_ast(&compare.normalized_ts);
    let rs = token_ast::build_document_ast(&compare.normalized_rs);
    let ts_order = ts
        .blocks
        .iter()
        .map(|b| b.hash.as_str())
        .collect::<Vec<_>>();
    let rs_order = rs
        .blocks
        .iter()
        .map(|b| b.hash.as_str())
        .collect::<Vec<_>>();
    let mut lines: Vec<String> = vec!["Block order".to_string()];
    lines.push(format!("- ts: [{}]", ts_order.join(",")));
    lines.push(format!("- rs: [{}]", rs_order.join(",")));
    lines.extend(render_block_moves(&ts_order, &rs_order));
    lines.join("\n")
}

fn render_block_moves(ts_order: &[&str], rs_order: &[&str]) -> Vec<String> {
    let ts_pos = ts_order
        .iter()
        .enumerate()
        .map(|(i, h)| (h.to_string(), i))
        .collect::<std::collections::BTreeMap<String, usize>>();
    let rs_pos = rs_order
        .iter()
        .enumerate()
        .map(|(i, h)| (h.to_string(), i))
        .collect::<std::collections::BTreeMap<String, usize>>();
    let moved = ts_pos
        .iter()
        .filter_map(|(h, ti)| rs_pos.get(h).map(|ri| (h.as_str(), *ti, *ri)))
        .filter(|(_, ti, ri)| ti != ri)
        .take(12)
        .map(|(h, ti, ri)| format!("  - {h}: ts={ti} rs={ri}"))
        .collect::<Vec<_>>();
    let missing_in_rs = ts_pos
        .keys()
        .filter(|h| !rs_pos.contains_key(*h))
        .take(12)
        .map(|h| format!("  - missing_in_rs: {h}"))
        .collect::<Vec<_>>();
    let missing_in_ts = rs_pos
        .keys()
        .filter(|h| !ts_pos.contains_key(*h))
        .take(12)
        .map(|h| format!("  - missing_in_ts: {h}"))
        .collect::<Vec<_>>();
    [moved, missing_in_rs, missing_in_ts]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
}

pub fn build_parity_report(out_ts: &str, out_rs: &str) -> String {
    build_parity_report_with_meta(&ParityCompareInput {
        raw_ts: out_ts.to_string(),
        raw_rs: out_rs.to_string(),
        normalized_ts: out_ts.to_string(),
        normalized_rs: out_rs.to_string(),
        meta: super::parity_meta::ParityCompareMeta {
            ts: super::parity_meta::ParitySideMeta {
                raw_bytes: out_ts.as_bytes().len(),
                raw_lines: out_ts.lines().count(),
                normalized_bytes: out_ts.as_bytes().len(),
                normalized_lines: out_ts.lines().count(),
                normalization: super::parity_meta::NormalizationMeta {
                    normalizer: super::parity_meta::NormalizerKind::NonTty,
                    used_fallback: false,
                    last_failed_tests_line: None,
                    last_test_files_line: None,
                    last_box_table_top_line: None,
                    stages: vec![],
                },
            },
            rs: super::parity_meta::ParitySideMeta {
                raw_bytes: out_rs.as_bytes().len(),
                raw_lines: out_rs.lines().count(),
                normalized_bytes: out_rs.as_bytes().len(),
                normalized_lines: out_rs.lines().count(),
                normalization: super::parity_meta::NormalizationMeta {
                    normalizer: super::parity_meta::NormalizerKind::NonTty,
                    used_fallback: false,
                    last_failed_tests_line: None,
                    last_test_files_line: None,
                    last_box_table_top_line: None,
                    stages: vec![],
                },
            },
        },
    })
}

fn build_artifact_summary(compare: &ParityCompareInput) -> String {
    let ts = &compare.meta.ts;
    let rs = &compare.meta.rs;
    let ts_norm_empty_raw_nonempty = ts.normalized_bytes == 0 && ts.raw_bytes > 0;
    let rs_norm_empty_raw_nonempty = rs.normalized_bytes == 0 && rs.raw_bytes > 0;
    [
        "Artifact summary".to_string(),
        format!(
            "- ts: raw_bytes={} raw_lines={} normalized_bytes={} normalized_lines={}",
            ts.raw_bytes, ts.raw_lines, ts.normalized_bytes, ts.normalized_lines
        ),
        format!(
            "- rs: raw_bytes={} raw_lines={} normalized_bytes={} normalized_lines={}",
            rs.raw_bytes, rs.raw_lines, rs.normalized_bytes, rs.normalized_lines
        ),
        format!(
            "- normalized_empty_but_raw_nonempty: ts={} rs={}",
            ts_norm_empty_raw_nonempty, rs_norm_empty_raw_nonempty
        ),
        format!(
            "- normalizer: ts={:?} fallback={} | rs={:?} fallback={}",
            ts.normalization.normalizer,
            ts.normalization.used_fallback,
            rs.normalization.normalizer,
            rs.normalization.used_fallback
        ),
    ]
    .join("\n")
}

fn build_classification_section(out_ts: &str, out_rs: &str) -> String {
    let ts_no_ansi = strip_ansi(out_ts);
    let rs_no_ansi = strip_ansi(out_rs);
    let ts_no_osc8 = strip_osc8(out_ts);
    let rs_no_osc8 = strip_osc8(out_rs);

    let ansi_only = out_ts != out_rs && ts_no_ansi == rs_no_ansi;
    let osc8_only = out_ts != out_rs && ts_no_osc8 == rs_no_osc8;

    let ts_trimmed = trim_line_ends(out_ts);
    let rs_trimmed = trim_line_ends(out_rs);
    let trailing_ws_only = out_ts != out_rs && ts_trimmed == rs_trimmed;

    let ts_collapsed = collapse_blank_runs(&ts_trimmed);
    let rs_collapsed = collapse_blank_runs(&rs_trimmed);
    let blank_runs_only = out_ts != out_rs && ts_collapsed == rs_collapsed;

    let ts_path_norm = normalize_temp_paths(&ts_no_ansi);
    let rs_path_norm = normalize_temp_paths(&rs_no_ansi);
    let path_only = out_ts != out_rs && ts_path_norm == rs_path_norm;

    let bullet = |label: &str, yes: bool| format!("- {label}: {}", if yes { "yes" } else { "no" });
    [
        "Parity mismatch analysis".to_string(),
        bullet("ANSI-only", ansi_only),
        bullet("OSC8-only", osc8_only),
        bullet("trailing-whitespace-only", trailing_ws_only),
        bullet("blank-runs-only", blank_runs_only),
        bullet("path-only", path_only),
    ]
    .join("\n")
}

fn build_first_mismatch_section(out_ts: &str, out_rs: &str) -> String {
    let ts_lines = out_ts.lines().collect::<Vec<_>>();
    let rs_lines = out_rs.lines().collect::<Vec<_>>();
    let max_len = ts_lines.len().max(rs_lines.len());
    let first = (0..max_len).find(|&i| ts_lines.get(i) != rs_lines.get(i));
    let Some(first) = first else {
        return String::new();
    };
    let window_before = 2usize;
    let window_after = 3usize;
    let start = first.saturating_sub(window_before);
    let end = min(max_len, first + window_after);
    let mut out: Vec<String> = vec![];
    out.push(format!("First mismatch at line {}", first + 1));
    for i in start..end {
        let ts = ts_lines.get(i).copied().unwrap_or("<missing>");
        let rs = rs_lines.get(i).copied().unwrap_or("<missing>");
        if ts == rs {
            out.push(format!(" {ln:>4}  =  {ts}", ln = i + 1));
            continue;
        }
        out.push(format!(" {ln:>4} TS {ts}", ln = i + 1));
        out.push(format!(" {ln:>4} RS {rs}", ln = i + 1));
        out.push(format!(
            "      TS len={} vis={} | RS len={} vis={}",
            ts.chars().count(),
            strip_ansi(ts).chars().count(),
            rs.chars().count(),
            strip_ansi(rs).chars().count()
        ));
    }
    out.join("\n")
}

fn build_table_section(out_ts: &str, out_rs: &str) -> String {
    let ts_blocks = find_box_table_blocks(out_ts);
    let rs_blocks = find_box_table_blocks(out_rs);
    if ts_blocks.is_empty() && rs_blocks.is_empty() {
        return String::new();
    }
    let mut out: Vec<String> = vec![];
    out.push(format!(
        "Box tables: ts={} rs={}",
        ts_blocks.len(),
        rs_blocks.len()
    ));
    let shared = min(ts_blocks.len(), rs_blocks.len());
    for i in 0..shared {
        let ts = &ts_blocks[i];
        let rs = &rs_blocks[i];
        let summary = summarize_box_table(ts, rs);
        if summary.ts_rows == summary.rs_rows && summary.ts_blank_rows == summary.rs_blank_rows {
            continue;
        }
        out.push(format!(
            "- table[{}] rows: ts={} rs={} | blank_rows: ts={} rs={}",
            i, summary.ts_rows, summary.rs_rows, summary.ts_blank_rows, summary.rs_blank_rows
        ));
        if let Some(detail) = first_box_row_cell_mismatch(ts, rs) {
            out.push(format!(
                "  first cell mismatch: row={} col={} (ts_trim='{}' rs_trim='{}')",
                detail.row_index + 1,
                detail.col_index + 1,
                detail.ts_trim,
                detail.rs_trim
            ));
        }
        if let Some(mismatch) = summary.first_aligned_mismatch {
            out.push(format!(
                "  first aligned mismatch: ts_row={} rs_row={}",
                mismatch.ts_row_index + 1,
                mismatch.rs_row_index + 1
            ));
            out.push(format!("    TS: {}", mismatch.ts_line));
            out.push(format!("    RS: {}", mismatch.rs_line));
        }
        if !summary.notes.is_empty() {
            out.extend(summary.notes.into_iter().take(6).map(|s| format!("  {s}")));
        }
    }
    out.join("\n")
}

fn build_istanbul_table_section(out_ts: &str, out_rs: &str) -> String {
    let ts_blocks = find_istanbul_pipe_table_blocks(out_ts);
    let rs_blocks = find_istanbul_pipe_table_blocks(out_rs);
    if ts_blocks.is_empty() && rs_blocks.is_empty() {
        return String::new();
    }
    let mut out: Vec<String> = vec![];
    out.push(format!(
        "Istanbul pipe tables: ts={} rs={}",
        ts_blocks.len(),
        rs_blocks.len()
    ));
    let shared = min(ts_blocks.len(), rs_blocks.len());
    for index in 0..shared {
        let ts = &ts_blocks[index];
        let rs = &rs_blocks[index];
        if ts == rs {
            continue;
        }
        let first = find_first_line_mismatch(ts, rs);
        out.push(format!(
            "- table[{}] lines: ts={} rs={} first_mismatch_line={}",
            index,
            ts.len(),
            rs.len(),
            first.map(|n| n + 1).unwrap_or(0)
        ));
        if let Some(first) = first {
            out.push(format!(
                "  TS: {}",
                ts.get(first).map(String::as_str).unwrap_or("<missing>")
            ));
            out.push(format!(
                "  RS: {}",
                rs.get(first).map(String::as_str).unwrap_or("<missing>")
            ));
        }
    }
    out.join("\n")
}

fn build_counts_section(out_ts: &str, out_rs: &str) -> String {
    let ts = strip_ansi(out_ts);
    let rs = strip_ansi(out_rs);
    let needles = [
        "Hotspots:",
        "Uncovered functions:",
        "Coverage summary",
        "Uncovered Line #s",
    ];
    let mut out: Vec<String> = vec!["Section marker counts (ANSI-stripped)".to_string()];
    needles.iter().for_each(|needle| {
        let c_ts = ts.matches(needle).count();
        let c_rs = rs.matches(needle).count();
        if c_ts != c_rs {
            out.push(format!("- '{needle}': ts={c_ts} rs={c_rs}"));
        }
    });
    if out.len() == 1 {
        String::new()
    } else {
        out.join("\n")
    }
}

fn build_blank_runs_section(out_ts: &str, out_rs: &str) -> String {
    let ts = strip_ansi(out_ts);
    let rs = strip_ansi(out_rs);
    let ts_stats = blank_run_stats(&ts);
    let rs_stats = blank_run_stats(&rs);
    if ts_stats == rs_stats {
        return String::new();
    }
    [
        "Blank run stats (ANSI-stripped)".to_string(),
        format!(
            "- ts: blank_lines={} runs={} max_run={}",
            ts_stats.blank_lines, ts_stats.runs, ts_stats.max_run
        ),
        format!(
            "- rs: blank_lines={} runs={} max_run={}",
            rs_stats.blank_lines, rs_stats.runs, rs_stats.max_run
        ),
    ]
    .join("\n")
}

fn strip_ansi(text: &str) -> String {
    headlamp_core::format::stacks::strip_ansi_simple(text)
}

fn strip_osc8(text: &str) -> String {
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

fn normalize_temp_paths(text: &str) -> String {
    // Keep this intentionally simple and targeted: enough to classify “path-only” diffs.
    // macOS temp dirs often look like:
    // - /var/folders/.../T/...
    // - /private/var/folders/.../T/...
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
                // Collapse the random hash path segment until we hit "/T/" (if present).
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

fn trim_line_ends(text: &str) -> String {
    text.lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

fn collapse_blank_runs(text: &str) -> String {
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

type BoxBlock = Vec<String>;

fn find_box_table_blocks(text: &str) -> Vec<BoxBlock> {
    let mut blocks: Vec<BoxBlock> = vec![];
    let mut current: Option<BoxBlock> = None;
    for raw_line in text.lines() {
        let line = strip_ansi(raw_line);
        let starts = line.starts_with('┌');
        let ends = line.starts_with('└');
        if starts {
            current = Some(vec![raw_line.to_string()]);
            continue;
        }
        if let Some(block) = current.as_mut() {
            block.push(raw_line.to_string());
            if ends {
                blocks.push(current.take().unwrap_or_default());
            }
        }
    }
    blocks
}

type PipeBlock = Vec<String>;

fn find_istanbul_pipe_table_blocks(text: &str) -> Vec<PipeBlock> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut blocks: Vec<PipeBlock> = vec![];
    let mut i = 0usize;
    while i < lines.len() {
        let line = strip_ansi(lines[i]);
        let is_sep = line.starts_with('-') && line.contains('|');
        if !is_sep {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < lines.len() {
            let ln = strip_ansi(lines[i]);
            if ln.trim().is_empty() {
                break;
            }
            if !(ln.contains('|') || (ln.starts_with('-') && ln.contains('|'))) {
                break;
            }
            i += 1;
        }
        let block = lines[start..i]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        if block.len() >= 3 {
            blocks.push(block);
        }
    }
    blocks
}

fn count_blank_box_rows(block: &BoxBlock) -> usize {
    block
        .iter()
        .filter(|raw| is_blank_box_row(&strip_ansi(raw)))
        .count()
}

fn is_blank_box_row(line: &str) -> bool {
    if !line.starts_with('│') || !line.ends_with('│') {
        return false;
    }
    line.chars().all(|c| c == '│' || c == ' ')
}

struct BoxCellMismatch {
    row_index: usize,
    col_index: usize,
    ts_trim: String,
    rs_trim: String,
}

fn first_box_row_cell_mismatch(ts: &BoxBlock, rs: &BoxBlock) -> Option<BoxCellMismatch> {
    let shared = min(ts.len(), rs.len());
    for row_index in 0..shared {
        let ts_line = strip_ansi(ts.get(row_index)?.as_str());
        let rs_line = strip_ansi(rs.get(row_index)?.as_str());
        if !(ts_line.starts_with('│') && rs_line.starts_with('│')) {
            continue;
        }
        let ts_cells = split_box_cells(&ts_line);
        let rs_cells = split_box_cells(&rs_line);
        let shared_cells = min(ts_cells.len(), rs_cells.len());
        for col_index in 0..shared_cells {
            let ts_trim = ts_cells[col_index].trim().to_string();
            let rs_trim = rs_cells[col_index].trim().to_string();
            if ts_trim != rs_trim {
                return Some(BoxCellMismatch {
                    row_index,
                    col_index,
                    ts_trim,
                    rs_trim,
                });
            }
        }
    }
    None
}

fn split_box_cells(line: &str) -> Vec<&str> {
    line.split('│')
        .skip(1)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .skip(1)
        .rev()
        .collect::<Vec<_>>()
}

fn find_first_line_mismatch(ts: &[String], rs: &[String]) -> Option<usize> {
    let shared = min(ts.len(), rs.len());
    (0..shared).find(|&i| ts.get(i) != rs.get(i)).or_else(|| {
        if ts.len() != rs.len() {
            Some(shared)
        } else {
            None
        }
    })
}

fn align_box_table_rows(ts: &BoxBlock, rs: &BoxBlock) -> Vec<String> {
    let ts_norm = ts.iter().map(|l| normalize_box_row(l)).collect::<Vec<_>>();
    let rs_norm = rs.iter().map(|l| normalize_box_row(l)).collect::<Vec<_>>();
    let mut i = 0usize;
    let mut j = 0usize;
    let mut notes: Vec<String> = vec![];
    while i < ts_norm.len() && j < rs_norm.len() {
        if ts_norm[i] == rs_norm[j] {
            i += 1;
            j += 1;
            continue;
        }
        if ts_norm.get(i).is_some_and(|k| k.kind == BoxRowKind::Blank)
            && ts_norm
                .get(i + 1)
                .is_some_and(|next| next == rs_norm.get(j).unwrap())
        {
            notes.push(format!("extra blank filler row in TS at row {}", i + 1));
            i += 1;
            continue;
        }
        if rs_norm.get(j).is_some_and(|k| k.kind == BoxRowKind::Blank)
            && rs_norm
                .get(j + 1)
                .is_some_and(|next| next == ts_norm.get(i).unwrap())
        {
            notes.push(format!("extra blank filler row in RS at row {}", j + 1));
            j += 1;
            continue;
        }
        notes.push(format!("row mismatch TS#{} vs RS#{}", i + 1, j + 1));
        break;
    }
    if i < ts_norm.len() && j == rs_norm.len() {
        notes.push(format!("TS has {} extra trailing rows", ts_norm.len() - i));
    }
    if j < rs_norm.len() && i == ts_norm.len() {
        notes.push(format!("RS has {} extra trailing rows", rs_norm.len() - j));
    }
    notes
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BlankRunStats {
    blank_lines: usize,
    runs: usize,
    max_run: usize,
}

fn blank_run_stats(text: &str) -> BlankRunStats {
    let mut blank_lines = 0usize;
    let mut runs = 0usize;
    let mut max_run = 0usize;
    let mut current_run = 0usize;
    for line in text.lines() {
        if line.trim().is_empty() {
            blank_lines += 1;
            current_run += 1;
            continue;
        }
        if current_run > 0 {
            runs += 1;
            max_run = max_run.max(current_run);
            current_run = 0;
        }
    }
    if current_run > 0 {
        runs += 1;
        max_run = max_run.max(current_run);
    }
    BlankRunStats {
        blank_lines,
        runs,
        max_run,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BoxRowKind {
    Border,
    Data,
    Blank,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BoxRowSig {
    kind: BoxRowKind,
    cells_trim: Vec<String>,
}

#[derive(Debug, Clone)]
struct AlignedRowMismatch {
    ts_row_index: usize,
    rs_row_index: usize,
    ts_line: String,
    rs_line: String,
}

#[derive(Debug, Clone)]
struct BoxTableSummary {
    ts_rows: usize,
    rs_rows: usize,
    ts_blank_rows: usize,
    rs_blank_rows: usize,
    first_aligned_mismatch: Option<AlignedRowMismatch>,
    notes: Vec<String>,
}

fn summarize_box_table(ts: &BoxBlock, rs: &BoxBlock) -> BoxTableSummary {
    let notes = align_box_table_rows(ts, rs);
    BoxTableSummary {
        ts_rows: ts.len(),
        rs_rows: rs.len(),
        ts_blank_rows: count_blank_box_rows(ts),
        rs_blank_rows: count_blank_box_rows(rs),
        first_aligned_mismatch: first_aligned_box_row_mismatch(ts, rs),
        notes,
    }
}

fn first_aligned_box_row_mismatch(ts: &BoxBlock, rs: &BoxBlock) -> Option<AlignedRowMismatch> {
    let ts_norm = ts.iter().map(|l| normalize_box_row(l)).collect::<Vec<_>>();
    let rs_norm = rs.iter().map(|l| normalize_box_row(l)).collect::<Vec<_>>();
    let mut i = 0usize;
    let mut j = 0usize;
    while i < ts_norm.len() && j < rs_norm.len() {
        if ts_norm[i] == rs_norm[j] {
            i += 1;
            j += 1;
            continue;
        }
        if is_extra_blank_on_left(&ts_norm, i, &rs_norm, j) {
            i += 1;
            continue;
        }
        if is_extra_blank_on_left(&rs_norm, j, &ts_norm, i) {
            j += 1;
            continue;
        }
        return Some(AlignedRowMismatch {
            ts_row_index: i,
            rs_row_index: j,
            ts_line: strip_ansi(ts.get(i).map(String::as_str).unwrap_or("<missing>")),
            rs_line: strip_ansi(rs.get(j).map(String::as_str).unwrap_or("<missing>")),
        });
    }
    None
}

fn is_extra_blank_on_left(
    left: &[BoxRowSig],
    left_index: usize,
    right: &[BoxRowSig],
    right_index: usize,
) -> bool {
    left.get(left_index)
        .is_some_and(|k| k.kind == BoxRowKind::Blank)
        && left
            .get(left_index + 1)
            .is_some_and(|next| right.get(right_index).is_some_and(|r| r == next))
}

fn normalize_box_row(raw: &str) -> BoxRowSig {
    let line = strip_ansi(raw);
    if line.starts_with('┌') || line.starts_with('┼') || line.starts_with('└') {
        return BoxRowSig {
            kind: BoxRowKind::Border,
            cells_trim: vec![],
        };
    }
    if is_blank_box_row(&line) {
        return BoxRowSig {
            kind: BoxRowKind::Blank,
            cells_trim: vec![],
        };
    }
    if line.starts_with('│') && line.ends_with('│') {
        return BoxRowSig {
            kind: BoxRowKind::Data,
            cells_trim: split_box_cells(&line)
                .into_iter()
                .map(|c| c.trim().to_string())
                .collect(),
        };
    }
    BoxRowSig {
        kind: BoxRowKind::Other,
        cells_trim: vec![line.trim().to_string()],
    }
}
