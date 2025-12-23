use std::path::Path;

use duct::cmd as duct_cmd;

use headlamp_core::args::ParsedArgs;
use headlamp_core::coverage::lcov::{merge_reports, read_lcov_file, resolve_lcov_paths_to_root};
use headlamp_core::coverage::print::{
    PrintOpts, filter_report, format_compact, format_hotspots, format_summary,
};

use crate::git::changed_files;
use crate::pytest_select::{changed_seeds, filter_tests_by_seeds, list_pytest_files};
use crate::run::RunError;

pub fn run_pytest(repo_root: &Path, args: &ParsedArgs) -> Result<i32, RunError> {
    let selected = resolve_pytest_selection(repo_root, args)?;

    let pytest_bin = if cfg!(windows) {
        "pytest.exe"
    } else {
        "pytest"
    };
    let mut cmd_args: Vec<String> = vec![];
    cmd_args.extend(args.runner_args.iter().cloned());
    cmd_args.extend(selected.iter().cloned());
    if args.collect_coverage && !args.runner_args.iter().any(|a| a.starts_with("--cov")) {
        cmd_args.push("--cov".to_string());
        cmd_args.push("--cov-report=lcov:coverage/lcov.info".to_string());
    }
    let out = duct_cmd(pytest_bin, cmd_args)
        .dir(repo_root)
        .unchecked()
        .run()
        .map_err(|e| {
            RunError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })?;
    let exit_code = out.status.code().unwrap_or(1);

    if args.coverage_abort_on_failure && exit_code != 0 {
        return Ok(exit_code);
    }

    if args.collect_coverage {
        let lcov = repo_root.join("coverage").join("lcov.info");
        if lcov.exists() {
            let reports = read_lcov_file(&lcov).ok().into_iter().collect::<Vec<_>>();
            let merged = merge_reports(&reports, repo_root);
            let resolved = resolve_lcov_paths_to_root(merged, repo_root);
            let filtered = filter_report(
                resolved,
                repo_root,
                &args.include_globs,
                &args.exclude_globs,
            );
            println!("{}", format_summary(&filtered));
            let print_opts = PrintOpts {
                max_files: args.coverage_max_files,
                max_hotspots: args.coverage_max_hotspots,
                page_fit: args.coverage_page_fit,
                tty: headlamp_core::format::terminal::is_output_terminal(),
                editor_cmd: args.editor_cmd.clone(),
            };
            println!("{}", format_compact(&filtered, &print_opts, repo_root));
            if let Some(detail) = args.coverage_detail {
                if detail != headlamp_core::args::CoverageDetail::Auto {
                    let hs = format_hotspots(&filtered, &print_opts, repo_root);
                    if !hs.trim().is_empty() {
                        println!("{hs}");
                    }
                }
            }
        }
    }

    Ok(exit_code)
}

fn resolve_pytest_selection(repo_root: &Path, args: &ParsedArgs) -> Result<Vec<String>, RunError> {
    let explicit = args
        .selection_paths
        .iter()
        .filter(|p| p.ends_with(".py") || p.contains('/') || p.contains('\\'))
        .map(|p| repo_root.join(p))
        .filter(|p| p.exists())
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    if !explicit.is_empty() {
        return Ok(explicit);
    }

    let changed = args
        .changed
        .map(|m| changed_files(repo_root, m))
        .transpose()?
        .unwrap_or_default();

    let tests_dir = repo_root.join("tests");
    if !tests_dir.exists() {
        return Ok(vec![]);
    }

    let all_tests = list_pytest_files(&tests_dir);
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
