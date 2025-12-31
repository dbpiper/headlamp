use std::path::{Path, PathBuf};
use std::process::Command;

use headlamp_core::args::ParsedArgs;
use headlamp_core::format::ctx::make_ctx;
use headlamp_core::format::vitest::render_vitest_from_test_model;
use headlamp_core::test_model::{TestLocation, TestRunModel};
use once_cell::sync::Lazy;
use regex::Regex;

use crate::git::changed_files;
use crate::live_progress;
use crate::pytest_select::{changed_seeds, discover_pytest_test_files, filter_tests_by_seeds};
use crate::run::{RunError, run_bootstrap};
use crate::streaming::run_streaming_capture_tail_merged;

const PYTEST_PLUGIN_BYTES: &[u8] = include_bytes!("../assets/pytest/headlamp_pytest_plugin.py");

mod adapter;
pub(crate) mod coverage;
use adapter::PytestAdapter;

pub fn run_pytest(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
) -> Result<i32, RunError> {
    let started_at = std::time::Instant::now();
    let started_at_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    run_bootstrap_if_configured(repo_root, args)?;
    let selected = resolve_pytest_selection(repo_root, args)?;
    let pytest_bin = pytest_bin();
    let (_tmp, pythonpath) = setup_pytest_plugin(repo_root, session)?;
    let cmd_args = build_pytest_cmd_args(args, session, &selected);
    if args.collect_coverage {
        coverage::ensure_cov_report_output_directories(repo_root, &cmd_args)?;
    }
    let (exit_code, mut model) =
        run_pytest_streaming(repo_root, args, session, pytest_bin, cmd_args, pythonpath)?;
    apply_run_timing_to_model(
        &mut model,
        started_at_unix_ms,
        started_at.elapsed().as_millis() as u64,
    );
    maybe_print_rendered_pytest_run(repo_root, args, exit_code, &model);
    if args.coverage_abort_on_failure && exit_code != 0 {
        headlamp_core::diagnostics_trace::maybe_write_run_trace(
            repo_root,
            "pytest",
            args,
            Some(started_at),
            serde_json::json!({
                "pytest_bin": pytest_bin,
                "selected_count": selected.len(),
                "exit_code": exit_code,
                "coverage_aborted": true,
            }),
        );
        return Ok(exit_code);
    }
    let final_exit = coverage::maybe_collect_pytest_coverage(repo_root, args, session, exit_code)?;
    headlamp_core::diagnostics_trace::maybe_write_run_trace(
        repo_root,
        "pytest",
        args,
        Some(started_at),
        serde_json::json!({
            "pytest_bin": pytest_bin,
            "selected_count": selected.len(),
            "exit_code": final_exit,
            "coverage_aborted": false,
        }),
    );
    Ok(final_exit)
}

fn run_bootstrap_if_configured(repo_root: &Path, args: &ParsedArgs) -> Result<(), RunError> {
    args.bootstrap_command
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|cmd| run_bootstrap(repo_root, cmd))
        .unwrap_or(Ok(()))
}

fn pytest_bin() -> &'static str {
    cfg!(windows).then_some("pytest.exe").unwrap_or("pytest")
}

fn setup_pytest_plugin(
    repo_root: &Path,
    session: &crate::session::RunSession,
) -> Result<(PathBuf, String), RunError> {
    let tmp = session.subdir("pytest");
    let _plugin_path = write_asset(&tmp.join("headlamp_pytest_plugin.py"), PYTEST_PLUGIN_BYTES)?;
    let pythonpath = crate::pythonpath::build_pytest_pythonpath(
        repo_root,
        &[tmp.as_path()],
        std::env::var("PYTHONPATH").ok(),
    );
    Ok((tmp, pythonpath))
}

