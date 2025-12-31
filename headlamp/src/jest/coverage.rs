use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use tempfile::NamedTempFile;

use headlamp_core::args::ParsedArgs;
use headlamp_core::coverage::istanbul::{merge_istanbul_reports, read_istanbul_coverage_tree};
use headlamp_core::coverage::istanbul_pretty::format_istanbul_pretty;
use headlamp_core::coverage::lcov::{merge_reports, read_lcov_file, resolve_lcov_paths_to_root};
use headlamp_core::coverage::model::{CoverageReport, apply_statement_totals_to_report};
use headlamp_core::coverage::print::{
    PrintOpts, filter_report, render_report_text, should_render_hotspots,
};
use headlamp_core::coverage::thresholds::compare_thresholds_and_print_if_needed;
use indexmap::IndexSet;

use crate::run::RunError;

pub(super) fn write_asset(path: &Path, bytes: &[u8]) -> Result<PathBuf, RunError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(RunError::Io)?;
        let mut tmp = NamedTempFile::new_in(parent).map_err(RunError::Io)?;
        use std::io::Write;
        tmp.write_all(bytes).map_err(RunError::Io)?;
        tmp.flush().map_err(RunError::Io)?;
        let _ = std::fs::remove_file(path);
        tmp.persist(path).map_err(|e| RunError::Io(e.error))?;
        return Ok(path.to_path_buf());
    }
    std::fs::write(path, bytes).map_err(RunError::Io)?;
    Ok(path.to_path_buf())
}

pub(crate) fn build_jest_threshold_report(
    resolved_lcov: Option<CoverageReport>,
    merged_json: Option<CoverageReport>,
) -> Option<CoverageReport> {
    let statement_totals_by_path = merged_json.as_ref().map(|report| {
        report
            .files
            .iter()
            .filter_map(|file| {
                Some((
                    file.path.clone(),
                    (file.statements_total?, file.statements_covered?),
                ))
            })
            .collect::<std::collections::BTreeMap<_, _>>()
    });

    match (resolved_lcov, statement_totals_by_path, merged_json) {
        (Some(lcov), Some(statement_totals_by_path), _merged_json) => Some(
            apply_statement_totals_to_report(lcov, &statement_totals_by_path),
        ),
        (Some(lcov), None, _merged_json) => Some(lcov),
        (None, _maybe_map, merged_json) => merged_json,
    }
}

pub(super) fn coverage_dir_for_config_in_root(cfg_path: &Path, coverage_root: &Path) -> PathBuf {
    coverage_root
        .join("jest")
        .join(coverage_dir_suffix_for_config(cfg_path))
}

fn coverage_dir_suffix_for_config(cfg_path: &Path) -> String {
    let base = cfg_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("default");
    base.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
}

pub(super) fn collect_coverage_from_args(
    repo_root: &Path,
    selection_paths_abs: &[String],
    selection_paths_tokens: &[String],
) -> Vec<String> {
    let explicit_prod_abs = selection_paths_abs
        .iter()
        .filter(|abs| abs.contains('/') && !super::selection::looks_like_test_path(abs))
        .filter_map(|abs| {
            Path::new(abs)
                .strip_prefix(repo_root)
                .ok()
                .and_then(|p| p.to_str())
                .map(|rel| rel.replace('\\', "/"))
        })
        .filter(|rel| !rel.is_empty() && !rel.starts_with("../") && !rel.starts_with("./../"))
        .map(|rel| {
            if rel.starts_with("./") {
                rel
            } else {
                format!("./{rel}")
            }
        })
        .collect::<Vec<_>>();

    let mut out: Vec<String> = vec![];
    for rel in explicit_prod_abs {
        out.push("--collectCoverageFrom".to_string());
        out.push(rel);
    }
    if out.is_empty() {
        let _ = selection_paths_tokens;
    }
    out
}

pub(super) fn ensure_watchman_disabled_by_default(jest_args: &mut Vec<String>) {
    let has_watchman_flag = jest_args
        .iter()
        .any(|tok| tok == "--no-watchman" || tok == "--watchman" || tok.starts_with("--watchman="));
    if !has_watchman_flag {
        jest_args.push("--no-watchman".to_string());
    }
}

pub(super) fn extract_coverage_failure_lines(
    stdout_bytes: &[u8],
    stderr_bytes: &[u8],
) -> Vec<String> {
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(stdout_bytes),
        String::from_utf8_lossy(stderr_bytes)
    );
    let mut out: IndexSet<String> = IndexSet::new();
    for line in text.lines() {
        let line_without_ansi = headlamp_core::format::stacks::strip_ansi_simple(line);
        let trimmed = line_without_ansi.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(formatted) = parse_global_coverage_threshold_failure_line(trimmed) {
            out.insert(formatted);
            continue;
        }
        if trimmed.to_ascii_lowercase().contains("does not meet")
            && trimmed.to_ascii_lowercase().contains("coverage for ")
        {
            out.insert(trimmed.to_string());
        }
    }
    out.into_iter().collect()
}

fn parse_global_coverage_threshold_failure_line(line: &str) -> Option<String> {
    let prefix = r#"Jest: "global" coverage threshold for "#;
    let rest = line.strip_prefix(prefix)?;
    let (metric_raw, rest) = rest.split_once(" (")?;
    let (expected_raw, rest) = rest.split_once("%)")?;
    let actual_raw = rest.split_once("not met:")?.1.trim();
    let actual_raw = actual_raw.strip_suffix('%')?.trim();
    let expected: f64 = expected_raw.trim().parse().ok()?;
    let actual: f64 = actual_raw.parse().ok()?;
    let short = (expected - actual).max(0.0);
    let metric = titlecase_first(metric_raw.trim());
    Some(format!(
        "{metric}: {actual:.2}% < {expected:.0}% (short {short:.2}%)"
    ))
}

