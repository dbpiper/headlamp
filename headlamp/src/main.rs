use std::io::IsTerminal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Runner {
    Jest,
    Pytest,
    CargoTest,
    CargoNextest,
}

fn base_flag(t: &str) -> &str {
    t.split_once('=').map(|(k, _)| k).unwrap_or(t)
}

fn should_print_terminal_debug() -> bool {
    std::env::var("HEADLAMP_DEBUG_TERMINAL")
        .ok()
        .is_some_and(|value| !value.trim().is_empty() && value.trim() != "0")
}

fn print_terminal_debug() {
    let stdout_is_tty = std::io::stdout().is_terminal();
    let stderr_is_tty = std::io::stderr().is_terminal();
    let detected_size = headlamp::format::terminal::detect_terminal_size_cols_rows();

    eprintln!(
        "HEADLAMP_DEBUG_TERMINAL: stdout_tty={stdout_is_tty} stderr_tty={stderr_is_tty} output_tty={} term={:?} term_program={:?} no_color={:?} force_color={:?} clicolor={:?} columns={:?} ci={:?} detected_size={:?}",
        headlamp::format::terminal::is_output_terminal(),
        std::env::var("TERM").ok(),
        std::env::var("TERM_PROGRAM").ok(),
        std::env::var("NO_COLOR").ok(),
        std::env::var("FORCE_COLOR").ok(),
        std::env::var("CLICOLOR").ok(),
        std::env::var("COLUMNS").ok(),
        std::env::var("CI").ok(),
        detected_size,
    );
}

fn main() {
    should_print_terminal_debug()
        .then(print_terminal_debug)
        .unwrap_or(());
    let argv0 = std::env::args().skip(1).collect::<Vec<_>>();
    if should_print_help(&argv0) {
        print_help();
        return;
    }
    let (runner, argv) = extract_runner(&argv0);
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let config_root = headlamp::config::find_repo_root(&cwd);
    let parsed = build_parsed_args(&config_root, &argv);
    let run_root = resolve_run_root(runner, &cwd, &parsed);
    apply_ci_env(&parsed);
    validate_watch_ci(&parsed);
    maybe_print_verbose_startup(runner, &run_root, &parsed);
    let user_cache_dir_was_set = std::env::var_os("HEADLAMP_CACHE_DIR").is_some();
    let mut run_once_closure = || run_once(runner, &run_root, &parsed, user_cache_dir_was_set);
    let code = if parsed.watch {
        {
            headlamp::watch::run_polling_watch_loop(
                &run_root,
                std::time::Duration::from_millis(800),
                parsed.verbose,
                &mut run_once_closure,
            )
        }
    } else {
        run_once_closure()
    };
    std::process::exit(code);
}

fn resolve_run_root(
    runner: Runner,
    cwd: &std::path::Path,
    parsed: &headlamp::args::ParsedArgs,
) -> std::path::PathBuf {
    let workspace_override = parsed
        .workspace_root
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .map(|p| if p.is_absolute() { p } else { cwd.join(p) });

    if let Some(p) = workspace_override {
        return p;
    }

    match runner {
        Runner::Pytest => headlamp::project::markers::find_pyproject_toml_root(cwd)
            .unwrap_or_else(|| cwd.to_path_buf()),
        _ => headlamp::config::find_repo_root(cwd),
    }
}

fn should_print_help(argv0: &[String]) -> bool {
    argv0.iter().any(|t| t == "--help" || t == "-h")
}

fn build_parsed_args(repo_root: &std::path::Path, argv: &[String]) -> headlamp::args::ParsedArgs {
    let cfg = headlamp::config::load_headlamp_config(repo_root).unwrap_or_default();
    let cfg_tokens = headlamp::args::config_tokens(&cfg, argv);
    headlamp::args::derive_args(
        &cfg_tokens,
        argv,
        headlamp::format::terminal::is_output_terminal(),
    )
}

fn apply_ci_env(parsed: &headlamp::args::ParsedArgs) {
    if parsed.ci {
        unsafe { std::env::set_var("CI", "1") };
    }
}

fn validate_watch_ci(parsed: &headlamp::args::ParsedArgs) {
    if parsed.watch && parsed.ci {
        eprintln!("headlamp: --watch is not allowed with --ci");
        std::process::exit(2);
    }
}

