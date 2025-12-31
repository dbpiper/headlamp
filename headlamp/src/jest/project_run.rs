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
    collect_coverage_from_args, coverage_dir_for_config_in_root,
    ensure_watchman_disabled_by_default, extract_coverage_failure_lines,
};
use super::streaming::merge_console_entries_into_bridge_json;

#[derive(Debug)]
struct RunProjectContext<'a> {
    repo_root: &'a Path,
    args: &'a ParsedArgs,
    jest_bin: &'a Path,
    discovery_args: &'a [String],
    related_selection: &'a [String],
    base_cmd_args: &'a [String],
    selection_paths_abs: &'a [String],
    name_pattern_only_for_discovery: bool,
    out_json_base: &'a Path,
    coverage_root: &'a Path,
}

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
    pub(super) coverage_root: &'a Path,
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
        coverage_root,
        mode,
    } = args;

    let stride = if args.sequential { 1 } else { 3 };
    let live_progress = LiveProgress::start(project_configs.len(), mode);
    let ctx = RunProjectContext {
        repo_root,
        args,
        jest_bin,
        discovery_args,
        related_selection,
        base_cmd_args,
        selection_paths_abs,
        name_pattern_only_for_discovery,
        out_json_base,
        coverage_root,
    };
    let per_project_results = run_parallel_stride(project_configs, stride, |cfg_path, index| {
        run_project_for_config(&ctx, &live_progress, cfg_path, index)
    })?;
    live_progress.finish();
    Ok(per_project_results)
}

fn run_project_for_config(
    ctx: &RunProjectContext<'_>,
    live_progress: &LiveProgress,
    cfg_path: &Path,
    index: usize,
) -> Result<ProjectRunOutput, RunError> {
    let cfg_token = config_token(ctx.repo_root, cfg_path);
    live_progress.set_current_label(cfg_token.clone());
    let tests_for_project = tests_for_project(ctx, cfg_path, &cfg_token)?;
    if should_skip_project(
        ctx.selection_paths_abs,
        &tests_for_project,
        ctx.name_pattern_only_for_discovery,
    ) {
        live_progress.increment_done(1);
        return Ok(empty_project_output());
    }
    let out_json = ctx.out_json_base.with_extension(format!("{index}.json"));
    let cmd_args = build_cmd_args(ctx, cfg_path, &cfg_token, &tests_for_project);
    let run = execute_jest_for_project(ctx, live_progress, &out_json, cmd_args)?;
    Ok(ProjectRunOutput {
        exit_code: run.exit_code,
        bridge: run.bridge,
        captured_stdout: run.captured_stdout,
        captured_stderr: run.captured_stderr,
        coverage_failure_lines: run.coverage_failure_lines,
        raw_output: run.raw_output,
    })
}

#[derive(Debug)]
struct ProjectExecution {
    exit_code: i32,
    bridge: Option<TestRunModel>,
    captured_stdout: Vec<String>,
    captured_stderr: Vec<String>,
    coverage_failure_lines: Vec<String>,
    raw_output: String,
}

fn tests_for_project(
    ctx: &RunProjectContext<'_>,
    cfg_path: &Path,
    cfg_token: &str,
) -> Result<Vec<String>, RunError> {
    if ctx.selection_paths_abs.is_empty() {
        return list_all_tests_for_project(ctx, cfg_path, cfg_token);
    }
    filter_candidates_for_project(
        ctx.repo_root,
        ctx.jest_bin,
        ctx.discovery_args,
        cfg_path,
        ctx.related_selection,
    )
}

fn list_all_tests_for_project(
    ctx: &RunProjectContext<'_>,
    cfg_path: &Path,
    cfg_token: &str,
) -> Result<Vec<String>, RunError> {
    if ctx.name_pattern_only_for_discovery {
        return Ok(vec![]);
    }
    let mut list_args = ctx.discovery_args.to_vec();
    list_args.extend(["--config".to_string(), cfg_token.to_string()]);
    discover_jest_list_tests_cached_with_timeout(
        cfg_path.parent().unwrap_or(ctx.repo_root),
        ctx.jest_bin,
        &list_args,
        ctx.args.no_cache,
        JEST_LIST_TESTS_TIMEOUT,
    )
}

fn should_skip_project(
    selection_paths_abs: &[String],
    tests_for_project: &[String],
    name_pattern_only_for_discovery: bool,
) -> bool {
    tests_for_project.is_empty()
        && (!selection_paths_abs.is_empty() || !name_pattern_only_for_discovery)
}

fn empty_project_output() -> ProjectRunOutput {
    ProjectRunOutput {
        exit_code: 0,
        bridge: None,
        captured_stdout: vec![],
        captured_stderr: vec![],
        coverage_failure_lines: vec![],
        raw_output: String::new(),
    }
}

