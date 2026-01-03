use std::io::BufRead;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::args::ParsedArgs;
use crate::run::RunError;

#[derive(Debug, Clone)]
pub(crate) struct BuiltTestBinary {
    pub(crate) executable: PathBuf,
    pub(crate) suite_source_path: String,
}

#[derive(Debug, Deserialize)]
struct CargoTarget {
    #[serde(default)]
    src_path: String,
    #[serde(default)]
    kind: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CargoProfile {
    #[serde(default)]
    test: bool,
}

#[derive(Debug, Deserialize)]
struct CargoMessage {
    #[serde(default)]
    reason: String,
    #[serde(default)]
    executable: Option<String>,
    #[serde(default)]
    target: Option<CargoTarget>,
    #[serde(default)]
    profile: Option<CargoProfile>,
}

pub(crate) fn build_test_binaries_via_cargo_no_run(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    extra_cargo_args: &[String],
) -> Result<Vec<BuiltTestBinary>, RunError> {
    build_test_binaries_via_cargo_no_run_with_options(
        repo_root,
        args,
        session,
        extra_cargo_args,
        CargoNoRunBuildOverrides {
            use_nightly: crate::cargo::paths::nightly_rustc_exists(repo_root),
            ..CargoNoRunBuildOverrides::default()
        },
    )
}

#[derive(Debug, Default, Clone)]
pub(crate) struct CargoNoRunBuildOverrides<'a> {
    pub(crate) use_nightly: bool,
    pub(crate) override_cargo_target_dir: Option<&'a Path>,
    pub(crate) additional_rustflags: &'a [std::ffi::OsString],
    pub(crate) llvm_profile_file: Option<&'a std::ffi::OsStr>,
}

pub(crate) fn build_test_binaries_via_cargo_no_run_with_overrides(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    extra_cargo_args: &[String],
    override_cargo_target_dir: &Path,
    additional_rustflags: &[std::ffi::OsString],
    llvm_profile_file: Option<&std::ffi::OsStr>,
) -> Result<Vec<BuiltTestBinary>, RunError> {
    build_test_binaries_via_cargo_no_run_with_options(
        repo_root,
        args,
        session,
        extra_cargo_args,
        CargoNoRunBuildOverrides {
            use_nightly: crate::cargo::paths::nightly_rustc_exists(repo_root),
            override_cargo_target_dir: Some(override_cargo_target_dir),
            additional_rustflags,
            llvm_profile_file,
        },
    )
}

fn build_test_binaries_via_cargo_no_run_with_options(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    extra_cargo_args: &[String],
    overrides: CargoNoRunBuildOverrides<'_>,
) -> Result<Vec<BuiltTestBinary>, RunError> {
    let mut cmd = build_cargo_no_run_command(repo_root, args, session, extra_cargo_args, overrides);
    let (mut child, stdout) = spawn_child_with_piped_stdout(&mut cmd)?;
    let (mut out, debug) = parse_cargo_no_run_json_stdout(repo_root, args, stdout);
    ensure_child_success(&mut child, "cargo test --no-run failed")?;
    debug.maybe_print_summary(args);
    out.sort_by(|a, b| a.executable.cmp(&b.executable));
    out.dedup_by(|a, b| a.executable == b.executable);
    Ok(out)
}

fn build_cargo_no_run_command(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    extra_cargo_args: &[String],
    overrides: CargoNoRunBuildOverrides<'_>,
) -> std::process::Command {
    let mut cmd = std::process::Command::new("cargo");
    if overrides.use_nightly {
        cmd.arg("+nightly");
    }
    cmd.args([
        "test",
        "--no-run",
        "--message-format=json",
        "--color",
        "never",
    ]);
    cmd.args(extra_cargo_args);
    cmd.current_dir(repo_root);
    cmd.env("CARGO_INCREMENTAL", "0");
    apply_rustflags(&mut cmd, overrides.additional_rustflags);
    if let Some(profile_file) = overrides.llvm_profile_file {
        cmd.env("LLVM_PROFILE_FILE", profile_file);
    }
    if std::env::var_os("CARGO_TARGET_DIR").is_none() {
        if let Some(override_dir) = overrides.override_cargo_target_dir {
            cmd.env("CARGO_TARGET_DIR", override_dir);
        } else {
            crate::cargo::paths::apply_headlamp_cargo_target_dir(
                &mut cmd,
                args.keep_artifacts,
                repo_root,
                session,
            );
        }
    }
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::inherit());
    cmd
}

fn spawn_child_with_piped_stdout(
    cmd: &mut std::process::Command,
) -> Result<(std::process::Child, std::process::ChildStdout), RunError> {
    let mut child = cmd.spawn().map_err(RunError::SpawnFailed)?;
    let stdout = child.stdout.take().ok_or_else(|| {
        RunError::Io(std::io::Error::other(
            "cargo test --no-run did not provide stdout",
        ))
    })?;
    Ok((child, stdout))
}

