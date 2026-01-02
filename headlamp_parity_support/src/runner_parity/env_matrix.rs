use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::exec::{TtyBackend, run_cmd_tty_with_backend_timeout, run_cmd_with_timeout};
use crate::parity_meta::ParitySideLabel;
use crate::types::ParityRunSpec;

use super::RunnerId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParityExecEnv {
    Tty { columns: usize },
    NonTty,
}

#[derive(Debug, Clone)]
pub struct EnvMatrixRunResult {
    pub env: ParityExecEnv,
    pub runner: RunnerId,
    pub spec: ParityRunSpec,
    pub exit: i32,
    pub output: String,
    pub tty_backend: Option<TtyBackend>,
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

fn enforce_timeout_ok(case: &str, result: &EnvMatrixRunResult, timeout: Duration) {
    if result.exit != 124 {
        return;
    }
    panic!(
        "parity env hang detected case={case} env={:?} runner={:?} timeout={}s\nspec: program={} cwd={}\nargs: {:?}\noutput:\n{}",
        result.env,
        result.runner,
        timeout.as_secs(),
        result.spec.program.display(),
        result.spec.cwd.display(),
        result.spec.args,
        result.output,
    );
}

fn mk_headlamp_run_spec(
    repo: &Path,
    headlamp_bin: &Path,
    runner_id: RunnerId,
    extra_args: &[&str],
    case_id: Option<&str>,
    tty_columns: Option<usize>,
) -> ParityRunSpec {
    let runner = runner_id.as_runner_flag_value();
    let base_args = [format!("--runner={runner}"), "--sequential".to_string()];
    let side_label = ParitySideLabel {
        binary: crate::env::program_display_name(headlamp_bin),
        runner_stack: crate::env::headlamp_runner_stack(runner),
    };
    let env = crate::env::build_env_map(repo, &side_label, case_id);
    ParityRunSpec {
        cwd: repo.to_path_buf(),
        program: headlamp_bin.to_path_buf(),
        side_label,
        args: base_args
            .into_iter()
            .chain(extra_args.iter().map(|s| s.to_string()))
            .collect(),
        env,
        tty_columns,
        stdout_piped: false,
        exec_backend: None,
    }
}

pub fn run_env_matrix_no_hang(
    repo: &Path,
    headlamp_bin: &Path,
    case: &str,
    runners: &[(RunnerId, &[&str])],
    environments: &[ParityExecEnv],
    timeout: Duration,
    case_id: Option<&str>,
) -> Vec<EnvMatrixRunResult> {
    let mut out: Vec<EnvMatrixRunResult> = Vec::with_capacity(runners.len() * environments.len());
    for env in environments {
        for (runner_id, args) in runners {
            let (tty_columns, tty_backend) = match env {
                ParityExecEnv::Tty { columns } => (Some(*columns), Some(TtyBackend::PortablePty)),
                ParityExecEnv::NonTty => (None, None),
            };
            let mut spec =
                mk_headlamp_run_spec(repo, headlamp_bin, *runner_id, args, case_id, tty_columns);

            let cmd = build_command_from_spec(&spec);
            let (exit, output, backend) = match env {
                ParityExecEnv::Tty { columns } => {
                    let (code, text, backend) =
                        run_cmd_tty_with_backend_timeout(cmd, *columns, timeout);
                    spec.exec_backend = Some(backend.to_string());
                    (code, text, Some(backend))
                }
                ParityExecEnv::NonTty => {
                    // Non-TTY: preserve the existing env map; run with a strict timeout to detect hangs.
                    let (code, text) = run_cmd_with_timeout(cmd, timeout);
                    (code, text, None)
                }
            };
            let result = EnvMatrixRunResult {
                env: *env,
                runner: *runner_id,
                spec,
                exit,
                output,
                tty_backend: backend.or(tty_backend),
            };
            enforce_timeout_ok(case, &result, timeout);
            out.push(result);
        }
    }
    out
}
