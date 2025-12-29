use std::path::{Path, PathBuf};

use headlamp_core::args::ParsedArgs;
use headlamp_core::test_model::TestRunModel;

use crate::jest_discovery::{
    JEST_LIST_TESTS_TIMEOUT, discover_jest_list_tests_cached_with_timeout,
};
use crate::jest_ownership::filter_candidates_for_project;
use crate::live_progress::{LiveProgress, LiveProgressMode};
use crate::parallel_stride::run_parallel_stride;
use crate::run::RunError;
use crate::streaming::run_streaming_capture_tail;

use super::bridge::{config_token, filter_bridge_for_name_pattern_only};
use super::coverage::{
    collect_coverage_from_args, coverage_dir_for_config, ensure_watchman_disabled_by_default,
    extract_coverage_failure_lines,
};
use super::streaming::merge_console_entries_into_bridge_json;

#[derive(Debug)]
pub(super) struct ProjectRunOutput {
    pub(super) exit_code: i32,
    pub(super) bridge: Option<TestRunModel>,
    pub(super) captured_stdout: Vec<String>,
    pub(super) captured_stderr: Vec<String>,
    pub(super) coverage_failure_lines: Vec<String>,
    pub(super) raw_output: String,
}

#[derive(Debug)]
pub(super) struct RunProjectsArgs<'a> {
    pub(super) repo_root: &'a Path,
    pub(super) args: &'a ParsedArgs,
    pub(super) project_configs: &'a [PathBuf],
    pub(super) jest_bin: &'a Path,
    pub(super) discovery_args: &'a [String],
    pub(super) related_selection: &'a [String],
    pub(super) base_cmd_args: &'a [String],
    pub(super) selection_paths_abs: &'a [String],
    pub(super) name_pattern_only_for_discovery: bool,
    pub(super) out_json_base: &'a Path,
    pub(super) mode: LiveProgressMode,
}

pub(super) fn run_projects(args: RunProjectsArgs<'_>) -> Result<Vec<ProjectRunOutput>, RunError> {
    let RunProjectsArgs {
        repo_root,
        args,
        project_configs,
        jest_bin,
        discovery_args,
        related_selection,
        base_cmd_args,
        selection_paths_abs,
        name_pattern_only_for_discovery,
        out_json_base,
        mode,
    } = args;

    let stride = if args.sequential { 1 } else { 3 };
    let live_progress = LiveProgress::start(project_configs.len(), mode);
    let per_project_results = run_parallel_stride(project_configs, stride, |cfg_path, index| {
        let cfg_token = config_token(repo_root, cfg_path);
        live_progress.set_current_label(cfg_token.clone());

        let tests_for_project = if selection_paths_abs.is_empty() {
            if name_pattern_only_for_discovery {
                vec![]
            } else {
                let mut list_args = discovery_args.to_vec();
                list_args.extend(["--config".to_string(), cfg_token.clone()]);
                discover_jest_list_tests_cached_with_timeout(
                    cfg_path.parent().unwrap_or(repo_root),
                    jest_bin,
                    &list_args,
                    args.no_cache,
                    JEST_LIST_TESTS_TIMEOUT,
                )?
            }
        } else {
            filter_candidates_for_project(
                repo_root,
                jest_bin,
                discovery_args,
                cfg_path,
                related_selection,
            )?
        };

        if selection_paths_abs.is_empty()
            && tests_for_project.is_empty()
            && !name_pattern_only_for_discovery
        {
            live_progress.increment_done(1);
            return Ok(ProjectRunOutput {
                exit_code: 0,
                bridge: None,
                captured_stdout: vec![],
                captured_stderr: vec![],
                coverage_failure_lines: vec![],
                raw_output: String::new(),
            });
        }

        if !selection_paths_abs.is_empty() && tests_for_project.is_empty() {
            live_progress.increment_done(1);
            return Ok(ProjectRunOutput {
                exit_code: 0,
                bridge: None,
                captured_stdout: vec![],
                captured_stderr: vec![],
                coverage_failure_lines: vec![],
                raw_output: String::new(),
            });
        }

        let out_json = out_json_base.with_extension(format!("{index}.json"));

        let mut cmd_args = base_cmd_args.to_vec();
        cmd_args.extend(["--config".to_string(), cfg_token.clone()]);
        cmd_args.extend(args.runner_args.iter().cloned());
        ensure_watchman_disabled_by_default(&mut cmd_args);
        if args.no_cache && !cmd_args.iter().any(|t| t == "--no-cache") {
            cmd_args.push("--no-cache".to_string());
        }
        if args.sequential {
            cmd_args.push("--runInBand".to_string());
        }
        if args.collect_coverage {
            if !cmd_args
                .iter()
                .any(|t| t == "--coverage" || t.starts_with("--coverage="))
            {
                cmd_args.extend(
                    [
                        "--coverage",
                        "--coverageProvider=babel",
                        "--coverageReporters=lcov",
                        "--coverageReporters=json",
                        "--coverageReporters=text-summary",
                    ]
                    .into_iter()
                    .map(String::from),
                );
            }
            cmd_args.push(format!(
                "--coverageDirectory={}",
                coverage_dir_for_config(cfg_path)
            ));
            cmd_args.extend(collect_coverage_from_args(
                repo_root,
                selection_paths_abs,
                &args.selection_paths,
            ));
        }
        if args.show_logs {
            cmd_args.push("--no-silent".to_string());
        }
        if !tests_for_project.is_empty() {
            cmd_args.extend(tests_for_project);
        } else if !name_pattern_only_for_discovery {
            cmd_args.extend(args.selection_paths.iter().cloned());
        }

        let emit_raw_lines = args.ci;
        let mut command = std::process::Command::new(jest_bin);
        command
            .args(cmd_args)
            .current_dir(repo_root)
            .env("NODE_ENV", "test")
            .env("FORCE_COLOR", "3")
            .env("JEST_BRIDGE_OUT", out_json.to_string_lossy().to_string());
        let mut adapter = super::streaming::JestStreamingAdapter::new(emit_raw_lines);
        let (exit_code, _tail) =
            run_streaming_capture_tail(command, &live_progress, &mut adapter, 1024 * 1024)?;

        let captured_stdout = adapter.captured_stdout;
        let captured_stderr = adapter.captured_stderr;
        let extra_bridge_entries_by_test_path = adapter.extra_bridge_entries_by_test_path;
        let raw_output = format!(
            "{}\n{}",
            captured_stdout.join("\n"),
            captured_stderr.join("\n")
        );
        let coverage_failure_lines = extract_coverage_failure_lines(raw_output.as_bytes(), b"");

        let bridge = std::fs::read_to_string(&out_json)
            .ok()
            .and_then(|raw| serde_json::from_str::<TestRunModel>(&raw).ok())
            .map(|mut bridge| {
                merge_console_entries_into_bridge_json(
                    &mut bridge,
                    &extra_bridge_entries_by_test_path,
                );
                if name_pattern_only_for_discovery {
                    bridge = filter_bridge_for_name_pattern_only(bridge);
                }
                bridge
            });

        Ok(ProjectRunOutput {
            exit_code,
            bridge,
            captured_stdout,
            captured_stderr,
            coverage_failure_lines,
            raw_output,
        })
    })?;
    live_progress.finish();
    Ok(per_project_results)
}
