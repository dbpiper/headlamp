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

pub fn run_parity_fixture_with_args(
    repo: &Path,
    ts_cli: &Path,
    rust_bin: &Path,
    ts_args: &[&str],
    rs_args: &[&str],
    runner: &str,
) -> (ParityRunGroup, i32, String, i32, String) {
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

    let mut cmd_ts = Command::new("node");
    cmd_ts
        .current_dir(repo)
        .arg(ts_cli)
        .arg("--sequential")
        .arg(format!("--runner={runner}"));
    ts_env.iter().for_each(|(k, v)| {
        cmd_ts.env(k, v);
    });
    ts_args.iter().for_each(|arg| {
        cmd_ts.arg(arg);
    });
    let ts_spec = ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: PathBuf::from("node"),
        side_label: ts_side_label,
        args: [
            ts_cli.to_string_lossy().to_string(),
            "--sequential".to_string(),
            format!("--runner={runner}"),
        ]
        .into_iter()
        .chain(ts_args.iter().map(|s| s.to_string()))
        .collect::<Vec<_>>(),
        env: ts_env,
        tty_columns: None,
        stdout_piped: false,
    };
    let (code_ts, out_ts) = crate::exec::run_cmd(cmd_ts);

    let mut cmd_rs = Command::new(rust_bin);
    cmd_rs
        .current_dir(repo)
        .arg(format!("--runner={runner}"))
        .arg("--sequential");
    rs_env.iter().for_each(|(k, v)| {
        cmd_rs.env(k, v);
    });
    rs_args.iter().for_each(|arg| {
        cmd_rs.arg(arg);
    });
    let rs_spec = ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: rust_bin.to_path_buf(),
        side_label: rs_side_label,
        args: [format!("--runner={runner}"), "--sequential".to_string()]
            .into_iter()
            .chain(rs_args.iter().map(|s| s.to_string()))
            .collect(),
        env: rs_env,
        tty_columns: None,
        stdout_piped: false,
    };
    let (code_rs, out_rs) = crate::exec::run_cmd(cmd_rs);
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

pub fn run_parity_fixture_with_args_tty(
    repo: &Path,
    ts_cli: &Path,
    rust_bin: &Path,
    columns: usize,
    ts_args: &[&str],
    rs_args: &[&str],
    runner: &str,
) -> (ParityRunGroup, i32, String, i32, String) {
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

    let mut cmd_ts = Command::new("node");
    cmd_ts
        .current_dir(repo)
        .arg(ts_cli)
        .arg("--sequential")
        .arg(format!("--runner={runner}"));
    ts_env.iter().for_each(|(k, v)| {
        cmd_ts.env(k, v);
    });
    ts_args.iter().for_each(|arg| {
        cmd_ts.arg(arg);
    });
    let ts_spec = ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: PathBuf::from("node"),
        side_label: ts_side_label,
        args: [
            ts_cli.to_string_lossy().to_string(),
            "--sequential".to_string(),
            format!("--runner={runner}"),
        ]
        .into_iter()
        .chain(ts_args.iter().map(|s| s.to_string()))
        .collect::<Vec<_>>(),
        env: ts_env,
        tty_columns: Some(columns),
        stdout_piped: false,
    };
    let (code_ts, out_ts) = crate::exec::run_cmd_tty(cmd_ts, columns);

    let mut cmd_rs = Command::new(rust_bin);
    cmd_rs
        .current_dir(repo)
        .arg(format!("--runner={runner}"))
        .arg("--sequential");
    rs_env.iter().for_each(|(k, v)| {
        cmd_rs.env(k, v);
    });
    rs_args.iter().for_each(|arg| {
        cmd_rs.arg(arg);
    });
    let rs_spec = ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: rust_bin.to_path_buf(),
        side_label: rs_side_label,
        args: [format!("--runner={runner}"), "--sequential".to_string()]
            .into_iter()
            .chain(rs_args.iter().map(|s| s.to_string()))
            .collect(),
        env: rs_env,
        tty_columns: Some(columns),
        stdout_piped: false,
    };
    let (code_rs, out_rs) = crate::exec::run_cmd_tty(cmd_rs, columns);
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

    let mut cmd_ts = Command::new("node");
    cmd_ts
        .current_dir(repo)
        .arg(ts_cli)
        .arg("--sequential")
        .arg(format!("--runner={runner}"));
    ts_env.iter().for_each(|(k, v)| {
        cmd_ts.env(k, v);
    });
    ts_args.iter().for_each(|arg| {
        cmd_ts.arg(arg);
    });
    let ts_spec = ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: PathBuf::from("node"),
        side_label: ts_side_label,
        args: [
            ts_cli.to_string_lossy().to_string(),
            "--sequential".to_string(),
            format!("--runner={runner}"),
        ]
        .into_iter()
        .chain(ts_args.iter().map(|s| s.to_string()))
        .collect::<Vec<_>>(),
        env: ts_env,
        tty_columns: Some(columns),
        stdout_piped: true,
    };
    let (code_ts, out_ts) = crate::exec::run_cmd_tty_stdout_piped(cmd_ts, columns);

    let mut cmd_rs = Command::new(rust_bin);
    cmd_rs
        .current_dir(repo)
        .arg(format!("--runner={runner}"))
        .arg("--sequential");
    rs_env.iter().for_each(|(k, v)| {
        cmd_rs.env(k, v);
    });
    rs_args.iter().for_each(|arg| {
        cmd_rs.arg(arg);
    });
    let rs_spec = ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: rust_bin.to_path_buf(),
        side_label: rs_side_label,
        args: [format!("--runner={runner}"), "--sequential".to_string()]
            .into_iter()
            .chain(rs_args.iter().map(|s| s.to_string()))
            .collect(),
        env: rs_env,
        tty_columns: Some(columns),
        stdout_piped: true,
    };
    let (code_rs, out_rs) = crate::exec::run_cmd_tty_stdout_piped(cmd_rs, columns);
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
