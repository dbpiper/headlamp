use std::cmp::min;

use super::cluster;
use super::parity_meta::ParityCompareInput;
use super::token_ast;

pub fn build_parity_report_with_meta(compare: &ParityCompareInput) -> String {
    if compare.sides.len() < 2 {
        return "Parity report: only one side".to_string();
    }

    let clusters = cluster::cluster_indices_by_normalized(compare);
    let pivot_index = cluster::pick_pivot_index(compare);
    let pivot_label = compare.sides[pivot_index].label.display_label();

    let mut sections: Vec<String> = vec![];
    sections.push(build_artifact_summary(compare));
    sections.push(build_cluster_summary(compare, pivot_index, &clusters));
    sections.push(build_token_ast_summary(compare, pivot_index, &clusters));
    sections.push(build_block_order_summary(compare, pivot_index, &clusters));

    let pivot_norm = &compare.sides[pivot_index].normalized;

    clusters
        .iter()
        .filter(|cluster| !cluster.member_indices.contains(&pivot_index))
        .filter_map(|cluster| {
            cluster.member_indices.iter().copied().min_by(|&a, &b| {
                compare.sides[a]
                    .label
                    .display_label()
                    .cmp(&compare.sides[b].label.display_label())
            })
        })
        .for_each(|other_index| {
            let other_label = compare.sides[other_index].label.display_label();
            let other_norm = &compare.sides[other_index].normalized;

            sections.push(format!("Comparison: {pivot_label} vs {other_label}"));
            sections.push(build_classification_section(pivot_norm, other_norm));
            sections.push(build_first_mismatch_section(
                &pivot_label,
                &other_label,
                pivot_norm,
                other_norm,
            ));
            sections.push(build_blank_runs_section(
                &pivot_label,
                &other_label,
                pivot_norm,
                other_norm,
            ));
            sections.push(build_table_section(
                &pivot_label,
                &other_label,
                pivot_norm,
                other_norm,
            ));
            sections.push(build_istanbul_table_section(
                &pivot_label,
                &other_label,
                pivot_norm,
                other_norm,
            ));
            sections.push(build_counts_section(
                &pivot_label,
                &other_label,
                pivot_norm,
                other_norm,
            ));
        });

    sections
        .into_iter()
        .filter(|section| !section.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn build_cluster_summary(
    compare: &ParityCompareInput,
    pivot_index: usize,
    clusters: &[cluster::OutputCluster],
) -> String {
    let mut lines: Vec<String> = vec![
        "Clusters".to_string(),
        format!(
            "- pivot: {}",
            compare.sides[pivot_index].label.display_label()
        ),
        format!("- cluster_count: {}", clusters.len()),
    ];
    clusters
        .iter()
        .enumerate()
        .for_each(|(cluster_index, cluster)| {
            let member_labels = cluster
                .member_indices
                .iter()
                .map(|&i| compare.sides[i].label.display_label())
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!(
                "- cluster_{cluster_index}: size={} [{member_labels}]",
                cluster.member_indices.len()
            ));
        });
    lines.join("\n")
}

fn build_token_ast_summary(
    compare: &ParityCompareInput,
    pivot_index: usize,
    clusters: &[cluster::OutputCluster],
) -> String {
    let mut lines: Vec<String> = vec!["Token stats".to_string()];
    compare.sides.iter().for_each(|side| {
        let raw = token_ast::build_token_stream(&side.raw);
        let norm = token_ast::build_token_stream(&side.normalized);
        lines.extend(render_token_stats_side(
            &format!("{}_raw", side.label.display_label()),
            &raw.stats,
        ));
        lines.extend(render_token_stats_side(
            &format!("{}_norm", side.label.display_label()),
            &norm.stats,
        ));
    });

    let pivot_norm = token_ast::build_token_stream(&compare.sides[pivot_index].normalized).stats;
    let pivot_label = compare.sides[pivot_index].label.display_label();

    clusters
        .iter()
        .filter(|cluster| !cluster.member_indices.contains(&pivot_index))
        .filter_map(|cluster| {
            cluster.member_indices.iter().copied().min_by(|&a, &b| {
                compare.sides[a]
                    .label
                    .display_label()
                    .cmp(&compare.sides[b].label.display_label())
            })
        })
        .for_each(|other_index| {
            let other_label = compare.sides[other_index].label.display_label();
            let other_norm =
                token_ast::build_token_stream(&compare.sides[other_index].normalized).stats;
            lines.extend(render_token_delta(
                &pivot_label,
                &other_label,
                &pivot_norm,
                &other_norm,
            ));
        });

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

fn render_token_delta(
    label_a: &str,
    label_b: &str,
    a: &token_ast::TokenStats,
    b: &token_ast::TokenStats,
) -> Vec<String> {
    let mut kinds = a
        .counts_by_kind
        .keys()
        .chain(b.counts_by_kind.keys())
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    kinds.sort();
    let by_kind = kinds
        .into_iter()
        .map(|k| {
            let a_count = a.counts_by_kind.get(&k).copied().unwrap_or(0);
            let b_count = b.counts_by_kind.get(&k).copied().unwrap_or(0);
            format!("{k:?}:{a_count}->{b_count}")
        })
        .collect::<Vec<_>>()
        .join(" ");
    vec![format!(
        "- norm_token_delta: {label_a} -> {label_b}: {by_kind}"
    )]
}

fn build_block_order_summary(
    compare: &ParityCompareInput,
    pivot_index: usize,
    clusters: &[cluster::OutputCluster],
) -> String {
    let orders = compare
        .sides
        .iter()
        .map(|side| {
            token_ast::build_document_ast(&side.normalized)
                .blocks
                .into_iter()
                .map(|block| block.hash)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let mut lines: Vec<String> = vec!["Block order".to_string()];
    compare.sides.iter().enumerate().for_each(|(index, side)| {
        lines.push(format!(
            "- {}: [{}]",
            side.label.display_label(),
            orders[index].join(",")
        ));
    });

    let pivot_label = compare.sides[pivot_index].label.display_label();
    let pivot_order = orders[pivot_index]
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();

    clusters
        .iter()
        .filter(|cluster| !cluster.member_indices.contains(&pivot_index))
        .filter_map(|cluster| {
            cluster.member_indices.iter().copied().min_by(|&a, &b| {
                compare.sides[a]
                    .label
                    .display_label()
                    .cmp(&compare.sides[b].label.display_label())
            })
        })
        .for_each(|other_index| {
            let other_label = compare.sides[other_index].label.display_label();
            let other_order = orders[other_index]
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            lines.push(format!("Block moves: {pivot_label} vs {other_label}"));
            lines.extend(render_block_moves_pair(
                &pivot_label,
                &other_label,
                &pivot_order,
                &other_order,
            ));
        });

    lines.join("\n")
}

fn render_block_moves_pair(
    label_a: &str,
    label_b: &str,
    order_a: &[&str],
    order_b: &[&str],
) -> Vec<String> {
    let pos_a = order_a
        .iter()
        .enumerate()
        .map(|(index, hash)| (hash.to_string(), index))
        .collect::<std::collections::BTreeMap<String, usize>>();
    let pos_b = order_b
        .iter()
        .enumerate()
        .map(|(index, hash)| (hash.to_string(), index))
        .collect::<std::collections::BTreeMap<String, usize>>();
    let moved = pos_a
        .iter()
        .filter_map(|(hash, a_index)| {
            pos_b
                .get(hash)
                .map(|b_index| (hash.as_str(), *a_index, *b_index))
        })
        .filter(|(_, a_index, b_index)| a_index != b_index)
        .take(12)
        .map(|(hash, a_index, b_index)| {
            format!("  - moved: {hash} {label_a}={a_index} {label_b}={b_index}")
        })
        .collect::<Vec<_>>();
    let missing_in_b = pos_a
        .keys()
        .filter(|hash| !pos_b.contains_key(*hash))
        .take(12)
        .map(|hash| format!("  - missing_in: {label_b}: {hash}"))
        .collect::<Vec<_>>();
    let missing_in_a = pos_b
        .keys()
        .filter(|hash| !pos_a.contains_key(*hash))
        .take(12)
        .map(|hash| format!("  - missing_in: {label_a}: {hash}"))
        .collect::<Vec<_>>();
    [moved, missing_in_b, missing_in_a]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
}

pub fn build_parity_report(out_ts: &str, out_rs: &str) -> String {
    build_parity_report_with_meta(&ParityCompareInput {
        sides: vec![
            super::parity_meta::ParityCompareSideInput {
                label: super::parity_meta::ParitySideLabel {
                    binary: "unknown".to_string(),
                    runner_stack: "unknown".to_string(),
                },
                exit: 0,
                raw: out_ts.to_string(),
                normalized: out_ts.to_string(),
                meta: super::parity_meta::ParitySideMeta {
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
            },
            super::parity_meta::ParityCompareSideInput {
                label: super::parity_meta::ParitySideLabel {
                    binary: "unknown".to_string(),
                    runner_stack: "unknown".to_string(),
                },
                exit: 0,
                raw: out_rs.to_string(),
                normalized: out_rs.to_string(),
                meta: super::parity_meta::ParitySideMeta {
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
        ],
    })
}

fn build_artifact_summary(compare: &ParityCompareInput) -> String {
    let mut lines: Vec<String> = vec!["Artifact summary".to_string()];
    compare.sides.iter().for_each(|side| {
        let normalized_empty_but_raw_nonempty =
            side.meta.normalized_bytes == 0 && side.meta.raw_bytes > 0;
        lines.push(format!(
            "- {}: exit={} raw_bytes={} raw_lines={} normalized_bytes={} normalized_lines={} normalizer={:?} fallback={} normalized_empty_but_raw_nonempty={}",
            side.label.display_label(),
            side.exit,
            side.meta.raw_bytes,
            side.meta.raw_lines,
            side.meta.normalized_bytes,
            side.meta.normalized_lines,
            side.meta.normalization.normalizer,
            side.meta.normalization.used_fallback,
            normalized_empty_but_raw_nonempty
        ));
    });
    lines.join("\n")
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

fn build_first_mismatch_section(label_0: &str, label_1: &str, out_0: &str, out_1: &str) -> String {
    let side_0_lines = out_0.lines().collect::<Vec<_>>();
    let side_1_lines = out_1.lines().collect::<Vec<_>>();
    let max_len = side_0_lines.len().max(side_1_lines.len());
    let first = (0..max_len).find(|&i| side_0_lines.get(i) != side_1_lines.get(i));
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
        let side_0 = side_0_lines.get(i).copied().unwrap_or("<missing>");
        let side_1 = side_1_lines.get(i).copied().unwrap_or("<missing>");
        if side_0 == side_1 {
            out.push(format!(" {ln:>4}  =  {side_0}", ln = i + 1));
            continue;
        }
        out.push(format!(" {ln:>4} {label_0} {side_0}", ln = i + 1));
        out.push(format!(" {ln:>4} {label_1} {side_1}", ln = i + 1));
        out.push(format!(
            "      {label_0} len={} vis={} | {label_1} len={} vis={}",
            side_0.chars().count(),
            strip_ansi(side_0).chars().count(),
            side_1.chars().count(),
            strip_ansi(side_1).chars().count()
        ));
    }
    out.join("\n")
}

fn build_table_section(label_0: &str, label_1: &str, out_ts: &str, out_rs: &str) -> String {
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

fn build_istanbul_table_section(
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

fn build_counts_section(label_0: &str, label_1: &str, out_ts: &str, out_rs: &str) -> String {
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
            out.push(format!("- '{needle}': {label_0}={c_ts} {label_1}={c_rs}"));
        }
    });
    if out.len() == 1 {
        String::new()
    } else {
        out.join("\n")
    }
}

fn build_blank_runs_section(label_0: &str, label_1: &str, out_ts: &str, out_rs: &str) -> String {
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
            "- {label_0}: blank_lines={} runs={} max_run={}",
            ts_stats.blank_lines, ts_stats.runs, ts_stats.max_run
        ),
        format!(
            "- {label_1}: blank_lines={} runs={} max_run={}",
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
