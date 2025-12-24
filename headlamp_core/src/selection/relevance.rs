use std::cmp::Ordering;
use std::collections::BTreeMap;

fn normalize_abs_posix(input_path: &str) -> String {
    input_path.replace('\\', "/")
}

pub fn file_failed(file_status: &str, assertion_statuses: impl Iterator<Item = String>) -> bool {
    if file_status == "failed" {
        return true;
    }
    assertion_statuses
        .into_iter()
        .any(|status| status == "failed")
}

pub fn comparator_for_path_rank<'a>(
    rank_by_path: &'a BTreeMap<String, i64>,
) -> impl Fn(&str, &str) -> Ordering + 'a {
    move |left_path: &str, right_path: &str| {
        let left_abs = normalize_abs_posix(left_path);
        let right_abs = normalize_abs_posix(right_path);
        let left_rank = rank_by_path.get(&left_abs).copied().unwrap_or(i64::MAX);
        let right_rank = rank_by_path.get(&right_abs).copied().unwrap_or(i64::MAX);

        left_rank
            .cmp(&right_rank)
            .then_with(|| left_abs.cmp(&right_abs))
    }
}

pub fn augment_rank_with_priority_paths(
    rank_by_path: &BTreeMap<String, i64>,
    priority_paths: &[String],
) -> BTreeMap<String, i64> {
    if priority_paths.is_empty() {
        return rank_by_path.clone();
    }
    let total = priority_paths.len() as i64;
    priority_paths
        .iter()
        .map(|path_text| normalize_abs_posix(path_text))
        .enumerate()
        .fold(rank_by_path.clone(), |mut acc, (index, abs)| {
            let priority_rank = -(total - index as i64);
            let next = acc
                .get(&abs)
                .copied()
                .map(|existing| existing.min(priority_rank))
                .unwrap_or(priority_rank);
            acc.insert(abs, next);
            acc
        })
}