pub(crate) fn build_pytest_cmd_args(
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    selected: &[String],
) -> Vec<String> {
    let mut cmd_args: Vec<String> = vec![
        "-p".to_string(),
        "headlamp_pytest_plugin".to_string(),
        "--tb=long".to_string(),
        "--no-header".to_string(),
        "--no-summary".to_string(),
        "-q".to_string(),
    ];
    if !args.keep_artifacts {
        cmd_args.push("-p".to_string());
        cmd_args.push("no:cacheprovider".to_string());
    }
    cmd_args.extend(rewrite_pytest_runner_args_for_no_artifacts(args, session));
    cmd_args.extend(selected.iter().cloned());
    let has_cov = args.runner_args.iter().any(|a| a.starts_with("--cov"));
    if args.collect_coverage {
        let has_lcov_report = cmd_args.iter().any(|a| a.starts_with("--cov-report=lcov:"))
            || cmd_args
                .windows(2)
                .any(|w| w[0] == "--cov-report" && w[1].starts_with("lcov:"));
        if !has_cov {
            cmd_args.push("--cov=.".to_string());
        }
        if !has_lcov_report {
            let lcov_path = coverage::pytest_lcov_path(args.keep_artifacts, session);
            cmd_args.push(format!("--cov-report=lcov:{}", lcov_path.to_string_lossy()));
        }
    }
    cmd_args
}

fn run_pytest_streaming(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    pytest_bin: &str,
    cmd_args: Vec<String>,
    pythonpath: String,
) -> Result<(i32, TestRunModel), RunError> {
    let mode = live_progress::live_progress_mode(
        headlamp_core::format::terminal::is_output_terminal(),
        args.ci,
    );
    let live_progress = live_progress::LiveProgress::start(1, mode);
    let mut command = Command::new(pytest_bin);
    command
        .args(cmd_args)
        .current_dir(repo_root)
        .env("CI", "1")
        .env("PYTHONPATH", pythonpath);
    if args.collect_coverage && !args.keep_artifacts {
        let coverage_data_path = coverage::pytest_coverage_data_path(session);
        command.env("COVERAGE_FILE", coverage_data_path.as_os_str());
    }
    if !args.keep_artifacts {
        command.env("PYTHONDONTWRITEBYTECODE", "1");
    }
    let mut adapter = PytestAdapter::new(args.show_logs, args.ci);
    let (exit_code, _tail) =
        run_streaming_capture_tail_merged(command, &live_progress, &mut adapter, 1024 * 1024)?;
    live_progress.increment_done(1);
    live_progress.finish();
    let model = adapter.finalize(exit_code);
    Ok((exit_code, model))
}

fn maybe_print_rendered_pytest_run(
    repo_root: &Path,
    args: &ParsedArgs,
    exit_code: i32,
    model: &TestRunModel,
) {
    let ctx = make_ctx(
        repo_root,
        None,
        exit_code != 0,
        args.show_logs,
        args.editor_cmd.clone(),
    );
    let rendered = render_vitest_from_test_model(model, &ctx, args.only_failures);
    (!rendered.trim().is_empty()).then(|| println!("{rendered}"));
}

pub(crate) fn apply_run_timing_to_model(
    model: &mut TestRunModel,
    started_at_unix_ms: u64,
    elapsed_ms: u64,
) {
    model.start_time = started_at_unix_ms;
    model.aggregated.start_time = started_at_unix_ms;
    model.aggregated.run_time_ms = Some(elapsed_ms);
}

