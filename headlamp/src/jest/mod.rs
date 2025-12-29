use std::path::Path;

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

pub fn run_jest(repo_root: &Path, args: &ParsedArgs) -> Result<i32, RunError> {
    if let Some(cmd) = args
        .bootstrap_command
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        run_bootstrap(repo_root, cmd)?;
    }

    let jest_bin = jest_bin(repo_root);
    if !jest_bin.exists() {
        return Err(RunError::MissingRunner {
            runner: "jest".to_string(),
            hint: format!("expected {}", jest_bin.display()),
        });
    }

    let selection_paths_abs = selection::selection_paths_abs(repo_root, args)?;
    let discovery_args = args_for_discovery(&args.runner_args);

    let discovered_project_configs = list_all_jest_configs(repo_root);
    let project_configs = if discovered_project_configs.is_empty() {
        vec![repo_root.to_path_buf()]
    } else {
        discovered_project_configs
    };

    let selection_exclude_globs = selection::exclude_globs_for_selection(&args.exclude_globs);

    let selection_is_tests_only = !selection_paths_abs.is_empty()
        && selection_paths_abs
            .iter()
            .all(|abs| selection::looks_like_test_path(abs));

    let production_seeds = selection_paths_abs
        .iter()
        .filter(|abs| !selection::looks_like_test_path(abs))
        .cloned()
        .collect::<Vec<_>>();

    let selection_key = (!selection_paths_abs.is_empty() && !selection_is_tests_only).then(|| {
        production_seeds
            .iter()
            .map(|abs| {
                Path::new(abs)
                    .strip_prefix(repo_root)
                    .ok()
                    .map(|p| p.to_slash_lossy().to_string())
                    .unwrap_or_else(|| Path::new(abs).to_slash_lossy().to_string())
            })
            .collect::<Vec<_>>()
            .join("|")
    });

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

    let directness_rank_base = selection::compute_directness_rank_base(
        repo_root,
        &selection_paths_abs,
        &selection_exclude_globs,
        args.no_cache,
    )?;
    let directness_rank = augment_rank_with_priority_paths(
        &directness_rank_base,
        &related_selection.selected_test_paths_abs,
    );

    let tmp = std::env::temp_dir().join("headlamp").join("jest");
    let reporter_path = coverage::write_asset(&tmp.join("reporter.cjs"), JEST_REPORTER_BYTES)?;
    let setup_path = coverage::write_asset(&tmp.join("setup.cjs"), JEST_SETUP_BYTES)?;
    let out_json_base = tmp.join(format!("jest-bridge-{}", std::process::id()));

    let name_pattern_only_for_discovery =
        bridge::should_skip_run_tests_by_path_for_name_pattern_only(args, &selection_paths_abs);

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
    let base_cmd_args = if name_pattern_only_for_discovery {
        base_cmd_args
    } else {
        base_cmd_args
            .iter()
            .cloned()
            .chain(std::iter::once("--runTestsByPath".to_string()))
            .collect::<Vec<_>>()
    };

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

    let mut exit_codes: Vec<i32> = vec![];
    let mut bridges: Vec<headlamp_core::test_model::TestRunModel> = vec![];
    let mut captured_stdout_all: Vec<String> = vec![];
    let mut captured_stderr_all: Vec<String> = vec![];
    let mut coverage_failure_lines: IndexSet<String> = IndexSet::new();
    let mut raw_output_all: Vec<String> = vec![];
    for result in per_project_results {
        exit_codes.push(result.exit_code);
        captured_stdout_all.extend(result.captured_stdout);
        captured_stderr_all.extend(result.captured_stderr);
        result.coverage_failure_lines.into_iter().for_each(|ln| {
            coverage_failure_lines.insert(ln);
        });
        raw_output_all.push(result.raw_output);
        if let Some(bridge) = result.bridge {
            bridges.push(bridge);
        }
    }

    let mut exit_code = exit_codes.into_iter().max().unwrap_or(1);

    if let Some(merged) = bridge::merge_bridge_json(&bridges, &directness_rank) {
        let ctx = make_ctx(
            repo_root,
            None,
            exit_code != 0,
            args.show_logs,
            args.editor_cmd.clone(),
        );
        let pretty = render_vitest_from_test_model(&merged, &ctx, args.only_failures);
        let combined = raw_output_all.join("\n");
        let maybe_merged = (!args.only_failures && bridge::looks_sparse(&pretty)).then(|| {
            let raw_also = headlamp_core::format::raw_jest::format_jest_output_vitest(
                &combined,
                &ctx,
                args.only_failures,
            );
            bridge::merge_sparse_bridge_and_raw(&pretty, &raw_also)
        });
        let final_text = maybe_merged.as_deref().unwrap_or(&pretty);
        if !final_text.trim().is_empty() {
            println!("{final_text}");
        }
    } else {
        let combined = raw_output_all.join("\n");
        let ctx = make_ctx(
            repo_root,
            None,
            combined.contains("FAIL"),
            args.show_logs,
            args.editor_cmd.clone(),
        );
        let formatted = headlamp_core::format::raw_jest::format_jest_output_vitest(
            &combined,
            &ctx,
            args.only_failures,
        );
        if !formatted.trim().is_empty() {
            println!("{formatted}");
        } else {
            for line in captured_stdout_all {
                println!("{line}");
            }
            for line in captured_stderr_all {
                eprintln!("{line}");
            }
        }
    }

    if args.collect_coverage {
        let coverage_outcome =
            coverage::collect_and_print_coverage(coverage::CollectCoverageArgs {
                repo_root,
                args,
                selection_paths_abs: &selection_paths_abs,
                coverage_failure_lines: &coverage_failure_lines,
                exit_code,
            })?;
        exit_code = coverage_outcome;
    }

    Ok(exit_code)
}
