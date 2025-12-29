use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use crate::diagnostics_assert::assert_parity_normalized_outputs;
use crate::env::{build_env_map, headlamp_runner_stack, program_display_name};
use crate::normalize::{normalize, normalize_tty_ui};
use crate::parity_meta::ParitySideLabel;
use crate::types::{ParityRunGroup, ParityRunSpec};

pub fn assert_parity(repo: &Path, binaries: &crate::ParityBinaries) {
    assert_parity_with_args(repo, binaries, &[], &[]);
}

pub fn assert_parity_with_args(
    repo: &Path,
    binaries: &crate::ParityBinaries,
    ts_args: &[&str],
    rs_args: &[&str],
) {
    let (_spec, code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args(
        repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        ts_args,
        rs_args,
        "jest",
    );

    let n_ts = normalize(out_ts, repo);
    let n_rs = normalize(out_rs, repo);
    assert_parity_normalized_outputs(repo, "fixture", code_ts, &n_ts, code_rs, &n_rs);
}

pub fn assert_parity_tty_ui_with_args(
    repo: &Path,
    binaries: &crate::ParityBinaries,
    case: &str,
    columns: usize,
    ts_args: &[&str],
    rs_args: &[&str],
) {
    let (_spec, code_ts, out_ts, code_rs, out_rs) = run_parity_fixture_with_args_tty(
        repo,
        &binaries.ts_cli,
        &binaries.rust_bin,
        columns,
        ts_args,
        rs_args,
        "jest",
    );

    let n_ts = normalize_tty_ui(out_ts, repo);
    let n_rs = normalize_tty_ui(out_rs, repo);
    assert_parity_normalized_outputs(repo, case, code_ts, &n_ts, code_rs, &n_rs);
}

#[derive(Debug, Clone, Copy)]
enum FixtureExecMode {
    Plain,
    Tty { columns: usize, stdout_piped: bool },
}

pub fn run_parity_fixture_with_args(
    repo: &Path,
    ts_cli: &Path,
    rust_bin: &Path,
    ts_args: &[&str],
    rs_args: &[&str],
    runner: &str,
) -> (ParityRunGroup, i32, String, i32, String) {
    run_parity_fixture_with_mode(
        repo,
        ts_cli,
        rust_bin,
        ts_args,
        rs_args,
        runner,
        FixtureExecMode::Plain,
    )
}

pub fn run_parity_fixture_with_args_tty(
    repo: &Path,
    ts_cli: &Path,
    rust_bin: &Path,
    columns: usize,
    ts_args: &[&str],
    rs_args: &[&str],
    runner: &str,
) -> (ParityRunGroup, i32, String, i32, String) {
    run_parity_fixture_with_mode(
        repo,
        ts_cli,
        rust_bin,
        ts_args,
        rs_args,
        runner,
        FixtureExecMode::Tty {
            columns,
            stdout_piped: false,
        },
    )
}

pub fn run_parity_headlamp_vs_headlamp_with_args_tty(
    repo: &Path,
    headlamp_bin: &Path,
    columns: usize,
    baseline_runner: &str,
    candidate_runner: &str,
    baseline_args: &[&str],
    candidate_args: &[&str],
) -> (ParityRunGroup, i32, String, i32, String) {
    let baseline_spec = crate::parity_run::headlamp::run_headlamp_with_args_tty_env(
        repo,
        headlamp_bin,
        columns,
        baseline_runner,
        baseline_args,
        &[],
    )
    .0;
    let candidate_spec = crate::parity_run::headlamp::run_headlamp_with_args_tty_env(
        repo,
        headlamp_bin,
        columns,
        candidate_runner,
        candidate_args,
        &[],
    )
    .0;

    let (code_baseline, out_baseline) =
        crate::exec::run_cmd_tty(build_command_from_spec(&baseline_spec), columns);
    let (code_candidate, out_candidate) =
        crate::exec::run_cmd_tty(build_command_from_spec(&candidate_spec), columns);

    (
        ParityRunGroup {
            sides: vec![baseline_spec, candidate_spec],
        },
        code_baseline,
        out_baseline,
        code_candidate,
        out_candidate,
    )
}

fn build_command_from_spec(spec: &ParityRunSpec) -> Command {
    let mut cmd = Command::new(&spec.program);
    cmd.current_dir(&spec.cwd);
    spec.args.iter().for_each(|arg| {
        cmd.arg(arg);
    });
    spec.env.iter().for_each(|(k, v)| {
        cmd.env(k, v);
    });
    cmd
}

pub fn run_parity_fixture_with_args_tty_stdout_piped(
    repo: &Path,
    ts_cli: &Path,
    rust_bin: &Path,
    columns: usize,
    ts_args: &[&str],
    rs_args: &[&str],
    runner: &str,
) -> (ParityRunGroup, i32, String, i32, String) {
    run_parity_fixture_with_mode(
        repo,
        ts_cli,
        rust_bin,
        ts_args,
        rs_args,
        runner,
        FixtureExecMode::Tty {
            columns,
            stdout_piped: true,
        },
    )
}

fn run_parity_fixture_with_mode(
    repo: &Path,
    ts_cli: &Path,
    rust_bin: &Path,
    ts_args: &[&str],
    rs_args: &[&str],
    runner: &str,
    mode: FixtureExecMode,
) -> (ParityRunGroup, i32, String, i32, String) {
    let (ts_spec, rs_spec) = fixture_specs(repo, ts_cli, rust_bin, ts_args, rs_args, runner, mode);
    let (code_ts, out_ts) = run_parity_spec(&ts_spec);
    let (code_rs, out_rs) = run_parity_spec(&rs_spec);
    (
        ParityRunGroup {
            sides: vec![ts_spec, rs_spec],
        },
        code_ts,
        out_ts,
        code_rs,
        out_rs,
    )
}

fn fixture_specs(
    repo: &Path,
    ts_cli: &Path,
    rust_bin: &Path,
    ts_args: &[&str],
    rs_args: &[&str],
    runner: &str,
    mode: FixtureExecMode,
) -> (ParityRunSpec, ParityRunSpec) {
    let (tty_columns, stdout_piped) = fixture_io_options(mode);
    let ts_side_label = ParitySideLabel {
        binary: "node".to_string(),
        runner_stack: format!("ts-cli->{runner}"),
    };
    let rs_side_label = ParitySideLabel {
        binary: program_display_name(rust_bin),
        runner_stack: headlamp_runner_stack(runner),
    };
    let ts_env = build_env_map(repo, &ts_side_label);
    let rs_env = build_env_map(repo, &rs_side_label);
    let ts_spec = ts_fixture_spec(
        FixtureSpecOptions {
            repo,
            runner,
            side_label: ts_side_label,
            env: ts_env,
            tty_columns,
            stdout_piped,
        },
        ts_cli,
        ts_args,
    );
    let rs_spec = rs_fixture_spec(
        FixtureSpecOptions {
            repo,
            runner,
            side_label: rs_side_label,
            env: rs_env,
            tty_columns,
            stdout_piped,
        },
        rust_bin,
        rs_args,
    );
    (ts_spec, rs_spec)
}

fn fixture_io_options(mode: FixtureExecMode) -> (Option<usize>, bool) {
    match mode {
        FixtureExecMode::Plain => (None, false),
        FixtureExecMode::Tty {
            columns,
            stdout_piped,
        } => (Some(columns), stdout_piped),
    }
}

struct FixtureSpecOptions<'a> {
    repo: &'a Path,
    runner: &'a str,
    side_label: ParitySideLabel,
    env: std::collections::BTreeMap<String, String>,
    tty_columns: Option<usize>,
    stdout_piped: bool,
}

