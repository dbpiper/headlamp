use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use serde::Deserialize;

use crate::coverage::model::{CoverageReport, FileCoverage};
use crate::error::HeadlampError;

#[derive(Debug, Clone, Deserialize)]
struct IstanbulLoc {
    #[serde(default)]
    line: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct IstanbulStatementLoc {
    #[serde(default)]
    start: Option<IstanbulLoc>,
}

#[derive(Debug, Clone, Deserialize)]
struct IstanbulFileRecord {
    #[serde(default)]
    path: Option<String>,

    #[serde(default)]
    l: Option<BTreeMap<String, u64>>,

    #[serde(default)]
    s: Option<BTreeMap<String, u64>>,

    #[serde(default)]
    #[serde(rename = "statementMap")]
    statement_map: Option<BTreeMap<String, IstanbulStatementLoc>>,
}

pub fn read_istanbul_coverage_file(path: &Path) -> Result<CoverageReport, HeadlampError> {
    let raw = std::fs::read_to_string(path).map_err(|source| HeadlampError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse_istanbul_coverage_text(&raw).map_err(|message| HeadlampError::ConfigParse {
        path: path.to_path_buf(),
        message,
    })
}

pub fn read_istanbul_coverage_tree(root: &Path) -> Vec<(PathBuf, CoverageReport)> {
    WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .build()
        .map_while(Result::ok)
        .filter(|dent| dent.file_type().is_some_and(|t| t.is_file()))
        .filter(|dent| {
            dent.path().file_name().and_then(|x| x.to_str()) == Some("coverage-final.json")
        })
        .filter_map(|dent| {
            let p = dent.into_path();
            read_istanbul_coverage_file(&p).ok().map(|rep| (p, rep))
        })
        .collect()
}

pub fn merge_istanbul_reports(reports: &[CoverageReport], root: &Path) -> CoverageReport {
    let mut by_file: BTreeMap<String, BTreeMap<u32, u32>> = BTreeMap::new();
    let mut statement_hits_by_file: BTreeMap<String, BTreeMap<u64, u32>> = BTreeMap::new();
    for report in reports {
        for file in &report.files {
            let abs = super::lcov::normalize_lcov_path(&file.path, root);
            let entry = by_file.entry(abs.clone()).or_default();
            for (ln, hit) in &file.line_hits {
                let prev = entry.get(ln).copied().unwrap_or(0);
                entry.insert(*ln, prev.saturating_add(*hit));
            }

            if let Some(statement_hits) = file.statement_hits.as_ref() {
                let statement_entry = statement_hits_by_file.entry(abs.clone()).or_default();
                for (statement_id, statement_hit_count) in statement_hits {
                    let prev = statement_entry.get(statement_id).copied().unwrap_or(0);
                    statement_entry
                        .insert(*statement_id, prev.saturating_add(*statement_hit_count));
                }
            }
        }
    }

    let files = by_file
        .into_iter()
        .map(|(path, hits)| {
            let (covered, total, uncovered) =
                hits.iter()
                    .fold((0u32, 0u32, Vec::<u32>::new()), |acc, (ln, h)| {
                        let (covered, total, mut uncovered) = acc;
                        let total2 = total.saturating_add(1);
                        let covered2 = if *h > 0 {
                            covered.saturating_add(1)
                        } else {
                            covered
                        };
                        if *h == 0 {
                            uncovered.push(*ln);
                        }
                        (covered2, total2, uncovered)
                    });
            let (statements_total, statements_covered, statement_hits) = statement_hits_by_file
                .remove(&path)
                .map_or((None, None, None), |statement_hits| {
                    let total = statement_hits.len() as u32;
                    let covered = statement_hits.values().filter(|h| **h > 0).count() as u32;
                    (Some(total), Some(covered), Some(statement_hits))
                });
            FileCoverage {
                path,
                lines_total: total,
                lines_covered: covered,
                statements_total,
                statements_covered,
                statement_hits,
                uncovered_lines: uncovered,
                line_hits: hits,
                function_hits: BTreeMap::new(),
                function_map: BTreeMap::new(),
                branch_hits: BTreeMap::new(),
                branch_map: BTreeMap::new(),
            }
        })
        .collect::<Vec<_>>();

    CoverageReport { files }
}

pub fn parse_istanbul_coverage_text(text: &str) -> Result<CoverageReport, String> {
    let obj = serde_json::from_str::<BTreeMap<String, IstanbulFileRecord>>(text)
        .map_err(|e| e.to_string())?;

    let mut files: Vec<FileCoverage> = vec![];

    for (file_key, file_record) in obj {
        let file_path = file_record
            .path
            .as_deref()
            .unwrap_or(file_key.as_str())
            .to_string();
        let line_hits = extract_line_hits(&file_record)?;
        let statement_hits = extract_statement_hits(&file_record);
        let (statements_total, statements_covered) = statement_hits
            .as_ref()
            .map(|hits| {
                let total = hits.len() as u32;
                let covered = hits.values().filter(|h| **h > 0).count() as u32;
                (Some(total), Some(covered))
            })
            .unwrap_or((None, None));
        let (covered, total, uncovered) =
            line_hits
                .iter()
                .fold((0u32, 0u32, Vec::<u32>::new()), |acc, (ln, h)| {
                    let (covered, total, mut uncovered) = acc;
                    let total2 = total.saturating_add(1);
                    let covered2 = if *h > 0 {
                        covered.saturating_add(1)
                    } else {
                        covered
                    };
                    if *h == 0 {
                        uncovered.push(*ln);
                    }
                    (covered2, total2, uncovered)
                });
        files.push(FileCoverage {
            path: file_path,
            lines_total: total,
            lines_covered: covered,
            statements_total,
            statements_covered,
            statement_hits,
            uncovered_lines: uncovered,
            line_hits,
            function_hits: BTreeMap::new(),
            function_map: BTreeMap::new(),
            branch_hits: BTreeMap::new(),
            branch_map: BTreeMap::new(),
        });
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(CoverageReport { files })
}

fn extract_line_hits(file_record: &IstanbulFileRecord) -> Result<BTreeMap<u32, u32>, String> {
    if let Some(lines_obj) = file_record.l.as_ref() {
        let hits = lines_obj
            .iter()
            .filter_map(|(k, hit)| Some((k.parse::<u32>().ok()?, (*hit as u32))))
            .collect::<BTreeMap<_, _>>();
        return Ok(hits);
    }

    let s = file_record
        .s
        .as_ref()
        .ok_or_else(|| "missing statement hit map (s)".to_string())?;
    let statement_map = file_record
        .statement_map
        .as_ref()
        .ok_or_else(|| "missing statementMap".to_string())?;

    let mut hits: BTreeMap<u32, u32> = BTreeMap::new();
    for (id, count_val) in s {
        let count = *count_val as u32;
        let line = statement_map
            .get(id)
            .and_then(|loc| loc.start.as_ref())
            .and_then(|start| start.line)
            .unwrap_or(0) as u32;
        if line == 0 {
            continue;
        }
        let prev = hits.get(&line).copied().unwrap_or(0);
        hits.insert(line, prev.saturating_add(count));
    }
    Ok(hits)
}

fn extract_statement_hits(file_record: &IstanbulFileRecord) -> Option<BTreeMap<u64, u32>> {
    file_record.s.as_ref().map(|statement_hits_raw| {
        statement_hits_raw
            .iter()
            .filter_map(|(id_text, hit_count)| {
                let statement_id = id_text.parse::<u64>().ok()?;
                let add = (*hit_count).min(u64::from(u32::MAX)) as u32;
                Some((statement_id, add))
            })
            .collect::<BTreeMap<_, _>>()
    })
}
