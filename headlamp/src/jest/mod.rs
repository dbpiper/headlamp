use std::path::{Path, PathBuf};

use path_slash::PathExt;

use indexmap::IndexSet;

#[cfg(test)]
use crate::coverage::model::CoverageReport;
use headlamp_core::args::ParsedArgs;
use headlamp_core::format::ctx::make_ctx;
use headlamp_core::format::vitest::render_vitest_from_test_model;
use headlamp_core::selection::dependency_language::DependencyLanguageId;
use headlamp_core::selection::relevance::augment_rank_with_priority_paths;

use crate::jest_config::list_all_jest_configs;
use crate::jest_discovery::{args_for_discovery, jest_bin};
use crate::live_progress::live_progress_mode;
use crate::run::{RunError, run_bootstrap};

mod bridge;
mod coverage;
mod project_run;
mod selection;
mod streaming;

#[cfg(test)]
pub(crate) fn build_jest_threshold_report(
    resolved_lcov: Option<CoverageReport>,
    merged_json: Option<CoverageReport>,
) -> Option<CoverageReport> {
    coverage::build_jest_threshold_report(resolved_lcov, merged_json)
}

#[cfg(test)]
pub(crate) fn should_print_coverage_threshold_failure_summary(
    exit_code: i32,
    coverage_failure_lines: &IndexSet<String>,
) -> bool {
    coverage::should_print_coverage_threshold_failure_summary(exit_code, coverage_failure_lines)
}

const JEST_REPORTER_BYTES: &[u8] = include_bytes!("../../assets/jest/reporter.cjs");
const JEST_SETUP_BYTES: &[u8] = include_bytes!("../../assets/jest/setup.cjs");

#[derive(Debug)]
struct AggregatedProjectRuns {
    exit_code: i32,
    bridges: Vec<headlamp_core::test_model::TestRunModel>,
    captured_stdout: Vec<String>,
    captured_stderr: Vec<String>,
    coverage_failure_lines: IndexSet<String>,
    raw_output_all: Vec<String>,
}

pub fn run_jest(repo_root: &Path, args: &ParsedArgs) -> Result<i32, RunError> {
    run_bootstrap_if_configured(repo_root, args)?;
    let jest_bin = ensure_jest_bin_exists(repo_root)?;
    let selection_paths_abs = selection::selection_paths_abs(repo_root, args)?;
    let discovery_args = args_for_discovery(&args.runner_args);
    let project_configs = project_configs_for_repo_root(repo_root);
    let selection_exclude_globs = selection::exclude_globs_for_selection(&args.exclude_globs);
    let selection_is_tests_only = selection_is_tests_only(&selection_paths_abs);
    let production_seeds = production_seeds_abs(&selection_paths_abs);
    let selection_key = selection_key(
        repo_root,
        &selection_paths_abs,
        selection_is_tests_only,
        &production_seeds,
    );
    let dependency_language = args
        .dependency_language
        .unwrap_or(DependencyLanguageId::TsJs);
    let related_selection =
        selection::compute_related_selection(selection::ComputeRelatedSelectionArgs {
            repo_root,
            args,
            project_configs: &project_configs,
            jest_bin: &jest_bin,
            discovery_args: &discovery_args,
            dependency_language,
            selection_key: selection_key.as_deref(),
            selection_is_tests_only,
            selection_paths_abs: &selection_paths_abs,
            production_seeds_abs: &production_seeds,
            selection_exclude_globs: &selection_exclude_globs,
        })?;
    let directness_rank = compute_directness_rank(
        repo_root,
        &selection_paths_abs,
        &selection_exclude_globs,
        args.no_cache,
        &related_selection.selected_test_paths_abs,
    )?;
    let tmp = std::env::temp_dir().join("headlamp").join("jest");
    let (reporter_path, setup_path, out_json_base) = write_jest_assets(&tmp)?;
    let name_pattern_only_for_discovery =
        bridge::should_skip_run_tests_by_path_for_name_pattern_only(args, &selection_paths_abs);
    let base_cmd_args =
        build_base_cmd_args(&setup_path, &reporter_path, name_pattern_only_for_discovery);
    let mode = live_progress_mode(
        headlamp_core::format::terminal::is_output_terminal(),
        args.ci,
    );
    let per_project_results = project_run::run_projects(project_run::RunProjectsArgs {
        repo_root,
        args,
        project_configs: &project_configs,
        jest_bin: &jest_bin,
        discovery_args: &discovery_args,
        related_selection: &related_selection.selected_test_paths_abs,
        base_cmd_args: &base_cmd_args,
        selection_paths_abs: &selection_paths_abs,
        name_pattern_only_for_discovery,
        out_json_base: &out_json_base,
        mode,
    })?;
    let aggregated = aggregate_project_runs(per_project_results);
    print_jest_run_output(repo_root, args, &directness_rank, &aggregated);
    maybe_collect_coverage(repo_root, args, &selection_paths_abs, &aggregated)
}

