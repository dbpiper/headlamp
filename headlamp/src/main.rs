use std::io::IsTerminal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Runner {
    Jest,
    Vitest,
    Pytest,
    Cargo,
}

fn base_flag(t: &str) -> &str {
    t.split_once('=').map(|(k, _)| k).unwrap_or(t)
}

fn main() {
    let argv0 = std::env::args().skip(1).collect::<Vec<_>>();
    if argv0.iter().any(|t| t == "--help" || t == "-h") {
        print_help();
        return;
    }
    let (runner, argv) = extract_runner(&argv0);
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let repo_root = headlamp_core::config::find_repo_root(&cwd);
    let cfg = headlamp_core::config::load_headlamp_config(&repo_root).unwrap_or_default();
    let cfg_tokens = headlamp_core::args::config_tokens(&cfg, &argv);
    let parsed =
        headlamp_core::args::derive_args(&cfg_tokens, &argv, std::io::stdout().is_terminal());
    let code = match runner {
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
        Runner::Cargo => match headlamp::cargo::run_cargo(&repo_root, &parsed) {
            Ok(code) => code,
            Err(err) => {
                eprintln!("{err}");
                1
            }
        },
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
        "cargo" => Runner::Cargo,
        _ => return None,
    })
}

fn print_help() {
    let msg = r#"headlamp

Usage:
  headlamp [--runner=<jest|vitest|pytest|cargo>] [--coverage] [--changed[=<mode>]] [args...]

Flags:
  --runner <runner>              Select runner (default: jest)
  --coverage                     Enable coverage collection (runner-specific)
  --coverage-ui=jest|both         Coverage output mode
  --coverage.abortOnFailure       Exit on test failures without printing coverage
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