#[derive(Debug, Default, Clone)]
struct CargoNoRunJsonDebugCounts {
    seen_artifacts_with_executable: usize,
    total_json_lines: usize,
    compiler_artifacts: usize,
    with_executable: usize,
    kept: usize,
}

impl CargoNoRunJsonDebugCounts {
    fn maybe_print_sample_line(
        &mut self,
        args: &ParsedArgs,
        executable: &str,
        message: &CargoMessage,
        is_test_profile: bool,
        is_test_kind: bool,
        is_custom_build: bool,
    ) {
        if !args.verbose || self.seen_artifacts_with_executable >= 5 {
            return;
        }
        self.seen_artifacts_with_executable = self.seen_artifacts_with_executable.saturating_add(1);
        eprintln!(
            "headlamp(headlamp-rust): cargo artifact executable={} is_test_profile={} is_test_kind={} is_custom_build={} kind={:?} src_path={:?}",
            executable,
            is_test_profile,
            is_test_kind,
            is_custom_build,
            message
                .target
                .as_ref()
                .map(|t| t.kind.clone())
                .unwrap_or_default(),
            message
                .target
                .as_ref()
                .map(|t| t.src_path.clone())
                .unwrap_or_default(),
        );
    }

    fn maybe_print_summary(&self, args: &ParsedArgs) {
        if !args.verbose {
            return;
        }
        eprintln!(
            "headlamp(headlamp-rust): cargo test --no-run json_lines={} compiler_artifacts={} artifacts_with_executable={} kept={}",
            self.total_json_lines, self.compiler_artifacts, self.with_executable, self.kept
        );
    }
}

fn parse_cargo_no_run_json_stdout(
    repo_root: &Path,
    args: &ParsedArgs,
    stdout: std::process::ChildStdout,
) -> (Vec<BuiltTestBinary>, CargoNoRunJsonDebugCounts) {
    let mut debug = CargoNoRunJsonDebugCounts::default();
    let mut out: Vec<BuiltTestBinary> = vec![];
    let reader = BufReader::new(stdout);
    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if trimmed.is_empty() || !trimmed.starts_with('{') {
            continue;
        }
        debug.total_json_lines = debug.total_json_lines.saturating_add(1);
        let Ok(message) = serde_json::from_str::<CargoMessage>(trimmed) else {
            continue;
        };
        if message.reason != "compiler-artifact" {
            continue;
        }
        debug.compiler_artifacts = debug.compiler_artifacts.saturating_add(1);
        let Some(executable) = message
            .executable
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        else {
            continue;
        };
        debug.with_executable = debug.with_executable.saturating_add(1);
        let is_test_profile = message.profile.as_ref().is_some_and(|p| p.test);
        let is_test_kind = message
            .target
            .as_ref()
            .is_some_and(|t| t.kind.iter().any(|k| k == "test"));
        let is_custom_build = message.target.as_ref().is_some_and(|t| {
            t.kind
                .iter()
                .any(|k| k == "custom-build" || k == "build-script-build")
        });
        debug.maybe_print_sample_line(
            args,
            executable,
            &message,
            is_test_profile,
            is_test_kind,
            is_custom_build,
        );
        if !(is_test_profile || is_test_kind) || is_custom_build {
            continue;
        }
        let suite_source_path = normalize_suite_source_path(repo_root, message.target.as_ref());
        out.push(BuiltTestBinary {
            executable: PathBuf::from(executable),
            suite_source_path,
        });
        debug.kept = debug.kept.saturating_add(1);
    }
    (out, debug)
}

fn ensure_child_success(
    child: &mut std::process::Child,
    failure_message: &'static str,
) -> Result<(), RunError> {
    let status = child.wait().map_err(RunError::WaitFailed)?;
    status
        .success()
        .then_some(())
        .ok_or(RunError::CommandFailed {
            message: failure_message.to_string(),
        })
}

fn apply_rustflags(cmd: &mut std::process::Command, additional: &[std::ffi::OsString]) {
    if additional.is_empty() {
        return;
    }
    let existing = std::env::var("RUSTFLAGS").unwrap_or_default();
    let mut appended = additional
        .iter()
        .map(|s| s.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    if !existing.trim().is_empty() {
        appended = format!("{existing} {appended}");
    }
    cmd.env("RUSTFLAGS", appended);
}

fn normalize_suite_source_path(repo_root: &Path, target: Option<&CargoTarget>) -> String {
    let src_path = target
        .map(|t| t.src_path.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("tests");
    let candidate = Path::new(src_path);
    if let Ok(stripped) = candidate.strip_prefix(repo_root) {
        return stripped.to_string_lossy().to_string();
    }
    if candidate.is_absolute() {
        return candidate.to_string_lossy().to_string();
    }
    src_path.to_string()
}
