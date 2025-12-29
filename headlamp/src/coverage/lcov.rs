use std::collections::BTreeMap;
use std::path::Path;

use lcov::Reader;
use lcov::Record;
use path_slash::PathExt;

use crate::coverage::model::{CoverageReport, FileCoverage};
use crate::coverage::print::filter_report;
use crate::error::HeadlampError;

#[derive(Debug, Clone)]
struct LcovFileBuf {
    path: String,
    hits: BTreeMap<u32, u32>,
    function_start_line_by_name: BTreeMap<String, u32>,
    function_count_by_name: BTreeMap<String, u32>,
    branch_count_by_line_and_block: BTreeMap<(u32, u32), BTreeMap<u32, u32>>,
}

#[derive(Debug, Default)]
struct LcovParseState {
    current: Option<LcovFileBuf>,
    files: Vec<FileCoverage>,
}

pub fn parse_lcov_text(text: &str) -> CoverageReport {
    let mut state = Reader::new(text.as_bytes()).map_while(Result::ok).fold(
        LcovParseState::default(),
        |mut state, record| {
            match record {
                Record::SourceFile { path } => {
                    flush_current(&mut state);
                    state.current = Some(LcovFileBuf {
                        path: path.to_string_lossy().to_string(),
                        hits: BTreeMap::new(),
                        function_start_line_by_name: BTreeMap::new(),
                        function_count_by_name: BTreeMap::new(),
                        branch_count_by_line_and_block: BTreeMap::new(),
                    });
                }
                Record::LineData { line, count, .. } => {
                    let Some(buf) = state.current.as_mut() else {
                        return state;
                    };
                    let hit_count_u32 = count.min(u64::from(u32::MAX)) as u32;
                    let previous_hit_count = buf.hits.get(&line).copied().unwrap_or(0);
                    buf.hits
                        .insert(line, previous_hit_count.saturating_add(hit_count_u32));
                }
                Record::FunctionName { name, start_line } => {
                    let Some(buf) = state.current.as_mut() else {
                        return state;
                    };
                    let normalized = normalize_lcov_function_key(&name);
                    buf.function_start_line_by_name
                        .entry(normalized)
                        .or_insert(start_line);
                }
                Record::FunctionData { name, count } => {
                    let Some(buf) = state.current.as_mut() else {
                        return state;
                    };
                    let normalized = normalize_lcov_function_key(&name);
                    let hit_count_u32 = count.min(u64::from(u32::MAX)) as u32;
                    let previous_hit_count = buf
                        .function_count_by_name
                        .get(&normalized)
                        .copied()
                        .unwrap_or(0);
                    buf.function_count_by_name
                        .insert(normalized, previous_hit_count.saturating_add(hit_count_u32));
                }
                Record::BranchData {
                    line,
                    block,
                    branch,
                    taken,
                } => {
                    let Some(buf) = state.current.as_mut() else {
                        return state;
                    };
                    let taken_u32 = taken.unwrap_or(0).min(u64::from(u32::MAX)) as u32;
                    let entry = buf
                        .branch_count_by_line_and_block
                        .entry((line, block))
                        .or_default();
                    let prev = entry.get(&branch).copied().unwrap_or(0);
                    entry.insert(branch, prev.saturating_add(taken_u32));
                }
                Record::EndOfRecord => {
                    flush_current(&mut state);
                }
                _ => {}
            }
            state
        },
    );

    flush_current(&mut state);
    state.files.sort_by(|a, b| a.path.cmp(&b.path));
    CoverageReport { files: state.files }
}

