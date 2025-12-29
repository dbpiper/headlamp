use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::parity_meta::ParityCompareInput;

#[derive(Debug, Clone)]
pub struct OutputCluster {
    pub normalized_hash: u64,
    pub member_indices: Vec<usize>,
}

pub fn cluster_indices_by_normalized(compare: &ParityCompareInput) -> Vec<OutputCluster> {
    let mut clusters: Vec<OutputCluster> = vec![];
    compare.sides.iter().enumerate().for_each(|(index, side)| {
        let normalized_hash = stable_hash(&side.normalized);
        match clusters
            .iter_mut()
            .find(|cluster| cluster.normalized_hash == normalized_hash)
        {
            Some(cluster) => cluster.member_indices.push(index),
            None => clusters.push(OutputCluster {
                normalized_hash,
                member_indices: vec![index],
            }),
        }
    });

    clusters.sort_by(|a, b| {
        b.member_indices
            .len()
            .cmp(&a.member_indices.len())
            .then_with(|| a.normalized_hash.cmp(&b.normalized_hash))
    });

    clusters
}

pub fn pick_pivot_index(compare: &ParityCompareInput) -> usize {
    compare
        .sides
        .iter()
        .enumerate()
        .map(|(pivot_index, pivot_side)| {
            let total_distance = compare
                .sides
                .iter()
                .map(|other_side| {
                    line_mismatch_distance(&pivot_side.normalized, &other_side.normalized)
                })
                .sum::<usize>();
            (pivot_index, total_distance)
        })
        .min_by(|(a_index, a_dist), (b_index, b_dist)| {
            a_dist.cmp(b_dist).then_with(|| {
                compare.sides[*a_index]
                    .label
                    .display_label()
                    .cmp(&compare.sides[*b_index].label.display_label())
            })
        })
        .map(|(pivot_index, _)| pivot_index)
        .unwrap_or(0)
}

fn line_mismatch_distance(a: &str, b: &str) -> usize {
    let a_lines = a.lines().collect::<Vec<_>>();
    let b_lines = b.lines().collect::<Vec<_>>();
    let shared = a_lines.len().min(b_lines.len());
    let mismatches = (0..shared).filter(|&i| a_lines[i] != b_lines[i]).count();
    mismatches + a_lines.len().abs_diff(b_lines.len())
}

fn stable_hash(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}
