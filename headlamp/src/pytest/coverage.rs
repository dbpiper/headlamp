use std::path::{Path, PathBuf};
use std::process::Command;

use headlamp_core::args::ParsedArgs;
use headlamp_core::coverage::istanbul_pretty::format_istanbul_pretty_from_lcov_report;
use headlamp_core::coverage::lcov::read_lcov_filtered_from_path;
use headlamp_core::coverage::model::apply_statement_totals_to_report;
use headlamp_core::coverage::print::PrintOpts;

use crate::run::RunError;

pub(super) fn maybe_collect_pytest_coverage(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    exit_code: i32,
) -> Result<i32, RunError> {
    if !args.collect_coverage {
        return Ok(exit_code);
    }
    let coverage_data_file = coverage_data_path_for_args(repo_root, args, session);
    if should_run_coveragepy_json(&coverage_data_file) {
        let _ = run_coveragepy_json_report(repo_root, args, session);
    }
    let lcov_path = lcov_path_for_args(repo_root, args, session);
    let Some(filtered) = read_lcov_filtered_from_path(
        repo_root,
        &lcov_path,
        &args.include_globs,
        &args.exclude_globs,
    ) else {
        return Ok(exit_code);
    };
    let filtered = augment_with_coveragepy_statement_totals(repo_root, args, session, filtered);
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
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    filtered: crate::coverage::model::CoverageReport,
) -> crate::coverage::model::CoverageReport {
    let totals = if args.keep_artifacts {
        crate::coverage::coveragepy_json::read_repo_coveragepy_json_statement_totals(repo_root)
    } else {
        crate::coverage::coveragepy_json::read_coveragepy_json_statement_totals_from_path(
            repo_root,
            &pytest_coverage_json_path(session),
        )
    };
    match totals.as_ref() {
        Some(statement_totals_by_path) => {
            apply_statement_totals_to_report(filtered, statement_totals_by_path)
        }
        None => filtered,
    }
}

fn run_coveragepy_json_report(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
) -> Result<(), RunError> {
    let out_path = if args.keep_artifacts {
        repo_root.join("coverage").join("coverage.json")
    } else {
        pytest_coverage_json_path(session)
    };
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(RunError::Io)?;
    }
    let python_bin = if cfg!(windows) {
        "python.exe"
    } else {
        "python"
    };
    let out_path_string = out_path.to_string_lossy().to_string();
    let coverage_data_path = coverage_data_path_for_args(repo_root, args, session);
    let status = Command::new(python_bin)
        .args(["-m", "coverage", "json", "-q", "-o"])
        .arg(out_path_string)
        .current_dir(repo_root)
        .env("COVERAGE_FILE", coverage_data_path.as_os_str())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| RunError::Io(std::io::Error::other(e.to_string())))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| RunError::CommandFailed {
            message: "python -m coverage json failed".to_string(),
        })
}

pub(crate) fn should_run_coveragepy_json(coverage_data_file: &Path) -> bool {
    std::fs::metadata(coverage_data_file).is_ok_and(|m| m.is_file() && m.len() > 0)
}

pub(crate) fn extract_lcov_report_paths(cmd_args: &[String]) -> Vec<PathBuf> {
    cmd_args
        .iter()
        .enumerate()
        .filter_map(|(index, arg)| {
            arg.strip_prefix("--cov-report=")
                .map(|v| v.to_string())
                .or_else(|| {
                    (arg == "--cov-report")
                        .then_some(index + 1)
                        .and_then(|next| cmd_args.get(next))
                        .cloned()
                })
        })
        .filter_map(|value| value.strip_prefix("lcov:").map(|v| v.to_string()))
        .map(PathBuf::from)
        .collect()
}

pub(crate) fn ensure_cov_report_output_directories(
    repo_root: &Path,
    cmd_args: &[String],
) -> Result<(), RunError> {
    extract_lcov_report_paths(cmd_args)
        .iter()
        .filter_map(|p| p.parent())
        .try_for_each(|parent| match parent.is_absolute() {
            true => std::fs::create_dir_all(parent).map_err(RunError::Io),
            false => std::fs::create_dir_all(repo_root.join(parent)).map_err(RunError::Io),
        })
}

pub(super) fn pytest_coverage_data_path(session: &crate::session::RunSession) -> PathBuf {
    session.subdir("coverage").join("pytest").join(".coverage")
}

fn pytest_coverage_json_path(session: &crate::session::RunSession) -> PathBuf {
    session
        .subdir("coverage")
        .join("pytest")
        .join("coverage.json")
}

pub(super) fn pytest_lcov_path(
    keep_artifacts: bool,
    session: &crate::session::RunSession,
) -> PathBuf {
    if keep_artifacts {
        PathBuf::from("coverage").join("lcov.info")
    } else {
        session.subdir("coverage").join("pytest").join("lcov.info")
    }
}

fn coverage_data_path_for_args(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
) -> PathBuf {
    if args.keep_artifacts {
        repo_root.join(".coverage")
    } else {
        pytest_coverage_data_path(session)
    }
}

fn lcov_path_for_args(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
) -> PathBuf {
    if args.keep_artifacts {
        repo_root.join("coverage").join("lcov.info")
    } else {
        pytest_lcov_path(false, session)
    }
}
