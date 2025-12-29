use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

pub fn read_repo_llvm_cov_json_statement_hits(
    repo_root: &Path,
) -> Option<BTreeMap<String, BTreeMap<String, u32>>> {
    let json_path = repo_root.join("coverage").join("coverage.json");
    let raw = std::fs::read_to_string(&json_path).ok()?;
    parse_llvm_cov_json_statement_hits(&raw, repo_root).ok()
}

pub fn parse_llvm_cov_json_statement_hits(
    text: &str,
    repo_root: &Path,
) -> Result<BTreeMap<String, BTreeMap<String, u32>>, String> {
    let root: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let data = root
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| "missing data array".to_string())?;

    let mut unit_hits_by_path: BTreeMap<String, BTreeMap<(u32, u32), u32>> = BTreeMap::new();

    data.iter()
        .flat_map(|datum| {
            datum
                .get("files")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(|file| {
            let filename = file.get("filename").and_then(Value::as_str)?;
            let normalized = crate::coverage::lcov::normalize_lcov_path(filename, repo_root);
            let segments = file.get("segments").and_then(Value::as_array)?;
            Some((normalized, segments))
        })
        .for_each(|(path, segments)| {
            let entry = unit_hits_by_path.entry(path).or_default();
            segments
                .iter()
                .filter_map(|segment| {
                    segment
                        .as_array()
                        .and_then(|v| parse_region_entry_unit(v.as_slice()))
                })
                .for_each(|((line, col), hit_count)| {
                    let prev = entry.get(&(line, col)).copied().unwrap_or(0);
                    entry.insert((line, col), prev.max(hit_count));
                });
        });

    Ok(unit_hits_by_path
        .into_iter()
        .map(|(path, units)| {
            let by_id = units
                .into_iter()
                .map(|((line, col), hit)| (format!("{line}:{col}"), hit))
                .collect::<BTreeMap<_, _>>();
            (path, by_id)
        })
        .collect::<BTreeMap<_, _>>())
}

fn parse_region_entry_unit(segment: &[Value]) -> Option<((u32, u32), u32)> {
    let line = segment.first().and_then(Value::as_u64)?;
    let col = segment.get(1).and_then(Value::as_u64)?;
    let hit_count = segment.get(2).and_then(Value::as_u64)?;
    let has_count = segment.get(3).and_then(Value::as_bool)?;
    let is_region_entry = segment.get(4).and_then(Value::as_bool)?;
    let is_gap_region = segment.get(5).and_then(Value::as_bool)?;
    let line_u32 = line.min(u64::from(u32::MAX)) as u32;
    let col_u32 = col.min(u64::from(u32::MAX)) as u32;
    let hit_u32 = hit_count.min(u64::from(u32::MAX)) as u32;
    (has_count && is_region_entry && !is_gap_region && line_u32 > 0)
        .then_some(((line_u32, col_u32), hit_u32))
}

pub fn read_repo_llvm_cov_json_statement_totals(
    repo_root: &Path,
) -> Option<BTreeMap<String, (u32, u32)>> {
    let hits = read_repo_llvm_cov_json_statement_hits(repo_root)?;
    Some(
        hits.iter()
            .map(|(path, by_id)| {
                let total = (by_id.len() as u64).min(u64::from(u32::MAX)) as u32;
                let covered = (by_id.values().filter(|h| **h > 0).count() as u64)
                    .min(u64::from(u32::MAX)) as u32;
                (path.clone(), (total, covered))
            })
            .collect::<BTreeMap<_, _>>(),
    )
}

pub fn parse_llvm_cov_json_statement_totals(
    text: &str,
    repo_root: &Path,
) -> Result<BTreeMap<String, (u32, u32)>, String> {
    let hits = parse_llvm_cov_json_statement_hits(text, repo_root)?;
    Ok(hits
        .into_iter()
        .map(|(path, by_id)| {
            let total = (by_id.len() as u64).min(u64::from(u32::MAX)) as u32;
            let covered =
                (by_id.values().filter(|h| **h > 0).count() as u64).min(u64::from(u32::MAX)) as u32;
            (path, (total, covered))
        })
        .collect::<BTreeMap<_, _>>())
}
