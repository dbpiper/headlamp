use std::path::Path;

use duct::cmd as duct_cmd;

use headlamp_core::args::ParsedArgs;
use headlamp_core::config::CoverageUi;
use headlamp_core::coverage::lcov::{merge_reports, read_lcov_file, resolve_lcov_paths_to_root};
use headlamp_core::coverage::print::{
    PrintOpts, filter_report, format_compact, format_hotspots, format_summary,
};
use headlamp_core::format::cargo_test::parse_cargo_test_output;
use headlamp_core::format::ctx::make_ctx;
use headlamp_core::format::nextest::parse_nextest_libtest_json_output;
use headlamp_core::format::vitest::render_vitest_from_test_model;

use crate::cargo_select::{changed_rust_seeds, filter_rust_tests_by_seeds, list_rust_test_files};
use crate::git::changed_files;
use crate::run::RunError;

pub fn run_cargo_test(repo_root: &Path, args: &ParsedArgs) -> Result<i32, RunError> {
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
        .stderr_to_stdout()
        .stdout_capture()
        .unchecked()
        .run()
        .map_err(|e| {
            RunError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })?;
        let exit_code = out.status.code().unwrap_or(1);
        let combined = String::from_utf8_lossy(&out.stdout).to_string();
        print_test_output(repo_root, args, exit_code, &combined);

        if args.coverage_abort_on_failure && exit_code != 0 {
            return Ok(normalize_runner_exit_code(exit_code));
        }
        if args.coverage_ui != CoverageUi::Jest {
            print_lcov(repo_root, args);
        }
        return Ok(normalize_runner_exit_code(exit_code));
    }

    let selection = derive_cargo_selection(repo_root, args, &changed);

    let exit_code = run_cargo_test_and_render(
        repo_root,
        args,
        selection.filter.as_deref(),
        &selection.extra_cargo_args,
    )?;

    if args.coverage_abort_on_failure && exit_code != 0 {
        return Ok(normalize_runner_exit_code(exit_code));
    }

    if args.collect_coverage && args.coverage_ui != CoverageUi::Jest {
        print_lcov(repo_root, args);
    }

    Ok(normalize_runner_exit_code(exit_code))
}

pub fn run_cargo_nextest(repo_root: &Path, args: &ParsedArgs) -> Result<i32, RunError> {
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
        .stderr_to_stdout()
        .stdout_capture()
        .unchecked()
        .run()
        .map_err(|e| {
            RunError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })?;
        let exit_code = out.status.code().unwrap_or(1);
        let combined = String::from_utf8_lossy(&out.stdout).to_string();
        print_test_output_nextest(repo_root, args, exit_code, &combined);

        if args.coverage_abort_on_failure && exit_code != 0 {
            return Ok(normalize_runner_exit_code(exit_code));
        }
        if args.coverage_ui != CoverageUi::Jest {
            print_lcov(repo_root, args);
        }
        return Ok(normalize_runner_exit_code(exit_code));
    }

    let selection = derive_cargo_selection(repo_root, args, &changed);
    if !has_cargo_nextest(repo_root) {
        return Err(RunError::MissingRunner {
            runner: "cargo-nextest".to_string(),
            hint: "expected `cargo nextest` to be installed and available".to_string(),
        });
    }

    let exit_code = run_nextest_and_render(
        repo_root,
        args,
        selection.filter.as_deref(),
        &selection.extra_cargo_args,
    )?;

    if args.coverage_abort_on_failure && exit_code != 0 {
        return Ok(normalize_runner_exit_code(exit_code));
    }

    if args.collect_coverage && args.coverage_ui != CoverageUi::Jest {
        print_lcov(repo_root, args);
    }

    Ok(normalize_runner_exit_code(exit_code))
}

fn normalize_runner_exit_code(exit_code: i32) -> i32 {
    if exit_code == 0 { 0 } else { 1 }
}

fn print_test_output(repo_root: &Path, args: &ParsedArgs, exit_code: i32, combined: &str) {
    let ctx = make_ctx(
        repo_root,
        None,
        exit_code != 0,
        args.show_logs,
        args.editor_cmd.clone(),
    );
    let rendered = parse_cargo_test_output(repo_root, combined)
        .map(|model| render_vitest_from_test_model(&model, &ctx, args.only_failures))
        .unwrap_or_else(|| combined.to_string());
    if !rendered.trim().is_empty() {
        println!("{rendered}");
    }
}

fn print_test_output_nextest(repo_root: &Path, args: &ParsedArgs, exit_code: i32, combined: &str) {
    let ctx = make_ctx(
        repo_root,
        None,
        exit_code != 0,
        args.show_logs,
        args.editor_cmd.clone(),
    );
    let rendered = parse_nextest_libtest_json_output(repo_root, combined)
        .map(|model| render_vitest_from_test_model(&model, &ctx, args.only_failures))
        .unwrap_or_else(|| combined.to_string());
    if !rendered.trim().is_empty() {
        println!("{rendered}");
    }
}