fn run_bootstrap_if_configured(repo_root: &Path, args: &ParsedArgs) -> Result<(), RunError> {
    args.bootstrap_command
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|cmd| run_bootstrap(repo_root, cmd))
        .transpose()?;
    Ok(())
}

fn ensure_jest_bin_exists(repo_root: &Path) -> Result<PathBuf, RunError> {
    let bin = jest_bin(repo_root);
    let hint = format!("expected {}", bin.display());
    bin.exists()
        .then_some(bin)
        .ok_or_else(|| RunError::MissingRunner {
            runner: "jest".to_string(),
            hint,
        })
}

fn project_configs_for_repo_root(repo_root: &Path) -> Vec<PathBuf> {
    let discovered = list_all_jest_configs(repo_root);
    if discovered.is_empty() {
        vec![repo_root.to_path_buf()]
    } else {
        discovered
    }
}

fn selection_is_tests_only(selection_paths_abs: &[String]) -> bool {
    !selection_paths_abs.is_empty()
        && selection_paths_abs
            .iter()
            .all(|abs| selection::looks_like_test_path(abs))
}

fn production_seeds_abs(selection_paths_abs: &[String]) -> Vec<String> {
    selection_paths_abs
        .iter()
        .filter(|abs| !selection::looks_like_test_path(abs))
        .cloned()
        .collect::<Vec<_>>()
}

fn selection_key(
    repo_root: &Path,
    selection_paths_abs: &[String],
    selection_is_tests_only: bool,
    production_seeds_abs: &[String],
) -> Option<String> {
    if selection_paths_abs.is_empty() || selection_is_tests_only {
        None
    } else {
        Some(
            production_seeds_abs
                .iter()
                .map(|abs| {
                    Path::new(abs)
                        .strip_prefix(repo_root)
                        .ok()
                        .map(|p| p.to_slash_lossy().to_string())
                        .unwrap_or_else(|| Path::new(abs).to_slash_lossy().to_string())
                })
                .collect::<Vec<_>>()
                .join("|"),
        )
    }
}

fn compute_directness_rank(
    repo_root: &Path,
    selection_paths_abs: &[String],
    selection_exclude_globs: &[String],
    no_cache: bool,
    related_tests_abs: &[String],
) -> Result<std::collections::BTreeMap<String, i64>, RunError> {
    let base = selection::compute_directness_rank_base(
        repo_root,
        selection_paths_abs,
        selection_exclude_globs,
        no_cache,
    )?;
    Ok(augment_rank_with_priority_paths(&base, related_tests_abs))
}

fn write_jest_assets(tmp: &Path) -> Result<(PathBuf, PathBuf, PathBuf), RunError> {
    let reporter_path = coverage::write_asset(&tmp.join("reporter.cjs"), JEST_REPORTER_BYTES)?;
    let setup_path = coverage::write_asset(&tmp.join("setup.cjs"), JEST_SETUP_BYTES)?;
    let out_json_base = tmp.join(format!("jest-bridge-{}", std::process::id()));
    Ok((reporter_path, setup_path, out_json_base))
}

fn build_base_cmd_args(
    setup_path: &Path,
    reporter_path: &Path,
    name_pattern_only_for_discovery: bool,
) -> Vec<String> {
    let base_cmd_args: Vec<String> = vec![
        "--testLocationInResults".to_string(),
        "--setupFilesAfterEnv".to_string(),
        setup_path.to_string_lossy().to_string(),
        "--colors".to_string(),
        "--passWithNoTests".to_string(),
        "--verbose".to_string(),
        "--reporters".to_string(),
        reporter_path.to_string_lossy().to_string(),
        "--reporters".to_string(),
        "default".to_string(),
    ];
    if name_pattern_only_for_discovery {
        base_cmd_args
    } else {
        base_cmd_args
            .into_iter()
            .chain(std::iter::once("--runTestsByPath".to_string()))
            .collect::<Vec<_>>()
    }
}