pub fn read_lcov_file(path: &Path) -> Result<CoverageReport, HeadlampError> {
    let raw = std::fs::read_to_string(path).map_err(|source| HeadlampError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(parse_lcov_text(&raw))
}

fn flush_current(state: &mut LcovParseState) {
    let Some(buf) = state.current.take() else {
        return;
    };
    let (covered, total, uncovered) = compute_line_totals(&buf.hits);
    let (function_hits, function_map) = compute_function_maps(
        &buf.function_count_by_name,
        &buf.function_start_line_by_name,
    );
    let (branch_hits, branch_map) = compute_branch_maps(&buf.branch_count_by_line_and_block);
    state.files.push(FileCoverage {
        path: buf.path,
        lines_total: total,
        lines_covered: covered,
        statements_total: None,
        statements_covered: None,
        statement_hits: None,
        uncovered_lines: uncovered,
        line_hits: buf.hits,
        function_hits,
        function_map,
        branch_hits,
        branch_map,
    });
}

fn compute_line_totals(hits: &BTreeMap<u32, u32>) -> (u32, u32, Vec<u32>) {
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
        })
}

fn normalize_lcov_function_key(raw: &str) -> String {
    normalize_rust_v0_mangled_function_key(raw).unwrap_or_else(|| raw.to_string())
}

fn normalize_rust_v0_mangled_function_key(raw: &str) -> Option<String> {
    // Rust v0-mangled symbols often contain a `Cs<crate_hash>_` segment:
    //   _RNvNtCs2PylkhAFI23_11parity_real1a1a
    //   _RNvNtCsiNoeFlk8yU1_11parity_real1a1a
    //
    // `cargo llvm-cov report --lcov` can emit multiple FN/FNDA records for the same source
    // function that only differ by this crate hash/disambiguator. For UI parity (and sensible
    // function counts), collapse them by stripping that segment and keeping the suffix.
    let rest = raw.strip_prefix("_RNvNtCs")?;
    let (_crate_hash, suffix) = rest.split_once('_')?;
    Some(suffix.to_string())
}

fn compute_function_maps(
    function_count_by_name: &BTreeMap<String, u32>,
    function_start_line_by_name: &BTreeMap<String, u32>,
) -> (BTreeMap<String, u32>, BTreeMap<String, (String, u32)>) {
    function_count_by_name
        .iter()
        .map(|(name, hits)| {
            let line = function_start_line_by_name.get(name).copied().unwrap_or(0);
            let id = if line == 0 {
                name.clone()
            } else {
                format!("{line}:{name}")
            };
            (id.clone(), (*hits, (name.clone(), line)))
        })
        .fold(
            (
                BTreeMap::<String, u32>::new(),
                BTreeMap::<String, (String, u32)>::new(),
            ),
            |mut acc, (id, (hits, meta))| {
                acc.0.insert(id.clone(), hits);
                acc.1.insert(id, meta);
                acc
            },
        )
}

fn compute_branch_maps(
    branch_count_by_line_and_block: &BTreeMap<(u32, u32), BTreeMap<u32, u32>>,
) -> (BTreeMap<String, Vec<u32>>, BTreeMap<String, u32>) {
    branch_count_by_line_and_block
        .iter()
        .map(|((line, block), by_branch)| {
            let id = format!("{line}:{block}");
            let hits = by_branch.values().copied().collect::<Vec<_>>();
            (id.clone(), (*line, hits))
        })
        .fold(
            (
                BTreeMap::<String, Vec<u32>>::new(),
                BTreeMap::<String, u32>::new(),
            ),
            |mut acc, (id, (line, hits))| {
                acc.0.insert(id.clone(), hits);
                acc.1.insert(id, line);
                acc
            },
        )
}

pub fn normalize_lcov_path(path: &str, root: &Path) -> String {
    let p = Path::new(path);
    if p.is_absolute() {
        return normalize_sep(p);
    }
    normalize_sep(&root.join(p))
}

fn normalize_sep(path: &Path) -> String {
    path.to_slash_lossy().to_string()
}

