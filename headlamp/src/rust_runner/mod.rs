use std::path::Path;

use crate::args::ParsedArgs;
use crate::live_progress::{LiveProgress, live_progress_mode};
use crate::run::RunError;
use crate::streaming::run_streaming_capture_tail_merged;

pub(crate) mod cargo_build;
mod coverage;
mod index;
#[cfg(test)]
mod libtest_parser;
#[cfg(test)]
mod libtest_parser_test;
mod stream_adapter;

pub fn run_headlamp_rust(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
) -> Result<i32, RunError> {
    let started_at = std::time::Instant::now();
    if args.collect_coverage {
        return coverage::run_headlamp_rust_with_coverage(repo_root, args, session);
    }
    run_optional_bootstrap(repo_root, args)?;

    let changed_files = changed_files_for_args(repo_root, args)?;
    let selection =
        crate::cargo::selection::derive_cargo_selection(repo_root, args, &changed_files);

    let binaries = index::load_or_build_binary_index(repo_root, args, session, &selection)?;
    if binaries.is_empty() {
        return Ok(0);
    }

    let libtest_filter = derive_libtest_filter(repo_root, args);
    let live_progress = start_live_progress(args, binaries.len());
    let (suite_models, exit_code) = run_test_binaries(
        repo_root,
        args,
        live_progress,
        binaries,
        libtest_filter.as_deref(),
    )?;

    let run_time_ms = started_at.elapsed().as_millis() as u64;
    let _model = render_and_print_run_model(repo_root, args, suite_models, run_time_ms, exit_code);
    Ok(exit_code)
}

fn start_live_progress(args: &ParsedArgs, total_units: usize) -> LiveProgress {
    let mode = live_progress_mode(
        crate::format::terminal::is_output_terminal(),
        args.ci,
        args.quiet,
    );
    LiveProgress::start(total_units, mode)
}

fn run_test_binaries(
    repo_root: &Path,
    args: &ParsedArgs,
    live_progress: LiveProgress,
    binaries: Vec<index::TestBinary>,
    libtest_filter: Option<&str>,
) -> Result<(Vec<crate::test_model::TestSuiteResult>, i32), RunError> {
    let use_libtest_json = crate::cargo::paths::nightly_rustc_exists(repo_root)
        && should_use_libtest_json_output(&args.runner_args);
    let test_binary_args = build_test_binary_args(args, libtest_filter, use_libtest_json);
    let mut suite_models: Vec<crate::test_model::TestSuiteResult> = vec![];
    let mut exit_code: i32 = 0;

    for binary in binaries {
        let (model, current_exit_code) = run_single_test_binary(
            repo_root,
            args,
            &live_progress,
            &binary,
            &test_binary_args,
            None,
            use_libtest_json,
        )?;
        if current_exit_code != 0 {
            exit_code = 1;
        }
        if let Some(model) = model {
            suite_models.extend(model.test_results);
        }
    }

    live_progress.finish();
    Ok((suite_models, exit_code))
}

fn run_single_test_binary(
    repo_root: &Path,
    args: &ParsedArgs,
    live_progress: &LiveProgress,
    binary: &index::TestBinary,
    test_binary_args: &[String],
    llvm_profile_file: Option<&std::ffi::OsStr>,
    use_libtest_json: bool,
) -> Result<(Option<crate::test_model::TestRunModel>, i32), RunError> {
    let mut cmd = std::process::Command::new(&binary.executable);
    cmd.current_dir(repo_root);
    cmd.env("RUST_BACKTRACE", "1");
    cmd.env("RUST_LIB_BACKTRACE", "1");
    if let Some(profile_file) = llvm_profile_file {
        cmd.env("LLVM_PROFILE_FILE", profile_file);
    }
    cmd.args(test_binary_args);

    if use_libtest_json {
        let mut adapter = stream_adapter::LibtestJsonAdapter::new(
            repo_root,
            args.only_failures,
            binary.suite_source_path.as_str(),
        );
        let (exit_code, _tail) =
            run_streaming_capture_tail_merged(cmd, live_progress, &mut adapter, 1024 * 1024)?;
        live_progress.increment_done(1);
        Ok((adapter.parser.finalize(), exit_code))
    } else {
        let mut adapter = stream_adapter::DirectLibtestAdapter::new(
            repo_root,
            args.only_failures,
            binary.suite_source_path.as_str(),
        );
        let (exit_code, _tail) =
            run_streaming_capture_tail_merged(cmd, live_progress, &mut adapter, 1024 * 1024)?;
        live_progress.increment_done(1);
        Ok((adapter.parser.finalize(), exit_code))
    }
}