fn ts_fixture_spec(
    opts: FixtureSpecOptions<'_>,
    ts_cli: &Path,
    ts_args: &[&str],
) -> ParityRunSpec {
    let FixtureSpecOptions {
        repo,
        runner,
        side_label,
        env,
        tty_columns,
        stdout_piped,
    } = opts;
    let runner_flag = format!("--runner={runner}");
    let base_args = [
        ts_cli.to_string_lossy().to_string(),
        "--sequential".to_string(),
        runner_flag,
    ];
    ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: PathBuf::from("node"),
        side_label,
        args: base_args
            .into_iter()
            .chain(ts_args.iter().map(|s| s.to_string()))
            .collect::<Vec<_>>(),
        env,
        tty_columns,
        stdout_piped,
    }
}

fn rs_fixture_spec(
    opts: FixtureSpecOptions<'_>,
    rust_bin: &Path,
    rs_args: &[&str],
) -> ParityRunSpec {
    let FixtureSpecOptions {
        repo,
        runner,
        side_label,
        env,
        tty_columns,
        stdout_piped,
    } = opts;
    let runner_flag = format!("--runner={runner}");
    let base_args = [runner_flag, "--sequential".to_string()];
    ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: rust_bin.to_path_buf(),
        side_label,
        args: base_args
            .into_iter()
            .chain(rs_args.iter().map(|s| s.to_string()))
            .collect(),
        env,
        tty_columns,
        stdout_piped,
    }
}

fn run_parity_spec(spec: &ParityRunSpec) -> (i32, String) {
    let cmd = build_command_from_spec(spec);
    match (spec.tty_columns, spec.stdout_piped) {
        (None, _) => crate::exec::run_cmd(cmd),
        (Some(columns), false) => crate::exec::run_cmd_tty(cmd, columns),
        (Some(columns), true) => crate::exec::run_cmd_tty_stdout_piped(cmd, columns),
    }
}

pub fn run_rust_fixture_with_args_tty_stdout_piped(
    repo: &Path,
    rust_bin: &Path,
    columns: usize,
    rs_args: &[&str],
) -> (i32, String) {
    let side_label = ParitySideLabel {
        binary: program_display_name(rust_bin),
        runner_stack: headlamp_runner_stack("jest"),
    };
    let env = build_env_map(repo, &side_label);
    let mut cmd_rs = Command::new(rust_bin);
    cmd_rs
        .current_dir(repo)
        .arg("--runner=jest")
        .arg("--sequential");
    env.iter().for_each(|(k, v)| {
        cmd_rs.env(k, v);
    });
    rs_args.iter().for_each(|arg| {
        cmd_rs.arg(arg);
    });
    crate::exec::run_cmd_tty_stdout_piped(cmd_rs, columns)
}
