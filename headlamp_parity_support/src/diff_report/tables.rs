use std::cmp::min;

use super::utils::{find_first_line_mismatch, strip_ansi};

pub(super) fn build_table_section(
    label_0: &str,
    label_1: &str,
    out_ts: &str,
    out_rs: &str,
) -> String {
    let ts_blocks = find_box_table_blocks(out_ts);
    let rs_blocks = find_box_table_blocks(out_rs);
    if ts_blocks.is_empty() && rs_blocks.is_empty() {
        return String::new();
    }
    let mut out: Vec<String> = vec![];
    out.push(format!(
        "Box tables: {label_0}={} {label_1}={}",
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
            "- table[{i}] rows: {label_0}={} {label_1}={} | blank_rows: {label_0}={} {label_1}={}",
            summary.ts_rows, summary.rs_rows, summary.ts_blank_rows, summary.rs_blank_rows
        ));
        if let Some(detail) = first_box_row_cell_mismatch(ts, rs) {
            out.push(format!(
                "  first cell mismatch: row={} col={} ({label_0}_trim='{}' {label_1}_trim='{}')",
                detail.row_index + 1,
                detail.col_index + 1,
                detail.ts_trim,
                detail.rs_trim
            ));
        }
        if let Some(mismatch) = summary.first_aligned_mismatch {
            out.push(format!(
                "  first aligned mismatch: {label_0}_row={} {label_1}_row={}",
                mismatch.ts_row_index + 1,
                mismatch.rs_row_index + 1
            ));
            out.push(format!("    {label_0}: {}", mismatch.ts_line));
            out.push(format!("    {label_1}: {}", mismatch.rs_line));
        }
        if !summary.notes.is_empty() {
            out.extend(summary.notes.into_iter().take(6).map(|s| format!("  {s}")));
        }
    }
    out.join("\n")
}

pub(super) fn build_istanbul_table_section(
    label_0: &str,
    label_1: &str,
    out_ts: &str,
    out_rs: &str,
) -> String {
    let ts_blocks = find_istanbul_pipe_table_blocks(out_ts);
    let rs_blocks = find_istanbul_pipe_table_blocks(out_rs);
    if ts_blocks.is_empty() && rs_blocks.is_empty() {
        return String::new();
    }
    let mut out: Vec<String> = vec![];
    out.push(format!(
        "Istanbul pipe tables: {label_0}={} {label_1}={}",
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
            "- table[{index}] lines: {label_0}={} {label_1}={} first_mismatch_line={}",
            ts.len(),
            rs.len(),
            first.map(|n| n + 1).unwrap_or(0)
        ));
        if let Some(first) = first {
            out.push(format!(
                "  {label_0}: {}",
                ts.get(first).map(String::as_str).unwrap_or("<missing>")
            ));
            out.push(format!(
                "  {label_1}: {}",
                rs.get(first).map(String::as_str).unwrap_or("<missing>")
            ));
        }
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
            notes.push(format!("extra blank filler row in side_0 at row {}", i + 1));
            i += 1;
            continue;
        }
        if rs_norm.get(j).is_some_and(|k| k.kind == BoxRowKind::Blank)
            && rs_norm
                .get(j + 1)
                .is_some_and(|next| next == ts_norm.get(i).unwrap())
        {
            notes.push(format!("extra blank filler row in side_1 at row {}", j + 1));
            j += 1;
            continue;
        }
        notes.push(format!("row mismatch side_0#{} vs side_1#{}", i + 1, j + 1));
        break;
    }
    if i < ts_norm.len() && j == rs_norm.len() {
        notes.push(format!(
            "side_0 has {} extra trailing rows",
            ts_norm.len() - i
        ));
    }
    if j < rs_norm.len() && i == ts_norm.len() {
        notes.push(format!(
            "side_1 has {} extra trailing rows",
            rs_norm.len() - j
        ));
    }
    notes
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