fn build_cmd_args(
    ctx: &RunProjectContext<'_>,
    cfg_path: &Path,
    cfg_token: &str,
    tests_for_project: &[String],
) -> Vec<String> {
    let mut cmd_args = ctx.base_cmd_args.to_vec();
    cmd_args.extend(["--config".to_string(), cfg_token.to_string()]);
    cmd_args.extend(ctx.args.runner_args.iter().cloned());
    ensure_watchman_disabled_by_default(&mut cmd_args);
    append_cache_and_execution_flags(&mut cmd_args, ctx.args);
    append_coverage_flags(&mut cmd_args, cfg_path, ctx);
    ctx.args
        .show_logs
        .then(|| cmd_args.push("--no-silent".to_string()));
    append_test_selection_args(&mut cmd_args, ctx, tests_for_project);
    cmd_args
}

fn append_cache_and_execution_flags(cmd_args: &mut Vec<String>, args: &ParsedArgs) {
    if args.no_cache && !cmd_args.iter().any(|t| t == "--no-cache") {
        cmd_args.push("--no-cache".to_string());
    }
    if args.sequential {
        cmd_args.push("--runInBand".to_string());
    }
}

fn append_coverage_flags(cmd_args: &mut Vec<String>, cfg_path: &Path, ctx: &RunProjectContext<'_>) {
    if !ctx.args.collect_coverage {
        return;
    }
    let has_coverage_arg = cmd_args
        .iter()
        .any(|t| t == "--coverage" || t.starts_with("--coverage="));
    (!has_coverage_arg).then(|| {
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
    });
    cmd_args.push(format!(
        "--coverageDirectory={}",
        coverage_dir_for_config_in_root(cfg_path, ctx.coverage_root).to_string_lossy()
    ));
    cmd_args.extend(collect_coverage_from_args(
        ctx.repo_root,
        ctx.selection_paths_abs,
        &ctx.args.selection_paths,
    ));
}

fn append_test_selection_args(
    cmd_args: &mut Vec<String>,
    ctx: &RunProjectContext<'_>,
    tests_for_project: &[String],
) {
    if !tests_for_project.is_empty() {
        cmd_args.extend(tests_for_project.iter().cloned());
        return;
    }
    if !ctx.name_pattern_only_for_discovery {
        cmd_args.extend(ctx.args.selection_paths.iter().cloned());
    }
}

fn execute_jest_for_project(
    ctx: &RunProjectContext<'_>,
    live_progress: &LiveProgress,
    out_json: &Path,
    cmd_args: Vec<String>,
) -> Result<ProjectExecution, RunError> {
    let emit_raw_lines = ctx.args.ci;
    let mut command = std::process::Command::new(ctx.jest_bin);
    command
        .args(cmd_args)
        .current_dir(ctx.repo_root)
        .env("NODE_ENV", "test")
        .env("FORCE_COLOR", "3")
        .env("JEST_BRIDGE_OUT", out_json.to_string_lossy().to_string());
    let mut adapter = super::streaming::JestStreamingAdapter::new(emit_raw_lines);
    let (exit_code, _tail) =
        run_streaming_capture_tail(command, live_progress, &mut adapter, 1024 * 1024)?;
    build_project_execution(
        exit_code,
        ctx.name_pattern_only_for_discovery,
        out_json,
        adapter,
    )
}

fn build_project_execution(
    exit_code: i32,
    name_pattern_only_for_discovery: bool,
    out_json: &Path,
    adapter: super::streaming::JestStreamingAdapter,
) -> Result<ProjectExecution, RunError> {
    let captured_stdout = adapter.captured_stdout;
    let captured_stderr = adapter.captured_stderr;
    let extra_bridge_entries_by_test_path = adapter.extra_bridge_entries_by_test_path;
    let raw_output = format!(
        "{}\n{}",
        captured_stdout.join("\n"),
        captured_stderr.join("\n")
    );
    let coverage_failure_lines = extract_coverage_failure_lines(raw_output.as_bytes(), b"");
    let bridge = std::fs::read_to_string(out_json)
        .ok()
        .and_then(|raw| serde_json::from_str::<TestRunModel>(&raw).ok())
        .map(|mut bridge| {
            merge_console_entries_into_bridge_json(&mut bridge, &extra_bridge_entries_by_test_path);
            if name_pattern_only_for_discovery {
                bridge = filter_bridge_for_name_pattern_only(bridge);
            }
            bridge
        });

    Ok(ProjectExecution {
        exit_code,
        bridge,
        captured_stdout,
        captured_stderr,
        coverage_failure_lines,
        raw_output,
    })
}
