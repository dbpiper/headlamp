use std::path::{Path, PathBuf};
use std::process::Command;

use headlamp_core::args::ParsedArgs;
use headlamp_core::coverage::istanbul_pretty::format_istanbul_pretty_from_lcov_report;
use headlamp_core::coverage::lcov::read_repo_lcov_filtered;
use headlamp_core::coverage::model::apply_statement_totals_to_report;
use headlamp_core::coverage::print::PrintOpts;
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
use adapter::PytestAdapter;

pub fn run_pytest(repo_root: &Path, args: &ParsedArgs) -> Result<i32, RunError> {
    let started_at = std::time::Instant::now();
    run_bootstrap_if_configured(repo_root, args)?;
    let selected = resolve_pytest_selection(repo_root, args)?;
    let pytest_bin = pytest_bin();
    let (_tmp, pythonpath) = setup_pytest_plugin(repo_root)?;
    let cmd_args = build_pytest_cmd_args(args, &selected);
    let (exit_code, model) =
        run_pytest_streaming(repo_root, args, pytest_bin, cmd_args, pythonpath)?;
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
    let final_exit = maybe_collect_pytest_coverage(repo_root, args, exit_code)?;
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

fn setup_pytest_plugin(repo_root: &Path) -> Result<(PathBuf, String), RunError> {
    let tmp = std::env::temp_dir().join("headlamp").join("pytest");
    let _plugin_path = write_asset(&tmp.join("headlamp_pytest_plugin.py"), PYTEST_PLUGIN_BYTES)?;
    let pythonpath = crate::pythonpath::build_pytest_pythonpath(
        repo_root,
        &[tmp.as_path()],
        std::env::var("PYTHONPATH").ok(),
    );
    Ok((tmp, pythonpath))
}

fn build_pytest_cmd_args(args: &ParsedArgs, selected: &[String]) -> Vec<String> {
    let mut cmd_args: Vec<String> = vec![
        "-p".to_string(),
        "headlamp_pytest_plugin".to_string(),
        "--tb=long".to_string(),
        "--no-header".to_string(),
        "--no-summary".to_string(),
        "-q".to_string(),
    ];
    cmd_args.extend(args.runner_args.iter().cloned());
    cmd_args.extend(selected.iter().cloned());
    let has_cov = args.runner_args.iter().any(|a| a.starts_with("--cov"));
    if args.collect_coverage && !has_cov {
        cmd_args.push("--cov=.".to_string());
        cmd_args.push("--cov-report=lcov:coverage/lcov.info".to_string());
    }
    cmd_args
}

fn run_pytest_streaming(
    repo_root: &Path,
    args: &ParsedArgs,
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

fn maybe_collect_pytest_coverage(
    repo_root: &Path,
    args: &ParsedArgs,
    exit_code: i32,
) -> Result<i32, RunError> {
    if !args.collect_coverage {
        return Ok(exit_code);
    }
    let _ = run_coveragepy_json_report(repo_root);
    let Some(filtered) =
        read_repo_lcov_filtered(repo_root, &args.include_globs, &args.exclude_globs)
    else {
        return Ok(exit_code);
    };
    let filtered = augment_with_coveragepy_statement_totals(repo_root, filtered);
    let print_opts =
        PrintOpts::for_run(args, headlamp_core::format::terminal::is_output_terminal());
    let threshold_failure_lines = args.coverage_thresholds.as_ref().map(|thresholds| {
        headlamp_core::coverage::thresholds::threshold_failure_lines(
            thresholds,
            headlamp_core::coverage::thresholds::compute_totals_from_report(&filtered),
        )
    });
    let pretty = format_istanbul_pretty_from_lcov_report(
        repo_root,
        filtered,
        &print_opts,
        &[],
        &args.include_globs,
        &args.exclude_globs,
        args.coverage_detail,
    );
    if args.coverage_ui != headlamp_core::config::CoverageUi::Jest {
        println!("{pretty}");
    }
    let thresholds_failed = threshold_failure_lines.is_some_and(|lines| {
        if lines.is_empty() {
            return false;
        }
        headlamp_core::coverage::thresholds::print_threshold_failure_summary(&lines);
        true
    });
    Ok(if exit_code == 0 && thresholds_failed {
        1
    } else {
        exit_code
    })
}

fn augment_with_coveragepy_statement_totals(
    repo_root: &Path,
    filtered: crate::coverage::model::CoverageReport,
) -> crate::coverage::model::CoverageReport {
    match crate::coverage::coveragepy_json::read_repo_coveragepy_json_statement_totals(repo_root)
        .as_ref()
    {
        Some(statement_totals_by_path) => {
            apply_statement_totals_to_report(filtered, statement_totals_by_path)
        }
        None => filtered,
    }
}

fn run_coveragepy_json_report(repo_root: &Path) -> Result<(), RunError> {
    let python_bin = if cfg!(windows) {
        "python.exe"
    } else {
        "python"
    };
    let status = Command::new(python_bin)
        .args([
            "-m",
            "coverage",
            "json",
            "-q",
            "-o",
            "coverage/coverage.json",
        ])
        .current_dir(repo_root)
        .status()
        .map_err(|e| RunError::Io(std::io::Error::other(e.to_string())))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| RunError::CommandFailed {
            message: "python -m coverage json failed".to_string(),
        })
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