fn maybe_print_verbose_startup(
    runner: Runner,
    repo_root: &std::path::Path,
    parsed: &headlamp::args::ParsedArgs,
) {
    if !parsed.verbose {
        return;
    }
    eprintln!(
        "headlamp: runner={runner:?} repo_root={} watch={} ci={} no_cache={}",
        repo_root.to_string_lossy(),
        parsed.watch,
        parsed.ci,
        parsed.no_cache
    );
}

fn run_once(
    runner: Runner,
    repo_root: &std::path::Path,
    parsed: &headlamp::args::ParsedArgs,
    user_cache_dir_was_set: bool,
) -> i32 {
    let session = match headlamp::session::RunSession::new(parsed.keep_artifacts) {
        Ok(session) => session,
        Err(err) => return render_run_error(repo_root, parsed, runner, err),
    };
    if !parsed.keep_artifacts && !user_cache_dir_was_set {
        let cache_dir = session.subdir("cache");
        let _ = std::fs::create_dir_all(&cache_dir);
        unsafe { std::env::set_var("HEADLAMP_CACHE_DIR", cache_dir) };
    }
    match runner {
        Runner::Jest => headlamp::jest::run_jest(repo_root, parsed, &session)
            .unwrap_or_else(|err| render_run_error(repo_root, parsed, runner, err)),
        Runner::Pytest => headlamp::pytest::run_pytest(repo_root, parsed, &session)
            .unwrap_or_else(|err| render_run_error(repo_root, parsed, runner, err)),
        Runner::CargoTest => headlamp::cargo::run_cargo_test(repo_root, parsed, &session)
            .unwrap_or_else(|err| render_run_error(repo_root, parsed, runner, err)),
        Runner::CargoNextest => headlamp::cargo::run_cargo_nextest(repo_root, parsed, &session)
            .unwrap_or_else(|err| render_run_error(repo_root, parsed, runner, err)),
    }
}

fn runner_label(runner: Runner) -> &'static str {
    match runner {
        Runner::Jest => "jest",
        Runner::Pytest => "pytest",
        Runner::CargoTest => "cargo-test",
        Runner::CargoNextest => "cargo-nextest",
    }
}

fn render_run_error(
    repo_root: &std::path::Path,
    parsed: &headlamp::args::ParsedArgs,
    runner: Runner,
    err: headlamp::run::RunError,
) -> i32 {
    let ctx = headlamp::format::ctx::make_ctx(
        repo_root,
        None,
        true,
        parsed.show_logs,
        parsed.editor_cmd.clone(),
    );
    let suite_path = format!("headlamp/{}", runner_label(runner));
    let model = headlamp::format::infra_failure::build_infra_failure_test_run_model(
        suite_path.as_str(),
        "Test suite failed to run",
        &err.to_string(),
    );
    let rendered = headlamp::format::vitest::render_vitest_from_test_model(&model, &ctx, true);
    if !rendered.trim().is_empty() {
        println!("{rendered}");
    }
    1
}

fn extract_runner(argv: &[String]) -> (Runner, Vec<String>) {
    let mut out: Vec<String> = vec![];
    let mut runner: Option<Runner> = None;

    let mut i = 0usize;
    while i < argv.len() {
        let tok = argv[i].as_str();
        if base_flag(tok) == "--runner" {
            let v = tok
                .split_once('=')
                .map(|(_, v)| v)
                .or_else(|| argv.get(i + 1).map(|s| s.as_str()));
            if let Some(v) = v {
                runner = parse_runner(v).or_else(|| {
                    eprintln!("headlamp: unknown runner: {v}");
                    eprintln!();
                    print_help();
                    std::process::exit(2);
                });
                i += if tok.contains('=') { 1 } else { 2 };
                continue;
            }
        }
        out.push(argv[i].clone());
        i += 1;
    }

    (runner.unwrap_or(Runner::Jest), out)
}

fn parse_runner(raw: &str) -> Option<Runner> {
    Some(match raw.trim().to_ascii_lowercase().as_str() {
        "jest" => Runner::Jest,
        "pytest" => Runner::Pytest,
        "cargo-nextest" => Runner::CargoNextest,
        "cargo-test" => Runner::CargoTest,
        _ => return None,
    })
}

fn print_help() {
    println!("{}", headlamp::help::help_text());
}
