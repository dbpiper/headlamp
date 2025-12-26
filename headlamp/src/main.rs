use std::io::IsTerminal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Runner {
    Jest,
    Vitest,
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
    if should_print_terminal_debug() {
        print_terminal_debug();
    }
    let argv0 = std::env::args().skip(1).collect::<Vec<_>>();
    if argv0.iter().any(|t| t == "--help" || t == "-h") {
        print_help();
        return;
    }
    let (runner, argv) = extract_runner(&argv0);
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let repo_root = headlamp::config::find_repo_root(&cwd);
    let cfg = headlamp::config::load_headlamp_config(&repo_root).unwrap_or_default();
    let cfg_tokens = headlamp::args::config_tokens(&cfg, &argv);
    let parsed = headlamp::args::derive_args(
        &cfg_tokens,
        &argv,
        headlamp::format::terminal::is_output_terminal(),
    );
    if parsed.ci {
        unsafe { std::env::set_var("CI", "1") };
    }
    if parsed.watch && parsed.ci {
        eprintln!("headlamp: --watch is not allowed with --ci");
        std::process::exit(2);
    }
    if parsed.verbose {
        eprintln!(
            "headlamp: runner={runner:?} repo_root={} watch={} ci={} no_cache={}",
            repo_root.to_string_lossy(),
            parsed.watch,
            parsed.ci,
            parsed.no_cache
        );
    }

    let mut run_once = || -> i32 {
        match runner {
            Runner::Jest | Runner::Vitest => match headlamp::jest::run_jest(&repo_root, &parsed) {
                Ok(code) => code,
                Err(err) => {
                    eprintln!("{err}");
                    1
                }
            },
            Runner::Pytest => match headlamp::pytest::run_pytest(&repo_root, &parsed) {
                Ok(code) => code,
                Err(err) => {
                    eprintln!("{err}");
                    1
                }
            },
            Runner::CargoTest => match headlamp::cargo::run_cargo_test(&repo_root, &parsed) {
                Ok(code) => code,
                Err(err) => {
                    eprintln!("{err}");
                    1
                }
            },
            Runner::CargoNextest => match headlamp::cargo::run_cargo_nextest(&repo_root, &parsed) {
                Ok(code) => code,
                Err(err) => {
                    eprintln!("{err}");
                    1
                }
            },
        }
    };

    let code = if parsed.watch {
        headlamp::watch::run_polling_watch_loop(
            &repo_root,
            std::time::Duration::from_millis(800),
            parsed.verbose,
            &mut run_once,
        )
    } else {
        run_once()
    };
    std::process::exit(code);
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
                runner = parse_runner(v);
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
        "vitest" => Runner::Vitest,
        "pytest" => Runner::Pytest,
        "cargo-nextest" => Runner::CargoNextest,
        "cargo-test" => Runner::CargoTest,
        _ => return None,
    })
}

fn print_help() {
    let msg = r#"headlamp

Usage:
  headlamp [--runner=<jest|vitest|pytest|cargo-nextest|cargo-test>] [--coverage] [--changed[=<mode>]] [args...]

Flags:
  --runner <runner>              Select runner (default: jest)
  --coverage                     Enable coverage collection (runner-specific)
  --coverage-ui=jest|both         Coverage output mode
  --coverage.abortOnFailure       Exit on test failures without printing coverage
  --watch                        Re-run on file changes (runner-agnostic polling watch)
  --ci                           CI mode (disable interactive UI and set CI=1)
  --verbose                      More Headlamp diagnostics
  --no-cache                     Disable Headlamp caches (and runner caches when possible)
  --onlyFailures                 Show only failing tests during live output
  --showLogs                     Show full logs under failing tests
  --sequential                   Serialize execution (maps to jest --runInBand)
  --bootstrapCommand <cmd>       Run once before tests (npm script name or shell cmd)
  --changed[=all|staged|unstaged|branch|lastCommit]
  --changed.depth=<n>

Notes:
  Unknown args are forwarded to the runner.
"#;
    println!("{msg}");
}