fn titlecase_first(text: &str) -> String {
    let mut chars = text.chars();
    let first = chars.next().unwrap_or_default();
    format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
}

fn print_coverage_threshold_failure_summary(lines: &IndexSet<String>) {
    println!();
    println!("Coverage thresholds not met");
    if lines.is_empty() {
        println!(" See tables above and jest coverageThreshold.");
        return;
    }
    lines.iter().for_each(|line| println!(" {line}"));
}

pub(crate) fn should_print_coverage_threshold_failure_summary(
    exit_code: i32,
    coverage_failure_lines: &IndexSet<String>,
) -> bool {
    exit_code != 0 && !coverage_failure_lines.is_empty()
}

pub(super) struct CollectCoverageArgs<'a> {
    pub(super) repo_root: &'a Path,
    pub(super) coverage_root: &'a Path,
    pub(super) args: &'a ParsedArgs,
    pub(super) selection_paths_abs: &'a [String],
    pub(super) coverage_failure_lines: &'a IndexSet<String>,
    pub(super) exit_code: i32,
}

struct CoverageInputs {
    jest_cov_dir: PathBuf,
    threshold_report: Option<CoverageReport>,
    resolved_for_fallback_render: Option<CoverageReport>,
}

fn collect_coverage_inputs(repo_root: &Path, coverage_root: &Path) -> CoverageInputs {
    let jest_cov_dir = coverage_root.join("jest");
    let json_tree = read_istanbul_coverage_tree(&jest_cov_dir);
    let json_reports = json_tree
        .into_iter()
        .map(|(_, report)| report)
        .collect::<Vec<_>>();
    let merged_json =
        (!json_reports.is_empty()).then(|| merge_istanbul_reports(&json_reports, repo_root));

    let lcov_candidates = collect_lcov_candidates(coverage_root, &jest_cov_dir);
    let reports = lcov_candidates
        .iter()
        .filter(|path| path.exists())
        .filter_map(|path| read_lcov_file(path).ok())
        .collect::<Vec<_>>();
    let resolved_lcov = (!reports.is_empty()).then(|| {
        let merged = merge_reports(&reports, repo_root);
        resolve_lcov_paths_to_root(merged, repo_root)
    });

    let threshold_report = build_jest_threshold_report(resolved_lcov.clone(), merged_json.clone());
    let resolved_for_fallback_render = merged_json.clone().or_else(|| resolved_lcov.clone());

    CoverageInputs {
        jest_cov_dir,
        threshold_report,
        resolved_for_fallback_render,
    }
}

fn collect_lcov_candidates(coverage_root: &Path, jest_cov_dir: &Path) -> Vec<PathBuf> {
    let mut lcov_candidates: Vec<PathBuf> = vec![coverage_root.join("lcov.info")];
    if jest_cov_dir.exists() {
        WalkBuilder::new(jest_cov_dir)
            .hidden(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .build()
            .map_while(Result::ok)
            .filter(|dent| dent.file_type().is_some_and(|t| t.is_file()))
            .filter(|dent| dent.path().file_name().and_then(|x| x.to_str()) == Some("lcov.info"))
            .for_each(|dent| lcov_candidates.push(dent.into_path()));
    }
    lcov_candidates
}

fn maybe_print_coverage(
    repo_root: &Path,
    args: &ParsedArgs,
    selection_paths_abs: &[String],
    inputs: &CoverageInputs,
) {
    if args.coverage_ui == headlamp_core::config::CoverageUi::Jest {
        return;
    }

    let print_opts =
        PrintOpts::for_run(args, headlamp_core::format::terminal::is_output_terminal());

    if let Some(pretty) = format_istanbul_pretty(
        repo_root,
        &inputs.jest_cov_dir,
        &print_opts,
        selection_paths_abs,
        &args.include_globs,
        &args.exclude_globs,
        args.coverage_detail,
    ) {
        println!("{pretty}");
        return;
    }

    let Some(resolved) = inputs.resolved_for_fallback_render.clone() else {
        return;
    };

    let filtered = filter_report(
        resolved,
        repo_root,
        &args.include_globs,
        &args.exclude_globs,
    );
    let include_hotspots = should_render_hotspots(args.coverage_detail);
    println!(
        "{}",
        render_report_text(&filtered, &print_opts, repo_root, include_hotspots)
    );
}

fn apply_thresholds_and_exit_code(
    args: &ParsedArgs,
    mut exit_code: i32,
    threshold_report: Option<&CoverageReport>,
    coverage_failure_lines: &IndexSet<String>,
) -> i32 {
    let thresholds_failed =
        compare_thresholds_and_print_if_needed(args.coverage_thresholds.as_ref(), threshold_report);
    if exit_code == 0 && thresholds_failed {
        exit_code = 1;
    } else if should_print_coverage_threshold_failure_summary(exit_code, coverage_failure_lines) {
        print_coverage_threshold_failure_summary(coverage_failure_lines);
    }
    exit_code
}

pub(super) fn collect_and_print_coverage(args: CollectCoverageArgs<'_>) -> Result<i32, RunError> {
    let CollectCoverageArgs {
        repo_root,
        coverage_root,
        args,
        selection_paths_abs,
        coverage_failure_lines,
        exit_code,
    } = args;

    let inputs = collect_coverage_inputs(repo_root, coverage_root);
    maybe_print_coverage(repo_root, args, selection_paths_abs, &inputs);
    let final_exit = apply_thresholds_and_exit_code(
        args,
        exit_code,
        inputs.threshold_report.as_ref(),
        coverage_failure_lines,
    );
    Ok(final_exit)
}
