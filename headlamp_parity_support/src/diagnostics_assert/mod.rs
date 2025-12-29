use std::path::Path;

use serde::Serialize;

use crate::parity_meta::{ParityCompareInput, ParitySideLabel};
use crate::types::ParityRunGroup;

#[derive(Debug, Clone, Serialize)]
pub struct RerunMeta {
    pub variant: String,
    pub sides: Vec<RerunSideMeta>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RerunSideMeta {
    pub label: ParitySideLabel,
    pub code: i32,
    pub path: String,
    pub bytes: usize,
    pub tokens: crate::token_ast::TokenStats,
    pub blocks: usize,
}

pub fn assert_parity_normalized_outputs(
    repo: &Path,
    case: &str,
    side_0_exit: i32,
    side_0_out: &str,
    side_1_exit: i32,
    side_1_out: &str,
) {
    let compare = ParityCompareInput {
        sides: vec![
            crate::parity_meta::ParityCompareSideInput {
                label: ParitySideLabel {
                    binary: "side_0".to_string(),
                    runner_stack: "normalized".to_string(),
                },
                exit: side_0_exit,
                raw: side_0_out.to_string(),
                normalized: side_0_out.to_string(),
                meta: mk_side_meta(crate::parity_meta::NormalizerKind::NonTty, side_0_out),
            },
            crate::parity_meta::ParityCompareSideInput {
                label: ParitySideLabel {
                    binary: "side_1".to_string(),
                    runner_stack: "normalized".to_string(),
                },
                exit: side_1_exit,
                raw: side_1_out.to_string(),
                normalized: side_1_out.to_string(),
                meta: mk_side_meta(crate::parity_meta::NormalizerKind::NonTty, side_1_out),
            },
        ],
    };
    assert_parity_with_diagnostics(repo, case, &compare, None);
}

pub fn assert_parity_non_tty_with_diagnostics(
    repo: &Path,
    case: &str,
    side_0_exit: i32,
    side_0_raw: String,
    side_1_exit: i32,
    side_1_raw: String,
    run_group: Option<&ParityRunGroup>,
) {
    let side_0_norm = crate::normalize::normalize(side_0_raw.clone(), repo);
    let side_1_norm = crate::normalize::normalize(side_1_raw.clone(), repo);
    let compare = ParityCompareInput {
        sides: vec![
            crate::parity_meta::ParityCompareSideInput {
                label: ParitySideLabel {
                    binary: "side_0".to_string(),
                    runner_stack: "non_tty".to_string(),
                },
                exit: side_0_exit,
                raw: side_0_raw,
                normalized: side_0_norm.clone(),
                meta: mk_side_meta(crate::parity_meta::NormalizerKind::NonTty, &side_0_norm),
            },
            crate::parity_meta::ParityCompareSideInput {
                label: ParitySideLabel {
                    binary: "side_1".to_string(),
                    runner_stack: "non_tty".to_string(),
                },
                exit: side_1_exit,
                raw: side_1_raw,
                normalized: side_1_norm.clone(),
                meta: mk_side_meta(crate::parity_meta::NormalizerKind::NonTty, &side_1_norm),
            },
        ],
    };
    assert_parity_with_diagnostics(repo, case, &compare, run_group);
}

pub fn assert_parity_tty_ui_with_diagnostics(
    repo: &Path,
    case: &str,
    side_0_exit: i32,
    side_0_raw: String,
    side_1_exit: i32,
    side_1_raw: String,
    run_group: Option<&ParityRunGroup>,
) {
    let side_0_norm = crate::normalize::normalize_tty_ui(side_0_raw.clone(), repo);
    let side_1_norm = crate::normalize::normalize_tty_ui(side_1_raw.clone(), repo);
    let compare = ParityCompareInput {
        sides: vec![
            crate::parity_meta::ParityCompareSideInput {
                label: ParitySideLabel {
                    binary: "side_0".to_string(),
                    runner_stack: "tty_ui".to_string(),
                },
                exit: side_0_exit,
                raw: side_0_raw,
                normalized: side_0_norm.clone(),
                meta: mk_side_meta(crate::parity_meta::NormalizerKind::TtyUi, &side_0_norm),
            },
            crate::parity_meta::ParityCompareSideInput {
                label: ParitySideLabel {
                    binary: "side_1".to_string(),
                    runner_stack: "tty_ui".to_string(),
                },
                exit: side_1_exit,
                raw: side_1_raw,
                normalized: side_1_norm.clone(),
                meta: mk_side_meta(crate::parity_meta::NormalizerKind::TtyUi, &side_1_norm),
            },
        ],
    };
    assert_parity_with_diagnostics(repo, case, &compare, run_group);
}

pub fn assert_parity_with_diagnostics(
    repo: &Path,
    case: &str,
    compare: &ParityCompareInput,
    _run_group: Option<&ParityRunGroup>,
) {
    if compare.sides.len() < 2 || parity_matches(compare) {
        return;
    }

    let safe = safe_case(case);
    let dump_dir = dump_dir_for_repo(repo);
    let _ = std::fs::create_dir_all(&dump_dir);

    let report_path = dump_dir.join(format!("{safe}--report.txt"));
    let analysis_path = dump_dir.join(format!("{safe}--analysis.json"));
    let meta_path = dump_dir.join(format!("{safe}--meta.json"));

    let side_dump_paths = build_side_dump_paths(&dump_dir, &safe, compare);
    write_side_dumps(compare, &side_dump_paths);
    let diff_paths = write_diffs(compare, &dump_dir, &safe);
    let artifacts = build_artifacts(
        compare,
        &side_dump_paths,
        diff_paths,
        &report_path,
        &meta_path,
        &analysis_path,
    );
    let bundle = crate::diagnostics::build_bundle(repo, case, artifacts, compare, &[]);
    if let Ok(mut file) = std::fs::File::create(&analysis_path) {
        let _ = serde_json::to_writer_pretty(&mut file, &bundle);
    }
    let _ = std::fs::write(
        &meta_path, "{}\n", // keep a stable placeholder for tooling
    );

    let report = crate::diff_report::build_parity_report_with_meta(compare);
    let _ = std::fs::write(&report_path, &report);

    panic!(
        "parity mismatch case={case}\nreport_path={}\nanalysis_path={}\n{}",
        report_path.display(),
        analysis_path.display(),
        truncate_report_for_panic(&report)
    );
}

#[derive(Debug, Clone)]
struct SideDumpPaths {
    normalized: std::path::PathBuf,
    raw: std::path::PathBuf,
    tokens: std::path::PathBuf,
    ast: std::path::PathBuf,
}

fn parity_matches(compare: &ParityCompareInput) -> bool {
    let Some(first) = compare.sides.first() else {
        return true;
    };
    let all_exits_equal = compare.sides.iter().all(|side| side.exit == first.exit);
    let all_normalized_equal = compare
        .sides
        .iter()
        .all(|side| side.normalized == first.normalized);
    all_exits_equal && all_normalized_equal
}

fn dump_dir_for_repo(repo: &Path) -> std::path::PathBuf {
    let repo_key = repo.file_name().unwrap_or_default();
    std::env::temp_dir()
        .join("headlamp-parity-dumps")
        .join(repo_key)
}

fn build_side_dump_paths(
    dump_dir: &Path,
    safe: &str,
    compare: &ParityCompareInput,
) -> Vec<SideDumpPaths> {
    compare
        .sides
        .iter()
        .map(|side| {
            let side_key = side.label.file_safe_label();
            SideDumpPaths {
                normalized: dump_dir.join(format!("{safe}--{side_key}--normalized.txt")),
                raw: dump_dir.join(format!("{safe}--{side_key}--raw.txt")),
                tokens: dump_dir.join(format!("{safe}--{side_key}--tokens.json")),
                ast: dump_dir.join(format!("{safe}--{side_key}--ast.json")),
            }
        })
        .collect::<Vec<_>>()
}

fn write_side_dumps(compare: &ParityCompareInput, side_dump_paths: &[SideDumpPaths]) {
    compare
        .sides
        .iter()
        .zip(side_dump_paths.iter())
        .for_each(|(side, paths)| write_side_dump(side, paths));
}

fn write_side_dump(side: &crate::parity_meta::ParityCompareSideInput, paths: &SideDumpPaths) {
    let _ = std::fs::write(&paths.normalized, &side.normalized);
    let _ = std::fs::write(&paths.raw, &side.raw);
    let raw_tokens = crate::token_ast::build_token_stream(&side.raw);
    let norm_tokens = crate::token_ast::build_token_stream(&side.normalized);
    let doc_ast = crate::token_ast::build_document_ast(&side.normalized);
    let _ = std::fs::File::create(&paths.tokens)
        .ok()
        .and_then(|mut file| {
            serde_json::to_writer_pretty(&mut file, &(raw_tokens, norm_tokens)).ok()
        });
    let _ = std::fs::File::create(&paths.ast)
        .ok()
        .and_then(|mut file| serde_json::to_writer_pretty(&mut file, &doc_ast).ok());
}

fn write_diffs(compare: &ParityCompareInput, dump_dir: &Path, safe: &str) -> Vec<String> {
    let pivot_index = crate::cluster::pick_pivot_index(compare);
    crate::cluster::cluster_indices_by_normalized(compare)
        .iter()
        .filter(|cluster| !cluster.member_indices.contains(&pivot_index))
        .filter_map(|cluster| pick_min_label_index(compare, cluster))
        .map(|other_index| write_diff(compare, dump_dir, safe, pivot_index, other_index))
        .collect::<Vec<_>>()
}

fn pick_min_label_index(
    compare: &ParityCompareInput,
    cluster: &crate::cluster::OutputCluster,
) -> Option<usize> {
    cluster.member_indices.iter().copied().min_by(|&a, &b| {
        compare.sides[a]
            .label
            .display_label()
            .cmp(&compare.sides[b].label.display_label())
    })
}

fn write_diff(
    compare: &ParityCompareInput,
    dump_dir: &Path,
    safe: &str,
    pivot_index: usize,
    other_index: usize,
) -> String {
    let pivot_key = compare.sides[pivot_index].label.file_safe_label();
    let other_key = compare.sides[other_index].label.file_safe_label();
    let diff_path = dump_dir.join(format!("{safe}--diff--{pivot_key}--vs--{other_key}.txt"));
    let diff = similar_asserts::SimpleDiff::from_str(
        &compare.sides[pivot_index].normalized,
        &compare.sides[other_index].normalized,
        &compare.sides[pivot_index].label.display_label(),
        &compare.sides[other_index].label.display_label(),
    )
    .to_string();
    let _ = std::fs::write(&diff_path, &diff);
    diff_path.to_string_lossy().to_string()
}

fn build_artifacts(
    compare: &ParityCompareInput,
    side_dump_paths: &[SideDumpPaths],
    diff_paths: Vec<String>,
    report_path: &Path,
    meta_path: &Path,
    analysis_path: &Path,
) -> crate::diagnostics::ArtifactPaths {
    crate::diagnostics::ArtifactPaths {
        sides: compare
            .sides
            .iter()
            .zip(side_dump_paths.iter())
            .map(|(side, paths)| crate::diagnostics::SideArtifactPaths {
                label: side.label.clone(),
                normalized: paths.normalized.to_string_lossy().to_string(),
                raw: paths.raw.to_string_lossy().to_string(),
                tokens: paths.tokens.to_string_lossy().to_string(),
                ast: paths.ast.to_string_lossy().to_string(),
            })
            .collect(),
        diffs: diff_paths,
        report: report_path.to_string_lossy().to_string(),
        meta: meta_path.to_string_lossy().to_string(),
        analysis: analysis_path.to_string_lossy().to_string(),
        reruns_dir: String::new(),
    }
}

fn mk_side_meta(
    kind: crate::parity_meta::NormalizerKind,
    normalized: &str,
) -> crate::parity_meta::ParitySideMeta {
    let raw_bytes = normalized.len();
    let raw_lines = normalized.lines().count();
    crate::parity_meta::ParitySideMeta {
        raw_bytes,
        raw_lines,
        normalized_bytes: raw_bytes,
        normalized_lines: raw_lines,
        normalization: crate::parity_meta::NormalizationMeta {
            normalizer: kind,
            used_fallback: false,
            last_failed_tests_line: None,
            last_test_files_line: None,
            last_box_table_top_line: None,
            stages: vec![],
        },
    }
}

fn safe_case(case: &str) -> String {
    case.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn truncate_report_for_panic(report: &str) -> String {
    const MAX: usize = 20_000;
    if report.len() <= MAX {
        return report.to_string();
    }
    let mut out = report.chars().take(MAX).collect::<String>();
    out.push_str("\n\n…(truncated)…\n");
    out
}
