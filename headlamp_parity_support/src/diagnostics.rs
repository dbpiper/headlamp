use serde::Serialize;
use std::path::Path;

use crate::{cluster, normalize, parity_meta, token_ast};

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticsBundle {
    pub schema_version: u32,
    pub case: String,
    pub repo: String,
    pub sides: Vec<RunSideDiagnostics>,
    pub artifacts: ArtifactPaths,
    pub clusters: Vec<ClusterDiagnostics>,
    pub pivot: PivotDiagnostics,
    pub reruns: Vec<RerunDiagnostics>,
    pub recommendation: Recommendation,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunSideDiagnostics {
    pub label: parity_meta::ParitySideLabel,
    pub exit: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactPaths {
    pub sides: Vec<SideArtifactPaths>,
    pub diffs: Vec<String>,
    pub report: String,
    pub meta: String,
    pub analysis: String,
    pub reruns_dir: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SideArtifactPaths {
    pub label: parity_meta::ParitySideLabel,
    pub normalized: String,
    pub raw: String,
    pub tokens: String,
    pub ast: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterDiagnostics {
    pub normalized_hash: u64,
    pub side_indices: Vec<usize>,
    pub labels: Vec<parity_meta::ParitySideLabel>,
    pub exits: Vec<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PivotDiagnostics {
    pub side_index: usize,
    pub label: parity_meta::ParitySideLabel,
}

#[derive(Debug, Clone, Serialize)]
pub struct RerunDiagnostics {
    pub variant: String,
    pub sides: Vec<RerunSideDiagnostics>,
    pub clusters: Vec<ClusterDiagnostics>,
    pub pivot: PivotDiagnostics,
    pub normalized_equal: bool,
    pub score: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RerunSideDiagnostics {
    pub label: parity_meta::ParitySideLabel,
    pub exit: i32,
    pub path: String,
    pub normalized_bytes: usize,
    pub token_stats_norm: token_ast::TokenStats,
}

#[derive(Debug, Clone)]
struct NormalizedRerunSide {
    compare_side: parity_meta::ParityCompareSideInput,
    path: String,
    token_stats_norm: token_ast::TokenStats,
}

#[derive(Debug, Clone, Serialize)]
pub struct Recommendation {
    pub best_variant: Option<String>,
    pub rationale: String,
}

pub fn build_bundle(
    repo: &Path,
    case: &str,
    artifacts: ArtifactPaths,
    compare: &parity_meta::ParityCompareInput,
    reruns: &[crate::RerunMeta],
) -> DiagnosticsBundle {
    let clusters = cluster::cluster_indices_by_normalized(compare);
    let pivot_index = cluster::pick_pivot_index(compare);

    let cluster_diagnostics = clusters_to_diagnostics(compare, &clusters);

    let rerun_diagnostics = reruns
        .iter()
        .filter_map(|rerun| build_rerun(repo, compare, rerun))
        .collect::<Vec<_>>();
    let recommendation = recommend_variant(&rerun_diagnostics);

    DiagnosticsBundle {
        schema_version: 4,
        case: case.to_string(),
        repo: repo.to_string_lossy().to_string(),
        sides: compare
            .sides
            .iter()
            .map(|side| RunSideDiagnostics {
                label: side.label.clone(),
                exit: side.exit,
            })
            .collect(),
        artifacts,
        clusters: cluster_diagnostics,
        pivot: PivotDiagnostics {
            side_index: pivot_index,
            label: compare.sides[pivot_index].label.clone(),
        },
        reruns: rerun_diagnostics,
        recommendation,
    }
}

fn clusters_to_diagnostics(
    compare: &parity_meta::ParityCompareInput,
    clusters: &[cluster::OutputCluster],
) -> Vec<ClusterDiagnostics> {
    clusters
        .iter()
        .map(|cluster| ClusterDiagnostics {
            normalized_hash: cluster.normalized_hash,
            side_indices: cluster.member_indices.clone(),
            labels: cluster
                .member_indices
                .iter()
                .map(|&i| compare.sides[i].label.clone())
                .collect(),
            exits: cluster
                .member_indices
                .iter()
                .map(|&i| compare.sides[i].exit)
                .collect(),
        })
        .collect::<Vec<_>>()
}

fn build_rerun(
    repo: &Path,
    baseline: &parity_meta::ParityCompareInput,
    rerun: &crate::RerunMeta,
) -> Option<RerunDiagnostics> {
    if baseline.sides.len() != rerun.sides.len() {
        return None;
    }

    let normalized_sides = normalize_rerun_sides(repo, baseline, rerun);
    let compare = compare_input_from_normalized_sides(&normalized_sides);
    let clusters = cluster::cluster_indices_by_normalized(&compare);
    let pivot_index = cluster::pick_pivot_index(&compare);
    let all_exits_equal = all_exits_equal(&compare);
    let normalized_equal = clusters.len() == 1 && all_exits_equal;
    let score = score_variant(&clusters, all_exits_equal);

    Some(RerunDiagnostics {
        variant: rerun.variant.clone(),
        sides: normalized_sides
            .into_iter()
            .map(rerun_side_diagnostics)
            .collect(),
        clusters: clusters_to_diagnostics(&compare, &clusters),
        pivot: PivotDiagnostics {
            side_index: pivot_index,
            label: compare.sides[pivot_index].label.clone(),
        },
        normalized_equal,
        score,
    })
}

fn normalize_rerun_sides(
    repo: &Path,
    baseline: &parity_meta::ParityCompareInput,
    rerun: &crate::RerunMeta,
) -> Vec<NormalizedRerunSide> {
    rerun
        .sides
        .iter()
        .enumerate()
        .map(|(index, side)| normalize_rerun_side(repo, baseline, index, side))
        .collect()
}

fn normalize_rerun_side(
    repo: &Path,
    baseline: &parity_meta::ParityCompareInput,
    index: usize,
    side: &crate::RerunSideMeta,
) -> NormalizedRerunSide {
    let normalizer = baseline.sides[index].meta.normalization.normalizer;
    let raw = std::fs::read_to_string(&side.path).unwrap_or_default();
    let normalized = normalize_text(repo, normalizer, raw);
    let token_stats_norm = token_ast::build_token_stream(&normalized).stats;
    let compare_side = parity_meta::ParityCompareSideInput {
        label: side.label.clone(),
        exit: side.code,
        raw: String::new(),
        normalized,
        meta: baseline.sides[index].meta.clone(),
    };
    NormalizedRerunSide {
        compare_side,
        path: side.path.clone(),
        token_stats_norm,
    }
}

fn normalize_text(repo: &Path, normalizer: parity_meta::NormalizerKind, raw: String) -> String {
    match normalizer {
        parity_meta::NormalizerKind::NonTty => normalize::normalize(raw, repo),
        parity_meta::NormalizerKind::TtyUi => normalize::normalize_tty_ui(raw, repo),
    }
}

fn compare_input_from_normalized_sides(
    normalized_sides: &[NormalizedRerunSide],
) -> parity_meta::ParityCompareInput {
    parity_meta::ParityCompareInput {
        sides: normalized_sides
            .iter()
            .map(|s| s.compare_side.clone())
            .collect(),
    }
}

fn all_exits_equal(compare: &parity_meta::ParityCompareInput) -> bool {
    compare
        .sides
        .first()
        .map(|first| compare.sides.iter().all(|side| side.exit == first.exit))
        .unwrap_or(true)
}

fn rerun_side_diagnostics(side: NormalizedRerunSide) -> RerunSideDiagnostics {
    let NormalizedRerunSide {
        compare_side,
        path,
        token_stats_norm,
    } = side;
    RerunSideDiagnostics {
        label: compare_side.label,
        exit: compare_side.exit,
        path,
        normalized_bytes: compare_side.normalized.len(),
        token_stats_norm,
    }
}

fn score_variant(clusters: &[cluster::OutputCluster], all_exits_equal: bool) -> u64 {
    let exit_penalty = if all_exits_equal { 0 } else { 1_000_000 };
    let cluster_penalty = clusters.len().saturating_sub(1) as u64;
    exit_penalty + cluster_penalty
}

fn recommend_variant(reruns: &[RerunDiagnostics]) -> Recommendation {
    let best = reruns.iter().min_by(|a, b| {
        a.score
            .cmp(&b.score)
            .then_with(|| a.variant.cmp(&b.variant))
    });
    let rationale = match best {
        Some(best) => format!(
            "picked {} (score={}) among {} variants",
            best.variant,
            best.score,
            reruns.len()
        ),
        None => "no reruns available".to_string(),
    };
    Recommendation {
        best_variant: best.map(|b| b.variant.clone()),
        rationale,
    }
}
