use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

pub fn read_repo_coveragepy_json_statement_totals(
    repo_root: &Path,
) -> Option<BTreeMap<String, (u32, u32)>> {
    read_coveragepy_json_statement_totals_from_path(
        repo_root,
        &repo_root.join("coverage").join("coverage.json"),
    )
}

pub fn read_coveragepy_json_statement_totals_from_path(
    repo_root: &Path,
    json_path: &Path,
) -> Option<BTreeMap<String, (u32, u32)>> {
    let raw = std::fs::read_to_string(json_path).ok()?;
    parse_coveragepy_json_statement_totals(&raw, repo_root).ok()
}

pub fn parse_coveragepy_json_statement_totals(
    text: &str,
    repo_root: &Path,
) -> Result<BTreeMap<String, (u32, u32)>, String> {
    let root: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let files_obj = root
        .get("files")
        .and_then(Value::as_object)
        .ok_or_else(|| "missing files object".to_string())?;

    let totals_by_path = files_obj
        .iter()
        .filter_map(|(path, file_record)| {
            let summary = file_record.get("summary")?;
            let total = summary.get("num_statements").and_then(Value::as_u64)?;
            let covered = summary
                .get("covered_lines")
                .and_then(Value::as_u64)
                .or_else(|| {
                    summary
                        .get("missing_lines")
                        .and_then(Value::as_u64)
                        .map(|missing| total.saturating_sub(missing))
                })?;
            let normalized = crate::coverage::lcov::normalize_lcov_path(path, repo_root);
            Some((
                normalized,
                (
                    total.min(u64::from(u32::MAX)) as u32,
                    covered.min(u64::from(u32::MAX)) as u32,
                ),
            ))
        })
        .collect::<BTreeMap<_, _>>();
    Ok(totals_by_path)
}
