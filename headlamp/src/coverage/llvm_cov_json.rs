use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

pub fn read_repo_llvm_cov_json_statement_hits(
    repo_root: &Path,
) -> Option<BTreeMap<String, BTreeMap<String, u32>>> {
    read_llvm_cov_json_statement_hits_from_path(
        repo_root,
        &repo_root.join("coverage").join("coverage.json"),
    )
}

pub fn read_llvm_cov_json_statement_hits_from_path(
    repo_root: &Path,
    json_path: &Path,
) -> Option<BTreeMap<String, BTreeMap<String, u32>>> {
    let raw = std::fs::read_to_string(json_path).ok()?;
    parse_llvm_cov_json_statement_hits(&raw, repo_root).ok()
}

pub fn parse_llvm_cov_json_statement_hits(
    text: &str,
    repo_root: &Path,
) -> Result<BTreeMap<String, BTreeMap<String, u32>>, String> {
    #[derive(Debug, Deserialize)]
    struct Root {
        data: Vec<Datum>,
    }

    #[derive(Debug, Deserialize)]
    struct Datum {
        files: Vec<FileRecord>,
    }

    #[derive(Debug, Deserialize)]
    struct FileRecord {
        filename: String,
        segments: Vec<(u64, u64, u64, bool, bool, bool)>,
    }

    let root: Root = serde_json::from_str(text).map_err(|e| e.to_string())?;

    let mut unit_hits_by_path: BTreeMap<String, BTreeMap<(u32, u32), u32>> = BTreeMap::new();

    root.data
        .into_iter()
        .flat_map(|datum| datum.files.into_iter())
        .for_each(|file| {
            let path = crate::coverage::lcov::normalize_lcov_path(&file.filename, repo_root);
            let entry = unit_hits_by_path.entry(path).or_default();
            file.segments
                .into_iter()
                .filter_map(parse_region_entry_unit)
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

fn parse_region_entry_unit(
    segment: (u64, u64, u64, bool, bool, bool),
) -> Option<((u32, u32), u32)> {
    let (line, col, hit_count, has_count, is_region_entry, is_gap_region) = segment;
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

pub fn read_llvm_cov_json_statement_totals_from_path(
    repo_root: &Path,
    json_path: &Path,
) -> Option<BTreeMap<String, (u32, u32)>> {
    let hits = read_llvm_cov_json_statement_hits_from_path(repo_root, json_path)?;
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
