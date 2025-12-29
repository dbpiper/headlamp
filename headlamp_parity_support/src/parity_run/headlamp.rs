use std::path::Path;
use std::process::Command;

use crate::env::{build_env_map, headlamp_runner_stack, program_display_name};
use crate::parity_meta::ParitySideLabel;
use crate::types::ParityRunSpec;

pub fn run_headlamp_with_args_tty(
    repo: &Path,
    headlamp_bin: &Path,
    columns: usize,
    runner: &str,
    args: &[&str],
) -> (ParityRunSpec, i32, String) {
    run_headlamp_with_args_tty_env(repo, headlamp_bin, columns, runner, args, &[])
}

pub fn run_headlamp_with_args_tty_env(
    repo: &Path,
    headlamp_bin: &Path,
    columns: usize,
    runner: &str,
    args: &[&str],
    extra_env: &[(&str, String)],
) -> (ParityRunSpec, i32, String) {
    let spec = mk_headlamp_tty_run_spec(repo, headlamp_bin, columns, runner, args, extra_env);
    let (code, out) = crate::exec::run_cmd_tty(build_command_from_spec(&spec), columns);
    (spec, code, out)
}

fn mk_headlamp_tty_run_spec(
    repo: &Path,
    headlamp_bin: &Path,
    columns: usize,
    runner: &str,
    args: &[&str],
    extra_env: &[(&str, String)],
) -> ParityRunSpec {
    let base_args = [format!("--runner={runner}"), "--sequential".to_string()];
    let side_label = ParitySideLabel {
        binary: program_display_name(headlamp_bin),
        runner_stack: headlamp_runner_stack(runner),
    };
    let mut env = build_env_map(repo, &side_label);
    extra_env.iter().for_each(|(k, v)| {
        env.insert((*k).to_string(), v.clone());
    });
    ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: headlamp_bin.to_path_buf(),
        side_label: side_label.clone(),
        args: base_args
            .into_iter()
            .chain(args.iter().map(|s| s.to_string()))
            .collect(),
        env,
        tty_columns: Some(columns),
        stdout_piped: false,
    }
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
