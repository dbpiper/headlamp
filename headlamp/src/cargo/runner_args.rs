use headlamp_core::args::ParsedArgs;

pub(super) fn build_llvm_cov_test_run_args(
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    extra_cargo_args: &[String],
    write_lcov_report_to_file: bool,
) -> Vec<String> {
    let (cargo_args, test_binary_args) = split_cargo_passthrough_args(&args.runner_args);
    let mut cmd_args: Vec<String> = vec![
        "test".to_string(),
        "--color".to_string(),
        "never".to_string(),
    ];
    if write_lcov_report_to_file {
        // When we run cargo-llvm-cov in "reuse instrumented build" mode (`--no-clean`), it
        // requires a report output mode (and forbids `--no-report`). Keep stdout clean by writing
        // lcov directly to a file during the run.
        cmd_args.extend([
            "--lcov".to_string(),
            "--output-path".to_string(),
            lcov_output_path_token(args.keep_artifacts, session),
        ]);
    } else {
        cmd_args.push("--no-report".to_string());
    }
    cmd_args.extend(extra_cargo_args.iter().cloned());
    if !cargo_args.iter().any(|t| t == "--no-fail-fast")
        && !extra_cargo_args.iter().any(|t| t == "--no-fail-fast")
    {
        cmd_args.push("--no-fail-fast".to_string());
    }
    cmd_args.extend(cargo_args);

    let mut normalized_test_args: Vec<String> = vec!["--color".to_string(), "never".to_string()];
    if should_force_pretty_test_output(&test_binary_args) {
        normalized_test_args.extend(["--format".to_string(), "pretty".to_string()]);
    }
    if args.show_logs && should_force_nocapture(&test_binary_args) {
        normalized_test_args.push("--nocapture".to_string());
    }
    if args.sequential && !test_binary_args.iter().any(|t| t == "--test-threads") {
        normalized_test_args.extend(["--test-threads".to_string(), "1".to_string()]);
    }
    normalized_test_args.extend(test_binary_args);

    cmd_args.push("--".to_string());
    cmd_args.extend(normalized_test_args);
    cmd_args
}

pub(super) fn build_llvm_cov_nextest_run_args(
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    extra_cargo_args: &[String],
    write_lcov_report_to_file: bool,
) -> Vec<String> {
    let (nextest_options, test_binary_args) = split_cargo_passthrough_args(&args.runner_args);
    let has_user_test_threads = nextest_options.iter().any(|t| t == "--test-threads");
    let has_user_color = nextest_options
        .iter()
        .any(|t| t == "--color" || t.starts_with("--color="));
    let translated = translate_libtest_args_to_nextest(&test_binary_args);

    let (success_output, failure_output) = nextest_output_modes(args.show_logs);
    let is_interactive = is_interactive_nextest_progress(args);

    let mut cmd_args: Vec<String> = vec!["nextest".to_string()];
    extend_llvm_cov_report_mode_args(
        &mut cmd_args,
        write_lcov_report_to_file,
        args.keep_artifacts,
        session,
    );
    cmd_args.extend(extra_cargo_args.iter().cloned());
    cmd_args.extend(nextest_options);
    extend_nextest_common_args(
        &mut cmd_args,
        has_user_color,
        is_interactive,
        success_output,
        failure_output,
    );
    extend_nextest_test_threads(&mut cmd_args, args, &translated, has_user_test_threads);
    extend_nextest_filter_and_passthrough(&mut cmd_args, translated);
    cmd_args
}

fn nextest_output_modes(show_logs: bool) -> (&'static str, &'static str) {
    if show_logs {
        ("immediate", "immediate")
    } else {
        ("never", "never")
    }
}

fn is_interactive_nextest_progress(args: &ParsedArgs) -> bool {
    crate::live_progress::live_progress_mode(
        headlamp_core::format::terminal::is_output_terminal(),
        args.ci,
    ) == crate::live_progress::LiveProgressMode::Interactive
}

fn extend_llvm_cov_report_mode_args(
    cmd_args: &mut Vec<String>,
    write_lcov_report_to_file: bool,
    keep_artifacts: bool,
    session: &crate::session::RunSession,
) {
    if write_lcov_report_to_file {
        cmd_args.extend([
            "--lcov".to_string(),
            "--output-path".to_string(),
            lcov_output_path_token(keep_artifacts, session),
        ]);
    } else {
        cmd_args.push("--no-report".to_string());
    }
}

