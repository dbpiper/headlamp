use crate::parity_meta::ParityCompareInput;

pub(super) fn build_cluster_summary(
    compare: &ParityCompareInput,
    pivot_index: usize,
    clusters: &[crate::cluster::OutputCluster],
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

pub(super) fn build_token_ast_summary(
    compare: &ParityCompareInput,
    pivot_index: usize,
    clusters: &[crate::cluster::OutputCluster],
) -> String {
    let mut lines: Vec<String> = vec!["Token stats".to_string()];
    compare.sides.iter().for_each(|side| {
        let raw = crate::token_ast::build_token_stream(&side.raw);
        let norm = crate::token_ast::build_token_stream(&side.normalized);
        lines.extend(render_token_stats_side(
            &format!("{}_raw", side.label.display_label()),
            &raw.stats,
        ));
        lines.extend(render_token_stats_side(
            &format!("{}_norm", side.label.display_label()),
            &norm.stats,
        ));
    });

    let pivot_norm =
        crate::token_ast::build_token_stream(&compare.sides[pivot_index].normalized).stats;
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
                crate::token_ast::build_token_stream(&compare.sides[other_index].normalized).stats;
            lines.extend(render_token_delta(
                &pivot_label,
                &other_label,
                &pivot_norm,
                &other_norm,
            ));
        });

    lines.join("\n")
}

fn render_token_stats_side(label: &str, stats: &crate::token_ast::TokenStats) -> Vec<String> {
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
    a: &crate::token_ast::TokenStats,
    b: &crate::token_ast::TokenStats,
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

pub(super) fn build_block_order_summary(
    compare: &ParityCompareInput,
    pivot_index: usize,
    clusters: &[crate::cluster::OutputCluster],
) -> String {
    let orders = compare
        .sides
        .iter()
        .map(|side| {
            crate::token_ast::build_document_ast(&side.normalized)
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

pub(super) fn build_artifact_summary(compare: &ParityCompareInput) -> String {
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
