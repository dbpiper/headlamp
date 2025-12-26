use std::collections::BTreeMap;
use std::path::Path;

use ignore::WalkBuilder;
use path_slash::PathExt;

use super::model::{FullFileCoverage, IstanbulFileRecord, IstanbulLocRange};

pub(super) fn read_and_merge_coverage_final_json(
    coverage_root: &Path,
    repo_root: &Path,
) -> Option<Vec<FullFileCoverage>> {
    let json_paths = WalkBuilder::new(coverage_root)
        .hidden(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .build()
        .filter_map(Result::ok)
        .filter(|dent| dent.file_type().is_some_and(|t| t.is_file()))
        .filter(|dent| {
            dent.path().file_name().and_then(|x| x.to_str()) == Some("coverage-final.json")
        })
        .map(|dent| dent.into_path())
        .collect::<Vec<_>>();

    if json_paths.is_empty() {
        return None;
    }

    let mut by_abs: BTreeMap<String, FullFileCoverage> = BTreeMap::new();
    for json_path in json_paths {
        let Ok(raw) = std::fs::read_to_string(&json_path) else {
            continue;
        };
        let Ok(obj) = serde_json::from_str::<BTreeMap<String, IstanbulFileRecord>>(&raw) else {
            continue;
        };
        for (file_key, record) in obj {
            merge_record_into_map(repo_root, &mut by_abs, &file_key, &record);
        }
    }

    by_abs.values_mut().for_each(|file| {
        if file.line_hits.is_empty()
            && !file.statement_hits.is_empty()
            && !file.statement_map.is_empty()
        {
            file.line_hits =
                derive_line_hits_from_statements(&file.statement_hits, &file.statement_map);
        }
    });

    Some(by_abs.into_values().collect::<Vec<_>>())
}

fn merge_record_into_map(
    repo_root: &Path,
    by_abs: &mut BTreeMap<String, FullFileCoverage>,
    file_key: &str,
    record: &IstanbulFileRecord,
) {
    let file_path = record.path.as_deref().unwrap_or(file_key).to_string();
    let abs = normalize_fs_path(&crate::coverage::lcov::normalize_lcov_path(
        &file_path, repo_root,
    ));
    let rel = Path::new(&abs)
        .strip_prefix(repo_root)
        .ok()
        .and_then(|p| p.to_str())
        .unwrap_or(abs.as_str())
        .to_string();

    let entry = by_abs
        .entry(abs.clone())
        .or_insert_with(|| FullFileCoverage {
            abs_path: abs.clone(),
            rel_path: Path::new(&rel).to_slash_lossy().to_string(),
            statement_hits: BTreeMap::new(),
            statement_map: BTreeMap::new(),
            function_hits: BTreeMap::new(),
            function_map: BTreeMap::new(),
            branch_hits: BTreeMap::new(),
            branch_map: BTreeMap::new(),
            line_hits: BTreeMap::new(),
        });

    merge_line_hits(&mut entry.line_hits, record.l.as_ref());
    merge_statement_hits_and_map(entry, record.s.as_ref(), record.statement_map.as_ref());
    merge_function_hits_and_map(entry, record.f.as_ref(), record.fn_map.as_ref());
    merge_branch_hits_and_map(entry, record.b.as_ref(), record.branch_map.as_ref());
}

fn merge_line_hits(target: &mut BTreeMap<u32, u32>, line_hits: Option<&BTreeMap<String, u64>>) {
    let Some(lines_obj) = line_hits else {
        return;
    };
    for (line_text, hit_count) in lines_obj {
        let Ok(line) = line_text.parse::<u32>() else {
            continue;
        };
        let add = (*hit_count).min(u64::from(u32::MAX)) as u32;
        let prev = target.get(&line).copied().unwrap_or(0);
        target.insert(line, prev.saturating_add(add));
    }
}

fn merge_statement_hits_and_map(
    target: &mut FullFileCoverage,
    statement_hits: Option<&BTreeMap<String, u64>>,
    statement_map: Option<&BTreeMap<String, IstanbulLocRange>>,
) {
    let Some(statement_hits) = statement_hits else {
        return;
    };
    for (id, hit_count) in statement_hits {
        let add = (*hit_count).min(u64::from(u32::MAX)) as u32;
        let prev = target.statement_hits.get(id).copied().unwrap_or(0);
        target
            .statement_hits
            .insert(id.to_string(), prev.saturating_add(add));
    }
    let Some(statement_map) = statement_map else {
        return;
    };
    for (id, loc) in statement_map {
        let start = loc.start.as_ref().and_then(|l| l.line).unwrap_or(0) as u32;
        let end = loc
            .end
            .as_ref()
            .and_then(|l| l.line)
            .unwrap_or(start as u64) as u32;
        if start == 0 {
            continue;
        }
        if !target.statement_map.contains_key(id) {
            target
                .statement_map
                .insert(id.to_string(), (start, end.max(start)));
        }
    }
}

fn derive_line_hits_from_statements(
    statement_hits: &BTreeMap<String, u32>,
    statement_map: &BTreeMap<String, (u32, u32)>,
) -> BTreeMap<u32, u32> {
    statement_hits
        .iter()
        .filter_map(|(id, hit)| {
            let (start, _end) = statement_map.get(id).copied()?;
            (start != 0).then_some((start, *hit))
        })
        .fold(BTreeMap::<u32, u32>::new(), |mut acc, (line, hit)| {
            let prev = acc.get(&line).copied().unwrap_or(0);
            acc.insert(line, prev.saturating_add(hit));
            acc
        })
}

fn merge_function_hits_and_map(
    target: &mut FullFileCoverage,
    function_hits: Option<&BTreeMap<String, u64>>,
    function_map: Option<&BTreeMap<String, super::model::IstanbulFnMeta>>,
) {
    let Some(function_hits) = function_hits else {
        return;
    };
    for (id, hit_count) in function_hits {
        let add = (*hit_count).min(u64::from(u32::MAX)) as u32;
        let prev = target.function_hits.get(id).copied().unwrap_or(0);
        target
            .function_hits
            .insert(id.to_string(), prev.saturating_add(add));
    }
    let Some(function_map) = function_map else {
        return;
    };
    for (id, meta) in function_map {
        let name = meta
            .name
            .clone()
            .unwrap_or_else(|| "(anonymous)".to_string());
        let line = meta.line.unwrap_or(0) as u32;
        if line == 0 {
            continue;
        }
        target
            .function_map
            .entry(id.to_string())
            .or_insert((name, line));
    }
}

fn merge_branch_hits_and_map(
    target: &mut FullFileCoverage,
    branch_hits: Option<&BTreeMap<String, Vec<u64>>>,
    branch_map: Option<&BTreeMap<String, super::model::IstanbulBranchMeta>>,
) {
    let Some(branch_hits) = branch_hits else {
        return;
    };
    for (id, hit_list) in branch_hits {
        let next = hit_list
            .iter()
            .map(|h| (*h).min(u64::from(u32::MAX)) as u32)
            .collect::<Vec<_>>();
        let existing = target.branch_hits.entry(id.to_string()).or_default();
        if existing.is_empty() {
            *existing = next;
        } else {
            let max_len = existing.len().max(next.len());
            existing.resize(max_len, 0);
            for (idx, hit) in next.into_iter().enumerate() {
                existing[idx] = existing[idx].saturating_add(hit);
            }
        }
    }

    let Some(branch_map) = branch_map else {
        return;
    };
    for (id, meta) in branch_map {
        let line = meta.line.unwrap_or(0) as u32;
        if line == 0 {
            continue;
        }
        target.branch_map.entry(id.to_string()).or_insert(line);
    }
}

fn normalize_fs_path(value: &str) -> String {
    value.replace('\\', "/")
}
