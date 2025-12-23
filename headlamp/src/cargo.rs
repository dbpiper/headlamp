use std::path::Path;

use duct::cmd as duct_cmd;

use headlamp_core::args::ParsedArgs;
use headlamp_core::coverage::lcov::{merge_reports, read_lcov_file, resolve_lcov_paths_to_root};
use headlamp_core::coverage::print::{
    PrintOpts, filter_report, format_compact, format_hotspots, format_summary,
};

use crate::cargo_select::{changed_rust_seeds, filter_rust_tests_by_seeds, list_rust_test_files};
use crate::git::changed_files;
use crate::run::RunError;

pub fn run_cargo(repo_root: &Path, args: &ParsedArgs) -> Result<i32, RunError> {
    let changed = args
        .changed
        .map(|m| changed_files(repo_root, m))
        .transpose()?
        .unwrap_or_default();

    if args.collect_coverage && has_cargo_llvm_cov(repo_root) {
        let out = duct_cmd(
            "cargo",
            ["llvm-cov", "--lcov", "--output-path", "coverage/lcov.info"],
        )
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
            print_lcov(repo_root, args);
        }
        return Ok(exit_code);
    }

    let filter = derive_cargo_filter(repo_root, args, &changed);

    let mut cmd_args: Vec<String> = vec!["test".to_string()];
    if let Some(f) = filter.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        cmd_args.push(f.to_string());
    }
    cmd_args.extend(args.runner_args.iter().cloned());
    let out = duct_cmd("cargo", cmd_args)
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
        print_lcov(repo_root, args);
    }

    Ok(exit_code)
}

fn print_lcov(repo_root: &Path, args: &ParsedArgs) {
    let lcov = repo_root.join("coverage").join("lcov.info");
    if !lcov.exists() {
        return;
    }
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

fn has_cargo_llvm_cov(repo_root: &Path) -> bool {
    duct_cmd("cargo", ["llvm-cov", "--version"])
        .dir(repo_root)
        .stdout_capture()
        .stderr_capture()
        .unchecked()
        .run()
        .ok()
        .map_or(false, |o| o.status.success())
}

fn derive_cargo_filter(
    repo_root: &Path,
    args: &ParsedArgs,
    changed: &[std::path::PathBuf],
) -> Option<String> {
    if !args.selection_paths.is_empty() {
        let seed = args
            .selection_paths
            .iter()
            .filter_map(|p| {
                Path::new(p)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            })
            .next();
        if seed.is_some() {
            return seed;
        }
    }

    if changed.is_empty() {
        return None;
    }

    let tests = list_rust_test_files(repo_root);
    if tests.is_empty() {
        return None;
    }

    let seeds = changed_rust_seeds(repo_root, changed);
    let kept = filter_rust_tests_by_seeds(&tests, &seeds);
    kept.iter()
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()))
        .next()
        .map(|s| s.to_string())
}