pub(crate) fn infer_test_location_from_pytest_longrepr(
    nodeid_file: &str,
    longrepr: &str,
) -> Option<TestLocation> {
    static PY_FILE_LINE_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"^\s*File\s+"([^"]+)",\s+line\s+(\d+)(?:,|$)"#).unwrap());
    static PY_PATH_COLON_LINE_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"^\s*([^:\s]+):(\d+):\s*"#).unwrap());
    let needle = nodeid_file.replace('\\', "/");
    longrepr.lines().find_map(|line| {
        PY_FILE_LINE_RE
            .captures(line)
            .and_then(|caps| {
                let file = caps.get(1)?.as_str().replace('\\', "/");
                let line_number = caps.get(2)?.as_str().parse::<i64>().ok()?;
                let matches_file = file.ends_with(&needle) || needle.ends_with(&file);
                (matches_file && line_number > 0).then_some(TestLocation {
                    line: line_number,
                    column: 1,
                })
            })
            .or_else(|| {
                let caps = PY_PATH_COLON_LINE_RE.captures(line)?;
                let file = caps.get(1)?.as_str().replace('\\', "/");
                let line_number = caps.get(2)?.as_str().parse::<i64>().ok()?;
                let matches_file = file.ends_with(&needle) || needle.ends_with(&file);
                (matches_file && line_number > 0).then_some(TestLocation {
                    line: line_number,
                    column: 1,
                })
            })
    })
}

fn write_asset(path: &Path, bytes: &[u8]) -> Result<String, RunError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(RunError::Io)?;
    }
    std::fs::write(path, bytes).map_err(RunError::Io)?;
    Ok(path.to_string_lossy().to_string())
}

fn rewrite_pytest_runner_args_for_no_artifacts(
    args: &ParsedArgs,
    session: &crate::session::RunSession,
) -> Vec<String> {
    if args.keep_artifacts {
        return args.runner_args.to_vec();
    }
    let lcov_path = coverage::pytest_lcov_path(false, session);
    let lcov_value = format!("lcov:{}", lcov_path.to_string_lossy());
    let mut rewritten: Vec<String> = vec![];
    let mut iter = args.runner_args.iter().peekable();
    while let Some(token) = iter.next() {
        if let Some(_old) = token.strip_prefix("--cov-report=lcov:") {
            rewritten.push(format!("--cov-report={}", lcov_value));
            continue;
        }
        if token.as_str() == "--cov-report"
            && let Some(next) = iter.peek()
            && next.starts_with("lcov:")
        {
            let _ = iter.next();
            rewritten.push("--cov-report".to_string());
            rewritten.push(lcov_value.clone());
            continue;
        }
        rewritten.push(token.clone());
    }
    rewritten
}

fn resolve_pytest_selection(repo_root: &Path, args: &ParsedArgs) -> Result<Vec<String>, RunError> {
    let changed = args
        .changed
        .map(|m| changed_files(repo_root, m))
        .transpose()?
        .unwrap_or_default();

    let all_tests = discover_pytest_test_files(repo_root, args.no_cache)?;
    let all_tests_set = all_tests
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<std::collections::BTreeSet<_>>();

    let (explicit, seed_inputs) = args
        .selection_paths
        .iter()
        .filter(|token| {
            token.ends_with(".py")
                || token.contains('/')
                || token.contains('\\')
                || token.contains("::")
        })
        .filter_map(|token| {
            let file_part = token.split("::").next().unwrap_or(token.as_str());
            let abs = repo_root.join(file_part);
            abs.exists().then_some((token, abs))
        })
        .fold(
            (Vec::<String>::new(), Vec::<PathBuf>::new()),
            |(mut explicit_acc, mut seeds_acc), (token, abs)| {
                let abs_key = abs.to_string_lossy().to_string();
                if all_tests_set.contains(&abs_key) {
                    explicit_acc.push((*token).clone());
                } else {
                    seeds_acc.push(abs);
                }
                (explicit_acc, seeds_acc)
            },
        );

    if !explicit.is_empty() {
        return Ok(explicit);
    }
    if !seed_inputs.is_empty() {
        let seeds = changed_seeds(repo_root, &seed_inputs);
        let kept = filter_tests_by_seeds(&all_tests, &seeds);
        return Ok(kept
            .into_iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect());
    }
    if changed.is_empty() {
        return Ok(all_tests
            .into_iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect());
    }

    let seeds = changed_seeds(repo_root, &changed);
    let kept = filter_tests_by_seeds(&all_tests, &seeds);

    Ok(kept
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect())
}
