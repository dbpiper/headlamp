use crate::parity_meta::ParityCompareInput;

mod compare;
mod summary;
mod tables;
mod utils;

pub fn build_parity_report_with_meta(compare: &ParityCompareInput) -> String {
    if compare.sides.len() < 2 {
        return "Parity report: only one side".to_string();
    }

    let clusters = crate::cluster::cluster_indices_by_normalized(compare);
    let pivot_index = crate::cluster::pick_pivot_index(compare);
    let pivot_label = compare.sides[pivot_index].label.display_label();
    let pivot_norm = &compare.sides[pivot_index].normalized;

    let mut sections = base_sections(compare, pivot_index, &clusters);

    other_indices_for_comparisons(compare, pivot_index, &clusters)
        .into_iter()
        .for_each(|other_index| {
            let other_label = compare.sides[other_index].label.display_label();
            let other_norm = &compare.sides[other_index].normalized;
            push_comparison_sections(
                &mut sections,
                &pivot_label,
                &other_label,
                pivot_norm,
                other_norm,
            );
        });

    finalize_sections(sections)
}

fn base_sections(
    compare: &ParityCompareInput,
    pivot_index: usize,
    clusters: &[crate::cluster::OutputCluster],
) -> Vec<String> {
    vec![
        summary::build_artifact_summary(compare),
        summary::build_cluster_summary(compare, pivot_index, clusters),
        summary::build_token_ast_summary(compare, pivot_index, clusters),
        summary::build_block_order_summary(compare, pivot_index, clusters),
    ]
}

fn other_indices_for_comparisons(
    compare: &ParityCompareInput,
    pivot_index: usize,
    clusters: &[crate::cluster::OutputCluster],
) -> Vec<usize> {
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
        .collect()
}

fn push_comparison_sections(
    sections: &mut Vec<String>,
    pivot_label: &str,
    other_label: &str,
    pivot_norm: &str,
    other_norm: &str,
) {
    sections.push(format!("Comparison: {pivot_label} vs {other_label}"));
    sections.push(compare::build_classification_section(
        pivot_norm, other_norm,
    ));
    sections.push(compare::build_first_mismatch_section(
        pivot_label,
        other_label,
        pivot_norm,
        other_norm,
    ));
    sections.push(compare::build_blank_runs_section(
        pivot_label,
        other_label,
        pivot_norm,
        other_norm,
    ));
    sections.push(tables::build_table_section(
        pivot_label,
        other_label,
        pivot_norm,
        other_norm,
    ));
    sections.push(tables::build_istanbul_table_section(
        pivot_label,
        other_label,
        pivot_norm,
        other_norm,
    ));
    sections.push(compare::build_counts_section(
        pivot_label,
        other_label,
        pivot_norm,
        other_norm,
    ));
}

fn finalize_sections(sections: Vec<String>) -> String {
    sections
        .into_iter()
        .filter(|section| !section.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn build_parity_report(out_ts: &str, out_rs: &str) -> String {
    build_parity_report_with_meta(&ParityCompareInput {
        sides: vec![
            crate::parity_meta::ParityCompareSideInput {
                label: crate::parity_meta::ParitySideLabel {
                    binary: "unknown".to_string(),
                    runner_stack: "unknown".to_string(),
                },
                exit: 0,
                raw: out_ts.to_string(),
                normalized: out_ts.to_string(),
                meta: crate::parity_meta::ParitySideMeta {
                    raw_bytes: out_ts.len(),
                    raw_lines: out_ts.lines().count(),
                    normalized_bytes: out_ts.len(),
                    normalized_lines: out_ts.lines().count(),
                    normalization: crate::parity_meta::NormalizationMeta {
                        normalizer: crate::parity_meta::NormalizerKind::NonTty,
                        used_fallback: false,
                        last_failed_tests_line: None,
                        last_test_files_line: None,
                        last_box_table_top_line: None,
                        stages: vec![],
                    },
                },
            },
            crate::parity_meta::ParityCompareSideInput {
                label: crate::parity_meta::ParitySideLabel {
                    binary: "unknown".to_string(),
                    runner_stack: "unknown".to_string(),
                },
                exit: 0,
                raw: out_rs.to_string(),
                normalized: out_rs.to_string(),
                meta: crate::parity_meta::ParitySideMeta {
                    raw_bytes: out_rs.len(),
                    raw_lines: out_rs.lines().count(),
                    normalized_bytes: out_rs.len(),
                    normalized_lines: out_rs.lines().count(),
                    normalization: crate::parity_meta::NormalizationMeta {
                        normalizer: crate::parity_meta::NormalizerKind::NonTty,
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