fn run_nextest_and_render(
    repo_root: &Path,
    args: &ParsedArgs,
    filter: Option<&str>,
    extra_cargo_args: &[String],
) -> Result<i32, RunError> {
    let cmd_args = build_nextest_run_args(filter, args, extra_cargo_args);
    let out = duct_cmd("cargo", cmd_args)
        .dir(repo_root)
        .env("NEXTEST_EXPERIMENTAL_LIBTEST_JSON", "1")
        .stderr_to_stdout()
        .stdout_capture()
        .unchecked()
        .run()
        .map_err(|e| {
            RunError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })?;
    let exit_code = out.status.code().unwrap_or(1);
    let combined = String::from_utf8_lossy(&out.stdout).to_string();

    print_test_output_nextest(repo_root, args, exit_code, &combined);
    Ok(exit_code)
}

fn run_cargo_test_and_render(
    repo_root: &Path,
    args: &ParsedArgs,
    filter: Option<&str>,
    extra_cargo_args: &[String],
) -> Result<i32, RunError> {
    let cmd_args = build_cargo_test_args(filter, args, extra_cargo_args);
    let out = duct_cmd("cargo", cmd_args)
        .dir(repo_root)
        .stderr_to_stdout()
        .stdout_capture()
        .unchecked()
        .run()
        .map_err(|e| {
            RunError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })?;
    let exit_code = out.status.code().unwrap_or(1);
    let combined = String::from_utf8_lossy(&out.stdout).to_string();
    print_test_output(repo_root, args, exit_code, &combined);
    Ok(exit_code)
}

fn build_nextest_run_args(
    filter: Option<&str>,
    args: &ParsedArgs,
    extra_cargo_args: &[String],
) -> Vec<String> {
    let (cargo_args, test_binary_args) = split_cargo_passthrough_args(&args.runner_args);
    let mut cmd_args: Vec<String> = vec!["nextest".to_string(), "run".to_string()];

    cmd_args.extend([
        "--color".to_string(),
        "never".to_string(),
        "--status-level".to_string(),
        "none".to_string(),
        "--final-status-level".to_string(),
        "none".to_string(),
        "--no-fail-fast".to_string(),
        "--show-progress".to_string(),
        "none".to_string(),
        "--success-output".to_string(),
        "never".to_string(),
        "--failure-output".to_string(),
        "never".to_string(),
        "--cargo-quiet".to_string(),
        "--no-input-handler".to_string(),
        "--no-output-indent".to_string(),
        "--message-format".to_string(),
        "libtest-json-plus".to_string(),
    ]);

    let translated = translate_libtest_args_to_nextest(&test_binary_args);
    if args.sequential
        && translated.test_threads.is_none()
        && !cargo_args.iter().any(|t| t == "--test-threads")
    {
        cmd_args.extend(["--test-threads".to_string(), "1".to_string()]);
    } else if let Some(n) = translated.test_threads.as_ref() {
        cmd_args.extend(["--test-threads".to_string(), n.to_string()]);
    }

    cmd_args.extend(extra_cargo_args.iter().cloned());
    cmd_args.extend(cargo_args);
    if let Some(f) = filter.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        cmd_args.push(f.to_string());
    } else if let Some(user_filter) = translated.filter.as_deref() {
        cmd_args.push(user_filter.to_string());
    }

    if !translated.passthrough.is_empty() {
        cmd_args.push("--".to_string());
        cmd_args.extend(translated.passthrough);
    }
    cmd_args
}

struct NextestArgTranslation {
    test_threads: Option<u32>,
    passthrough: Vec<String>,
    filter: Option<String>,
}

fn translate_libtest_args_to_nextest(test_binary_args: &[String]) -> NextestArgTranslation {
    let mut test_threads: Option<u32> = None;
    let mut passthrough: Vec<String> = vec![];
    let mut filter: Option<String> = None;
    let mut index: usize = 0;
    while index < test_binary_args.len() {
        let token = test_binary_args[index].as_str();
        match token {
            "--test-threads" => {
                test_threads = test_binary_args
                    .get(index + 1)
                    .and_then(|s| s.parse::<u32>().ok());
                index += 2;
            }
            "--nocapture" | "--no-capture" => {
                passthrough.push("--no-capture".to_string());
                index += 1;
            }
            "--ignored" | "--include-ignored" | "--exact" => {
                passthrough.push(token.to_string());
                index += 1;
            }
            "--skip" => {
                passthrough.push("--skip".to_string());
                if let Some(value) = test_binary_args.get(index + 1) {
                    passthrough.push(value.clone());
                    index += 2;
                } else {
                    index += 1;
                }
            }
            _ => {
                if !token.starts_with('-') && filter.is_none() {
                    filter = Some(token.to_string());
                }
                index += 1;
            }
        }
    }
    NextestArgTranslation {
        test_threads,
        passthrough,
        filter,
    }
}