fn aggregate_project_runs(
    per_project_results: Vec<project_run::ProjectRunOutput>,
) -> AggregatedProjectRuns {
    per_project_results.into_iter().fold(
        AggregatedProjectRuns {
            exit_code: 0,
            bridges: vec![],
            captured_stdout: vec![],
            captured_stderr: vec![],
            coverage_failure_lines: IndexSet::new(),
            raw_output_all: vec![],
        },
        |mut acc, result| {
            acc.exit_code = acc.exit_code.max(result.exit_code);
            acc.captured_stdout.extend(result.captured_stdout);
            acc.captured_stderr.extend(result.captured_stderr);
            result.coverage_failure_lines.into_iter().for_each(|ln| {
                acc.coverage_failure_lines.insert(ln);
            });
            acc.raw_output_all.push(result.raw_output);
            result.bridge.into_iter().for_each(|bridge| {
                acc.bridges.push(bridge);
            });
            acc
        },
    )
}

fn print_jest_run_output(
    repo_root: &Path,
    args: &ParsedArgs,
    directness_rank: &std::collections::BTreeMap<String, i64>,
    aggregated: &AggregatedProjectRuns,
) {
    let combined_raw = aggregated.raw_output_all.join("\n");
    match bridge::merge_bridge_json(&aggregated.bridges, directness_rank) {
        Some(merged) => {
            print_from_merged_bridge(
                repo_root,
                args,
                &merged,
                &combined_raw,
                aggregated.exit_code,
            );
        }
        None => {
            print_from_raw_output(repo_root, args, &combined_raw, aggregated);
        }
    }
}

fn print_from_merged_bridge(
    repo_root: &Path,
    args: &ParsedArgs,
    merged: &headlamp_core::test_model::TestRunModel,
    combined_raw: &str,
    exit_code: i32,
) {
    let ctx = make_ctx(
        repo_root,
        None,
        exit_code != 0,
        args.show_logs,
        args.editor_cmd.clone(),
    );
    let pretty = render_vitest_from_test_model(merged, &ctx, args.only_failures);
    let maybe_merged_text = if !args.only_failures && bridge::looks_sparse(&pretty) {
        let raw_also = headlamp_core::format::raw_jest::format_jest_output_vitest(
            combined_raw,
            &ctx,
            args.only_failures,
        );
        Some(bridge::merge_sparse_bridge_and_raw(&pretty, &raw_also))
    } else {
        None
    };
    let final_text = maybe_merged_text.as_deref().unwrap_or(&pretty);
    if !final_text.trim().is_empty() {
        println!("{final_text}");
    }
}

fn print_from_raw_output(
    repo_root: &Path,
    args: &ParsedArgs,
    combined_raw: &str,
    aggregated: &AggregatedProjectRuns,
) {
    let ctx = make_ctx(
        repo_root,
        None,
        combined_raw.contains("FAIL"),
        args.show_logs,
        args.editor_cmd.clone(),
    );
    let formatted = headlamp_core::format::raw_jest::format_jest_output_vitest(
        combined_raw,
        &ctx,
        args.only_failures,
    );
    if !formatted.trim().is_empty() {
        println!("{formatted}");
    } else {
        aggregated
            .captured_stdout
            .iter()
            .for_each(|line| println!("{line}"));
        aggregated
            .captured_stderr
            .iter()
            .for_each(|line| eprintln!("{line}"));
    }
}

fn maybe_collect_coverage(
    repo_root: &Path,
    args: &ParsedArgs,
    selection_paths_abs: &[String],
    aggregated: &AggregatedProjectRuns,
) -> Result<i32, RunError> {
    if !args.collect_coverage {
        return Ok(aggregated.exit_code);
    }
    coverage::collect_and_print_coverage(coverage::CollectCoverageArgs {
        repo_root,
        args,
        selection_paths_abs,
        coverage_failure_lines: &aggregated.coverage_failure_lines,
        exit_code: aggregated.exit_code,
    })
}
