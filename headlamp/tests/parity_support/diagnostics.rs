use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

use super::{normalize, parity_meta, token_ast};

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticsBundle {
    pub schema_version: u32,
    pub case: String,
    pub repo: String,
    pub ts_exit: i32,
    pub rs_exit: i32,
    pub artifacts: ArtifactPaths,
    pub baseline: BaselineDiagnostics,
    pub mismatch: MismatchSummary,
    pub reruns: Vec<RerunDiagnostics>,
    pub recommendation: Recommendation,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactPaths {
    pub normalized_ts: String,
    pub normalized_rs: String,
    pub raw_ts: String,
    pub raw_rs: String,
    pub diff: String,
    pub report: String,
    pub meta: String,
    pub analysis: String,
    pub tokens_ts: String,
    pub tokens_rs: String,
    pub ast_ts: String,
    pub ast_rs: String,
    pub reruns_dir: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BaselineDiagnostics {
    pub normalizer: parity_meta::NormalizerKind,
    pub ts: SideDiagnostics,
    pub rs: SideDiagnostics,
    pub block_moves: Vec<BlockMove>,
    pub missing_in_rs: Vec<String>,
    pub missing_in_ts: Vec<String>,
    pub token_delta_norm: BTreeMap<String, (usize, usize)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SideDiagnostics {
    pub raw_bytes: usize,
    pub raw_lines: usize,
    pub normalized_bytes: usize,
    pub normalized_lines: usize,
    pub token_stats_raw: token_ast::TokenStats,
    pub token_stats_norm: token_ast::TokenStats,
    pub blocks: Vec<BlockSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlockSummary {
    pub index: usize,
    pub kind: token_ast::BlockKind,
    pub hash: String,
    pub line_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlockMove {
    pub hash: String,
    pub ts_index: usize,
    pub rs_index: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RerunDiagnostics {
    pub variant: String,
    pub ts_exit: i32,
    pub rs_exit: i32,
    pub ts_path: String,
    pub rs_path: String,
    pub ts_normalized_bytes: usize,
    pub rs_normalized_bytes: usize,
    pub ts_token_stats_norm: token_ast::TokenStats,
    pub rs_token_stats_norm: token_ast::TokenStats,
    pub ts_blocks: Vec<BlockSummary>,
    pub rs_blocks: Vec<BlockSummary>,
    pub block_moves: Vec<BlockMove>,
    pub missing_in_rs: Vec<String>,
    pub missing_in_ts: Vec<String>,
    pub token_delta_norm: BTreeMap<String, (usize, usize)>,
    pub mismatch: MismatchSummary,
    pub normalized_equal: bool,
    pub score: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Recommendation {
    pub best_variant: Option<String>,
    pub rationale: String,
}

pub fn build_bundle(
    repo: &Path,
    case: &str,
    ts_exit: i32,
    rs_exit: i32,
    artifacts: ArtifactPaths,
    compare: &parity_meta::ParityCompareInput,
    reruns: &[super::RerunMeta],
) -> DiagnosticsBundle {
    let baseline = build_baseline(compare);
    let mismatch = summarize_mismatch(compare);
    let rerun_diagnostics = reruns
        .iter()
        .map(|rerun| build_rerun(repo, compare, rerun))
        .collect::<Vec<_>>();
    let recommendation = recommend_variant(&rerun_diagnostics);
    DiagnosticsBundle {
        schema_version: 2,
        case: case.to_string(),
        repo: repo.to_string_lossy().to_string(),
        ts_exit,
        rs_exit,
        artifacts,
        baseline,
        mismatch,
        reruns: rerun_diagnostics,
        recommendation,
    }
}

fn build_baseline(compare: &parity_meta::ParityCompareInput) -> BaselineDiagnostics {
    let normalizer = compare.meta.ts.normalization.normalizer;
    let ts = build_side(&compare.raw_ts, &compare.normalized_ts);
    let rs = build_side(&compare.raw_rs, &compare.normalized_rs);
    let (block_moves, missing_in_rs, missing_in_ts) = compute_block_moves(&ts.blocks, &rs.blocks);
    let token_delta_norm = compute_token_delta(&ts.token_stats_norm, &rs.token_stats_norm);
    BaselineDiagnostics {
        normalizer,
        ts,
        rs,
        block_moves,
        missing_in_rs,
        missing_in_ts,
        token_delta_norm,
    }
}

fn build_side(raw: &str, normalized: &str) -> SideDiagnostics {
    let token_stats_raw = token_ast::build_token_stream(raw).stats;
    let token_stats_norm = token_ast::build_token_stream(normalized).stats;
    let blocks = token_ast::build_document_ast(normalized)
        .blocks
        .into_iter()
        .enumerate()
        .map(|(index, block)| BlockSummary {
            index,
            kind: block.kind,
            hash: block.hash,
            line_count: block.line_count,
        })
        .collect::<Vec<_>>();
    SideDiagnostics {
        raw_bytes: raw.as_bytes().len(),
        raw_lines: raw.lines().count(),
        normalized_bytes: normalized.as_bytes().len(),
        normalized_lines: normalized.lines().count(),
        token_stats_raw,
        token_stats_norm,
        blocks,
    }
}

fn compute_block_moves(
    ts_blocks: &[BlockSummary],
    rs_blocks: &[BlockSummary],
) -> (Vec<BlockMove>, Vec<String>, Vec<String>) {
    let ts_pos = ts_blocks
        .iter()
        .map(|b| (b.hash.clone(), b.index))
        .collect::<BTreeMap<_, _>>();
    let rs_pos = rs_blocks
        .iter()
        .map(|b| (b.hash.clone(), b.index))
        .collect::<BTreeMap<_, _>>();
    let block_moves = ts_pos
        .iter()
        .filter_map(|(hash, ts_index)| {
            rs_pos
                .get(hash)
                .map(|rs_index| BlockMove {
                    hash: hash.clone(),
                    ts_index: *ts_index,
                    rs_index: *rs_index,
                })
        })
        .filter(|m| m.ts_index != m.rs_index)
        .collect::<Vec<_>>();
    let missing_in_rs = ts_pos
        .keys()
        .filter(|hash| !rs_pos.contains_key(*hash))
        .cloned()
        .collect::<Vec<_>>();
    let missing_in_ts = rs_pos
        .keys()
        .filter(|hash| !ts_pos.contains_key(*hash))
        .cloned()
        .collect::<Vec<_>>();
    (block_moves, missing_in_rs, missing_in_ts)
}

fn compute_token_delta(
    ts: &token_ast::TokenStats,
    rs: &token_ast::TokenStats,
) -> BTreeMap<String, (usize, usize)> {
    let mut keys = ts
        .counts_by_kind
        .keys()
        .chain(rs.counts_by_kind.keys())
        .map(|k| format!("{k:?}"))
        .collect::<Vec<_>>();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .map(|k| {
            let ts_count = ts
                .counts_by_kind
                .iter()
                .find(|(kind, _)| format!("{kind:?}") == k)
                .map(|(_, v)| *v)
                .unwrap_or(0);
            let rs_count = rs
                .counts_by_kind
                .iter()
                .find(|(kind, _)| format!("{kind:?}") == k)
                .map(|(_, v)| *v)
                .unwrap_or(0);
            (k, (ts_count, rs_count))
        })
        .collect()
}

fn build_rerun(
    repo: &Path,
    baseline: &parity_meta::ParityCompareInput,
    rerun: &super::RerunMeta,
) -> RerunDiagnostics {
    let (ts_norm, ts_stats) = normalize_and_summarize(
        baseline.meta.ts.normalization.normalizer,
        repo,
        &std::fs::read_to_string(&rerun.ts_path).unwrap_or_default(),
    );
    let (rs_norm, rs_stats) = normalize_and_summarize(
        baseline.meta.rs.normalization.normalizer,
        repo,
        &std::fs::read_to_string(&rerun.rs_path).unwrap_or_default(),
    );
    let normalized_equal = ts_norm == rs_norm;
    let ts_blocks = build_side("", &ts_norm).blocks;
    let rs_blocks = build_side("", &rs_norm).blocks;
    let (block_moves, missing_in_rs, missing_in_ts) = compute_block_moves(&ts_blocks, &rs_blocks);
    let token_delta_norm = compute_token_delta(&ts_stats, &rs_stats);
    let mismatch = summarize_mismatch(&parity_meta::ParityCompareInput {
        raw_ts: String::new(),
        raw_rs: String::new(),
        normalized_ts: ts_norm.clone(),
        normalized_rs: rs_norm.clone(),
        meta: baseline.meta.clone(),
    });
    let score = score_variant(
        baseline,
        &ts_stats,
        &rs_stats,
        missing_in_rs.len(),
        missing_in_ts.len(),
        block_moves.len(),
        normalized_equal,
    );
    RerunDiagnostics {
        variant: rerun.variant.clone(),
        ts_exit: rerun.ts_code,
        rs_exit: rerun.rs_code,
        ts_path: rerun.ts_path.clone(),
        rs_path: rerun.rs_path.clone(),
        ts_normalized_bytes: ts_norm.as_bytes().len(),
        rs_normalized_bytes: rs_norm.as_bytes().len(),
        ts_token_stats_norm: ts_stats,
        rs_token_stats_norm: rs_stats,
        ts_blocks,
        rs_blocks,
        block_moves,
        missing_in_rs,
        missing_in_ts,
        token_delta_norm,
        mismatch,
        normalized_equal,
        score,
    }
}

fn normalize_and_summarize(
    kind: parity_meta::NormalizerKind,
    repo: &Path,
    raw: &str,
) -> (String, token_ast::TokenStats) {
    let normalized = match kind {
        parity_meta::NormalizerKind::NonTty => normalize::normalize(raw.to_string(), repo),
        parity_meta::NormalizerKind::TtyUi => normalize::normalize_tty_ui(raw.to_string(), repo),
    };
    let stats = token_ast::build_token_stream(&normalized).stats;
    (normalized, stats)
}

fn score_variant(
    baseline: &parity_meta::ParityCompareInput,
    ts: &token_ast::TokenStats,
    rs: &token_ast::TokenStats,
    missing_in_rs: usize,
    missing_in_ts: usize,
    moved_blocks: usize,
    equal: bool,
) -> u64 {
    if equal && baseline.meta.ts.normalization.used_fallback == false {
        return 0;
    }
    let token_gap = ts
        .token_count
        .abs_diff(rs.token_count)
        .saturating_add(ts.visible_width_total.abs_diff(rs.visible_width_total));
    let structural_penalty = (missing_in_rs as u64)
        .saturating_add(missing_in_ts as u64)
        .saturating_mul(100)
        .saturating_add((moved_blocks as u64).saturating_mul(20));
    (token_gap as u64).saturating_add(structural_penalty)
}

fn recommend_variant(reruns: &[RerunDiagnostics]) -> Recommendation {
    let best = reruns
        .iter()
        .min_by_key(|r| (r.score, !r.normalized_equal, r.variant.as_str()))
        .map(|r| r.variant.clone());
    Recommendation {
        best_variant: best.clone(),
        rationale: best
            .map(|v| format!("lowest_score_variant={v}"))
            .unwrap_or_else(|| "no_reruns_available".to_string()),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MismatchSummary {
    pub classification: Classification,
    pub first_mismatch_line: Option<usize>,
    pub ts_line: String,
    pub rs_line: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Classification {
    pub ansi_only: bool,
    pub whitespace_only: bool,
    pub path_only: bool,
}

fn summarize_mismatch(compare: &parity_meta::ParityCompareInput) -> MismatchSummary {
    let ts = &compare.normalized_ts;
    let rs = &compare.normalized_rs;
    let classification = classify(ts, rs);
    let (first_mismatch_line, ts_line, rs_line) = first_mismatch_line(ts, rs);
    MismatchSummary {
        classification,
        first_mismatch_line,
        ts_line,
        rs_line,
    }
}

fn classify(ts: &str, rs: &str) -> Classification {
    let ts_simple = headlamp_core::format::stacks::strip_ansi_simple(ts);
    let rs_simple = headlamp_core::format::stacks::strip_ansi_simple(rs);
    let ansi_only = ts_simple == rs_simple && ts != rs;
    let whitespace_only = collapse_ws(&ts_simple) == collapse_ws(&rs_simple) && ts_simple != rs_simple;
    let path_only = normalize_paths_minimal(&ts_simple) == normalize_paths_minimal(&rs_simple)
        && ts_simple != rs_simple;
    Classification {
        ansi_only,
        whitespace_only,
        path_only,
    }
}

fn first_mismatch_line(ts: &str, rs: &str) -> (Option<usize>, String, String) {
    let ts_lines = ts.lines().collect::<Vec<_>>();
    let rs_lines = rs.lines().collect::<Vec<_>>();
    let limit = ts_lines.len().min(rs_lines.len());
    let idx = (0..limit).find(|&i| ts_lines[i] != rs_lines[i]);
    match idx {
        None if ts_lines.len() != rs_lines.len() => {
            let i = limit;
            let ts_line = ts_lines.get(i).copied().unwrap_or("").to_string();
            let rs_line = rs_lines.get(i).copied().unwrap_or("").to_string();
            (Some(i + 1), ts_line, rs_line)
        }
        None => (None, String::new(), String::new()),
        Some(i) => (
            Some(i + 1),
            ts_lines.get(i).copied().unwrap_or("").to_string(),
            rs_lines.get(i).copied().unwrap_or("").to_string(),
        ),
    }
}

fn collapse_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_paths_minimal(text: &str) -> String {
    text.replace("/private/var/folders/", "/var/folders/")
        .replace("\\\\", "/")
}