fn build_cargo_test_args(
    filter: Option<&str>,
    args: &ParsedArgs,
    extra_cargo_args: &[String],
) -> Vec<String> {
    let (cargo_args, test_binary_args) = split_cargo_passthrough_args(&args.runner_args);
    let mut cmd_args: Vec<String> = vec!["test".to_string()];
    if let Some(f) = filter.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        cmd_args.push(f.to_string());
    }
    cmd_args.extend(extra_cargo_args.iter().cloned());
    cmd_args.extend(cargo_args);

    let mut normalized_test_args: Vec<String> = vec!["--color".to_string(), "never".to_string()];
    if should_force_pretty_test_output(&test_binary_args) {
        normalized_test_args.extend(["--format".to_string(), "pretty".to_string()]);
    }
    if args.sequential && !test_binary_args.iter().any(|t| t == "--test-threads") {
        normalized_test_args.extend(["--test-threads".to_string(), "1".to_string()]);
    }
    normalized_test_args.extend(test_binary_args);

    cmd_args.push("--".to_string());
    cmd_args.extend(normalized_test_args);
    cmd_args
}

fn should_force_pretty_test_output(test_binary_args: &[String]) -> bool {
    let overrides_format = test_binary_args.iter().any(|token| {
        token == "--format" || token.starts_with("--format=") || token == "-q" || token == "--quiet"
    });
    !overrides_format
}

fn split_cargo_passthrough_args(passthrough: &[String]) -> (Vec<String>, Vec<String>) {
    let sanitized = passthrough
        .iter()
        .filter(|t| !is_jest_default_runner_arg(t))
        .cloned()
        .collect::<Vec<_>>();
    sanitized
        .iter()
        .position(|t| t == "--")
        .map(|index| (sanitized[..index].to_vec(), sanitized[index + 1..].to_vec()))
        .unwrap_or((sanitized, vec![]))
}

fn is_jest_default_runner_arg(token: &str) -> bool {
    token == "--runInBand"
        || token == "--no-silent"
        || token == "--coverage"
        || token.starts_with("--coverageProvider=")
        || token.starts_with("--coverageReporters=")
}

fn has_cargo_nextest(repo_root: &Path) -> bool {
    duct_cmd("cargo", ["nextest", "--version"])
        .dir(repo_root)
        .stdout_capture()
        .stderr_capture()
        .unchecked()
        .run()
        .ok()
        .map_or(false, |o| o.status.success())
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

#[derive(Debug, Clone)]
struct CargoSelection {
    filter: Option<String>,
    extra_cargo_args: Vec<String>,
}

fn derive_cargo_selection(
    repo_root: &Path,
    args: &ParsedArgs,
    changed: &[std::path::PathBuf],
) -> CargoSelection {
    if !args.selection_paths.is_empty() {
        return derive_selection_from_selection_paths(repo_root, &args.selection_paths);
    }

    if changed.is_empty() {
        return CargoSelection {
            filter: None,
            extra_cargo_args: vec![],
        };
    }

    let tests = list_rust_test_files(repo_root);
    if tests.is_empty() {
        return CargoSelection {
            filter: None,
            extra_cargo_args: vec![],
        };
    }

    let seeds = changed_rust_seeds(repo_root, changed);
    let kept = filter_rust_tests_by_seeds(&tests, &seeds);
    let filter = kept
        .iter()
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()))
        .next()
        .map(|s| s.to_string());

    CargoSelection {
        filter,
        extra_cargo_args: vec![],
    }
}

fn derive_selection_from_selection_paths(
    repo_root: &Path,
    selection_paths: &[String],
) -> CargoSelection {
    let abs = selection_paths
        .iter()
        .map(|p| repo_root.join(p))
        .filter(|p| p.exists())
        .collect::<Vec<_>>();
    if abs.is_empty() {
        return CargoSelection {
            filter: None,
            extra_cargo_args: vec![],
        };
    }

    let direct_test_stem = abs
        .iter()
        .find(|p| is_rust_test_file(p))
        .and_then(|p| p.file_stem().and_then(|s| s.to_str()))
        .map(|s| s.to_string());
    if let Some(stem) = direct_test_stem {
        return CargoSelection {
            filter: None,
            extra_cargo_args: vec!["--test".to_string(), stem],
        };
    }

    CargoSelection {
        filter: derive_filter_from_seeds(repo_root, &abs),
        extra_cargo_args: vec![],
    }
}

fn derive_filter_from_seeds(
    repo_root: &Path,
    seeds_input: &[std::path::PathBuf],
) -> Option<String> {
    let tests = list_rust_test_files(repo_root);
    if tests.is_empty() {
        return None;
    }
    let seeds = changed_rust_seeds(repo_root, seeds_input);
    let kept = filter_rust_tests_by_seeds(&tests, &seeds);
    kept.iter()
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()))
        .next()
        .map(|s| s.to_string())
}

fn is_rust_test_file(path: &std::path::Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("rs")
        && path
            .components()
            .any(|c| c.as_os_str().to_string_lossy() == "tests")
}