fn render_and_print_run_model(
    repo_root: &Path,
    args: &ParsedArgs,
    suites: Vec<crate::test_model::TestSuiteResult>,
    run_time_ms: u64,
    exit_code: i32,
) -> crate::test_model::TestRunModel {
    let model = stream_adapter::build_run_model(suites, run_time_ms);
    let ctx = crate::format::ctx::make_ctx(
        repo_root,
        None,
        exit_code != 0,
        args.show_logs,
        args.editor_cmd.clone(),
    );
    let rendered =
        crate::format::vitest::render_vitest_from_test_model(&model, &ctx, args.only_failures);
    if !rendered.trim().is_empty() {
        println!("{rendered}");
    }
    model
}

fn run_optional_bootstrap(repo_root: &Path, args: &ParsedArgs) -> Result<(), RunError> {
    let Some(command) = args.bootstrap_command.as_deref() else {
        return Ok(());
    };
    crate::run::run_bootstrap(repo_root, command)
}

fn changed_files_for_args(
    repo_root: &Path,
    args: &ParsedArgs,
) -> Result<Vec<std::path::PathBuf>, RunError> {
    args.changed
        .map(|mode| crate::git::changed_files(repo_root, mode))
        .transpose()
        .map(|v| v.unwrap_or_default())
}

fn derive_libtest_filter(repo_root: &Path, args: &ParsedArgs) -> Option<String> {
    if args.selection_paths.is_empty() {
        return None;
    }
    if args.selection_paths.len() != 1 {
        return None;
    }
    let candidate = args.selection_paths[0].trim();
    if candidate.is_empty() {
        return None;
    }
    let candidate_path = repo_root.join(candidate);
    if candidate_path.exists() {
        return None;
    }
    Some(candidate.to_string())
}

fn build_test_binary_args(
    args: &ParsedArgs,
    filter: Option<&str>,
    use_libtest_json: bool,
) -> Vec<String> {
    let mut out: Vec<String> = vec!["--color".to_string(), "never".to_string()];
    if use_libtest_json {
        out.extend([
            "-Z".to_string(),
            "unstable-options".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ]);
        out.push("--report-time".to_string());
    } else if should_force_pretty_test_output(&args.runner_args) {
        out.extend(["--format".to_string(), "pretty".to_string()]);
    }
    if let Some(filter) = filter.map(str::trim).filter(|s| !s.is_empty()) {
        out.push(filter.to_string());
    }
    // Mirror cargo-test/nextest behavior: show passing test output without switching libtest into
    // "no capture" mode (which changes line formats and makes parsing harder).
    if args.show_logs && should_force_show_output(&args.runner_args) {
        out.push("--show-output".to_string());
    }
    if args.sequential && !args.runner_args.iter().any(|t| t == "--test-threads") {
        out.extend(["--test-threads".to_string(), "1".to_string()]);
    }
    out.extend(args.runner_args.iter().cloned());
    out
}

fn should_use_libtest_json_output(test_binary_args: &[String]) -> bool {
    let overrides_format = test_binary_args
        .iter()
        .any(|token| token == "--format" || token.starts_with("--format="));
    let overrides_unstable = test_binary_args
        .iter()
        .any(|token| token == "-Z" || token.starts_with("-Z") || token == "--unstable-options");
    !overrides_format && !overrides_unstable
}

fn should_force_pretty_test_output(test_binary_args: &[String]) -> bool {
    let overrides_format = test_binary_args.iter().any(|token| {
        token == "--format" || token.starts_with("--format=") || token == "-q" || token == "--quiet"
    });
    !overrides_format
}

fn should_force_show_output(test_binary_args: &[String]) -> bool {
    let overrides = test_binary_args
        .iter()
        .any(|token| token == "--show-output" || token == "--nocapture" || token == "--no-capture");
    !overrides
}
