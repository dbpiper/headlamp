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
    run_headlamp_with_args_tty_env(repo, headlamp_bin, columns, runner, args, &[], None)
}

pub fn run_headlamp_with_args_tty_env(
    repo: &Path,
    headlamp_bin: &Path,
    columns: usize,
    runner: &str,
    args: &[&str],
    extra_env: &[(&str, String)],
    case_id: Option<&str>,
) -> (ParityRunSpec, i32, String) {
    let mut spec = mk_headlamp_tty_run_spec(
        repo,
        headlamp_bin,
        columns,
        runner,
        args,
        extra_env,
        case_id,
    );
    let (code, out, backend) =
        crate::exec::run_cmd_tty_with_backend(build_command_from_spec(&spec), columns);
    spec.exec_backend = Some(backend.to_string());
    (spec, code, out)
}

fn mk_headlamp_tty_run_spec(
    repo: &Path,
    headlamp_bin: &Path,
    columns: usize,
    runner: &str,
    args: &[&str],
    extra_env: &[(&str, String)],
    case_id: Option<&str>,
) -> ParityRunSpec {
    let base_args = [
        format!("--runner={runner}"),
        "--sequential".to_string(),
        "--no-cache".to_string(),
    ];
    let side_label = ParitySideLabel {
        binary: program_display_name(headlamp_bin),
        runner_stack: headlamp_runner_stack(runner),
    };
    let mut env = build_env_map(repo, &side_label, case_id);
    // Always-on in CI: write headlamp sidecar diagnostics under the parity dump root (or temp).
    // This does not affect stdout/stderr that parity compares.
    if std::env::var("CI").ok().is_some()
        || std::env::var("HEADLAMP_PARITY_DUMP_ROOT").ok().is_some()
    {
        let dump_root = std::env::var("HEADLAMP_PARITY_DUMP_ROOT")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let repo_key = headlamp::fast_related::stable_repo_key_hash_12(repo);
        let run_id = format!(
            "run-{}-{}",
            std::process::id(),
            crate::hashing::next_capture_id()
        );
        let dir = dump_root
            .join("headlamp-trace")
            .join(repo_key)
            .join(side_label.file_safe_label())
            .join(run_id);
        env.insert(
            "HEADLAMP_DIAGNOSTICS_DIR".to_string(),
            dir.to_string_lossy().to_string(),
        );
    }
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
        exec_backend: None,
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