fn lcov_output_path_token(keep_artifacts: bool, session: &crate::session::RunSession) -> String {
    if keep_artifacts {
        "coverage/lcov.info".to_string()
    } else {
        session
            .subdir("coverage")
            .join("rust")
            .join("lcov.info")
            .to_string_lossy()
            .to_string()
    }
}

fn extend_nextest_common_args(
    cmd_args: &mut Vec<String>,
    has_user_color: bool,
    is_interactive: bool,
    success_output: &str,
    failure_output: &str,
) {
    if !has_user_color {
        cmd_args.extend(["--color".to_string(), "never".to_string()]);
    }
    cmd_args.extend([
        "--status-level".to_string(),
        "none".to_string(),
        "--final-status-level".to_string(),
        "none".to_string(),
        "--no-fail-fast".to_string(),
        "--show-progress".to_string(),
        "none".to_string(),
        "--success-output".to_string(),
        success_output.to_string(),
        "--failure-output".to_string(),
        failure_output.to_string(),
        "--no-input-handler".to_string(),
        "--no-output-indent".to_string(),
        "--message-format".to_string(),
        "libtest-json-plus".to_string(),
    ]);
    if !is_interactive {
        cmd_args.push("--cargo-quiet".to_string());
    }
}

fn extend_nextest_test_threads(
    cmd_args: &mut Vec<String>,
    args: &ParsedArgs,
    translated: &NextestArgTranslation,
    has_user_test_threads: bool,
) {
    if args.sequential && translated.test_threads.is_none() && !has_user_test_threads {
        cmd_args.extend(["--test-threads".to_string(), "1".to_string()]);
    } else if let Some(n) = translated.test_threads.as_ref() {
        cmd_args.extend(["--test-threads".to_string(), n.to_string()]);
    }
}

fn extend_nextest_filter_and_passthrough(
    cmd_args: &mut Vec<String>,
    translated: NextestArgTranslation,
) {
    if let Some(user_filter) = translated.filter.as_deref() {
        cmd_args.push(user_filter.to_string());
    }
    if !translated.passthrough.is_empty() {
        cmd_args.push("--".to_string());
        cmd_args.extend(translated.passthrough);
    }
}

pub(super) fn build_nextest_run_args(
    filter: Option<&str>,
    args: &ParsedArgs,
    extra_cargo_args: &[String],
) -> Vec<String> {
    let (cargo_args, test_binary_args) = split_cargo_passthrough_args(&args.runner_args);
    let mut cmd_args: Vec<String> = vec!["nextest".to_string(), "run".to_string()];
    let (success_output, failure_output) = if args.show_logs {
        ("immediate", "immediate")
    } else {
        ("never", "never")
    };
    let is_interactive = crate::live_progress::live_progress_mode(
        headlamp_core::format::terminal::is_output_terminal(),
        args.ci,
    ) == crate::live_progress::LiveProgressMode::Interactive;

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
        success_output.to_string(),
        "--failure-output".to_string(),
        failure_output.to_string(),
        "--no-input-handler".to_string(),
        "--no-output-indent".to_string(),
        "--message-format".to_string(),
        "libtest-json-plus".to_string(),
    ]);
    if !is_interactive {
        cmd_args.push("--cargo-quiet".to_string());
    }

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

pub(super) fn build_cargo_test_args(
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
    if !cargo_args.iter().any(|t| t == "--no-fail-fast")
        && !extra_cargo_args.iter().any(|t| t == "--no-fail-fast")
    {
        cmd_args.push("--no-fail-fast".to_string());
    }
    cmd_args.extend(cargo_args);

    let mut normalized_test_args: Vec<String> = vec!["--color".to_string(), "never".to_string()];
    if should_force_pretty_test_output(&test_binary_args) {
        normalized_test_args.extend(["--format".to_string(), "pretty".to_string()]);
    }
    if args.show_logs && should_force_show_output(&test_binary_args) {
        normalized_test_args.push("--show-output".to_string());
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

fn should_force_nocapture(test_binary_args: &[String]) -> bool {
    let overrides_capture = test_binary_args.iter().any(|token| {
        token == "--nocapture"
            || token == "--no-capture"
            || token == "--capture"
            || token == "--show-output"
    });
    !overrides_capture
}

fn should_force_show_output(test_binary_args: &[String]) -> bool {
    should_force_nocapture(test_binary_args)
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

#[derive(Debug)]
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