pub fn merge_reports(reports: &[CoverageReport], root: &Path) -> CoverageReport {
    let mut by_file: BTreeMap<String, BTreeMap<u32, u32>> = BTreeMap::new();
    type FnMeta = (String, u32);
    type FnEntry = (u32, FnMeta);
    type FnMap = BTreeMap<String, FnEntry>;
    let mut functions_by_file: BTreeMap<String, FnMap> = BTreeMap::new();
    let mut branches_by_file: BTreeMap<String, BTreeMap<String, (Vec<u32>, u32)>> = BTreeMap::new();
    for report in reports {
        for file in &report.files {
            let abs = normalize_lcov_path(&file.path, root);
            let entry = by_file.entry(abs.clone()).or_default();
            for (ln, hit) in &file.line_hits {
                let prev = entry.get(ln).copied().unwrap_or(0);
                entry.insert(*ln, prev.saturating_add(*hit));
            }

            let fn_entry = functions_by_file.entry(abs.clone()).or_default();
            for (id, hit) in &file.function_hits {
                let prev = fn_entry.get(id).map(|(h, _meta)| *h).unwrap_or(0);
                let meta = file
                    .function_map
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| ("(anonymous)".to_string(), 0));
                fn_entry.insert(id.clone(), (prev.saturating_add(*hit), meta));
            }

            let br_entry = branches_by_file.entry(abs.clone()).or_default();
            for (id, hits) in &file.branch_hits {
                let prev = br_entry
                    .get(id)
                    .map(|(h, _line)| h.clone())
                    .unwrap_or_default();
                let max_len = prev.len().max(hits.len());
                let mut merged = prev;
                merged.resize(max_len, 0);
                for (idx, hit) in hits.iter().copied().enumerate() {
                    merged[idx] = merged[idx].saturating_add(hit);
                }
                let line = file.branch_map.get(id).copied().unwrap_or(0);
                br_entry.insert(id.clone(), (merged, line));
            }
        }
    }

    let files = by_file
        .into_iter()
        .map(|(path, hits)| {
            let (covered, total, uncovered) = compute_line_totals(&hits);
            let (function_hits, function_map) = functions_by_file
                .get(&path)
                .map(|m| {
                    m.iter().fold(
                        (
                            BTreeMap::<String, u32>::new(),
                            BTreeMap::<String, (String, u32)>::new(),
                        ),
                        |mut acc, (id, (hit, meta))| {
                            acc.0.insert(id.clone(), *hit);
                            acc.1.insert(id.clone(), meta.clone());
                            acc
                        },
                    )
                })
                .unwrap_or_else(|| (BTreeMap::new(), BTreeMap::new()));
            let (branch_hits, branch_map) = branches_by_file
                .get(&path)
                .map(|m| {
                    m.iter().fold(
                        (
                            BTreeMap::<String, Vec<u32>>::new(),
                            BTreeMap::<String, u32>::new(),
                        ),
                        |mut acc, (id, (hits, line))| {
                            acc.0.insert(id.clone(), hits.clone());
                            acc.1.insert(id.clone(), *line);
                            acc
                        },
                    )
                })
                .unwrap_or_else(|| (BTreeMap::new(), BTreeMap::new()));
            FileCoverage {
                path,
                lines_total: total,
                lines_covered: covered,
                statements_total: None,
                statements_covered: None,
                statement_hits: None,
                uncovered_lines: uncovered,
                line_hits: hits,
                function_hits,
                function_map,
                branch_hits,
                branch_map,
            }
        })
        .collect::<Vec<_>>();

    CoverageReport { files }
}

pub fn resolve_lcov_paths_to_root(report: CoverageReport, root: &Path) -> CoverageReport {
    let files = report
        .files
        .into_iter()
        .map(|f| {
            let abs = normalize_lcov_path(&f.path, root);
            FileCoverage { path: abs, ..f }
        })
        .collect::<Vec<_>>();
    CoverageReport { files }
}

pub fn read_repo_lcov_filtered(
    repo_root: &Path,
    includes: &[String],
    excludes: &[String],
) -> Option<CoverageReport> {
    let lcov = repo_root.join("coverage").join("lcov.info");
    if !lcov.exists() {
        return None;
    }
    let reports = read_lcov_file(&lcov).ok().into_iter().collect::<Vec<_>>();
    let merged = merge_reports(&reports, repo_root);
    let resolved = resolve_lcov_paths_to_root(merged, repo_root);
    Some(filter_report(resolved, repo_root, includes, excludes))
}
