use std::path::{Path, PathBuf};
use std::process::Stdio;

use headlamp_core::args::ParsedArgs;

use crate::run::RunError;

pub(crate) struct RustCoveragePaths {
    pub(crate) lcov_path: PathBuf,
    pub(crate) llvm_cov_json_path: PathBuf,
    pub(crate) profraw_dir: PathBuf,
    pub(crate) profdata_path: PathBuf,
}

pub(crate) fn rust_coverage_paths(
    keep_artifacts: bool,
    repo_root: &Path,
    session: &crate::session::RunSession,
) -> RustCoveragePaths {
    let (lcov_path, llvm_cov_json_path, profraw_dir, profdata_path) = if keep_artifacts {
        (
            repo_root.join("coverage").join("lcov.info"),
            repo_root.join("coverage").join("coverage.json"),
            repo_root.join("coverage").join("rust").join("profraw"),
            repo_root
                .join("coverage")
                .join("rust")
                .join("coverage.profdata"),
        )
    } else {
        (
            session.subdir("coverage").join("rust").join("lcov.info"),
            session
                .subdir("coverage")
                .join("rust")
                .join("coverage.json"),
            session.subdir("coverage").join("rust").join("profraw"),
            session
                .subdir("coverage")
                .join("rust")
                .join("coverage.profdata"),
        )
    };
    RustCoveragePaths {
        lcov_path,
        llvm_cov_json_path,
        profraw_dir,
        profdata_path,
    }
}

pub(crate) fn coverage_rustflags() -> Vec<std::ffi::OsString> {
    vec![
        std::ffi::OsString::from("-C"),
        std::ffi::OsString::from("instrument-coverage"),
    ]
}

pub(crate) fn coverage_rustflags_with_branch_coverage(
    enable_branch_coverage: bool,
) -> Vec<std::ffi::OsString> {
    if !enable_branch_coverage {
        return coverage_rustflags();
    }
    let mut out = coverage_rustflags();
    out.push(std::ffi::OsString::from("-Z"));
    out.push(std::ffi::OsString::from("coverage-options=branch"));
    out
}

pub(crate) fn append_rustflags(
    existing_rustflags: &str,
    additional: &[std::ffi::OsString],
) -> String {
    if additional.is_empty() {
        return existing_rustflags.to_string();
    }
    let appended = additional
        .iter()
        .map(|s| s.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    if existing_rustflags.trim().is_empty() {
        appended
    } else {
        format!("{existing_rustflags} {appended}")
    }
}

pub(crate) fn choose_llvm_tools_toolchain(repo_root: &Path) -> (String, bool) {
    // Branch coverage requires nightly rustc instrumentation, but llvm-profdata/llvm-cov do not
    // need to come from nightly. Prefer nightly llvm-tools if they're installed; otherwise use
    // stable llvm-tools but still enable branch coverage when nightly rustc exists.
    let enable_branch_coverage = crate::cargo::paths::nightly_rustc_exists(repo_root);
    let toolchain = if crate::cargo::paths::can_use_nightly(repo_root) {
        "nightly".to_string()
    } else {
        "stable".to_string()
    };
    (toolchain, enable_branch_coverage)
}

pub(crate) fn ensure_llvm_tools_available(
    repo_root: &Path,
    toolchain: &str,
) -> Result<(), RunError> {
    for tool in ["llvm-profdata", "llvm-cov"] {
        let Some(tool_path) = llvm_tool_path_from_rustc(repo_root, toolchain, tool) else {
            return Err(RunError::MissingRunner {
                runner: tool.to_string(),
                hint: format!(
                    "expected `{tool}` via rustup; try `rustup component add llvm-tools-preview --toolchain {toolchain}`"
                ),
            });
        };
        let status = std::process::Command::new(tool_path)
            .current_dir(repo_root)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if !status.is_ok_and(|s| s.success()) {
            return Err(RunError::MissingRunner {
                runner: tool.to_string(),
                hint: format!(
                    "expected `{tool}` via rustup; try `rustup component add llvm-tools-preview --toolchain {toolchain}`"
                ),
            });
        }
    }
    Ok(())
}

fn llvm_tool_path_from_rustc(repo_root: &Path, toolchain: &str, tool: &str) -> Option<PathBuf> {
    // `rustup run <toolchain> <tool>` does NOT reliably work for llvm-tools, because on many
    // toolchains the binaries live under:
    //   lib/rustlib/<target>/bin/{llvm-profdata,llvm-cov}
    //
    // Instead, ask rustc for its target libdir and derive the sibling bin dir.
    let output = std::process::Command::new("rustup")
        .current_dir(repo_root)
        .args(["run", toolchain, "rustc", "--print", "target-libdir"])
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let libdir_text = String::from_utf8_lossy(&output.stdout);
    let libdir = PathBuf::from(libdir_text.trim());
    let rustlib_target_dir = libdir.parent()?.to_path_buf();
    let bin_dir = rustlib_target_dir.join("bin");
    let tool_path = bin_dir.join(tool);
    tool_path.exists().then_some(tool_path)
}

pub(crate) fn purge_profile_artifacts(dir: &Path) {
    fn purge_dir(dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(ty) = entry.file_type() else {
                continue;
            };
            if ty.is_dir() {
                purge_dir(&path);
                continue;
            }
            if !ty.is_file() {
                continue;
            }
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            if ext == "profraw" || ext == "profdata" {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    purge_dir(dir);
}

pub(crate) fn merge_profraw_dir_to_profdata(
    repo_root: &Path,
    toolchain: &str,
    profraw_dir: &Path,
    profdata_path: &Path,
) -> Result<(), RunError> {
    let profraw_files = list_profraw_files_recursive(profraw_dir)?;
    if profraw_files.is_empty() {
        return Err(RunError::CommandFailed {
            message: format!(
                "coverage requested but no profraw files were produced under {}",
                profraw_dir.display()
            ),
        });
    }
    if let Some(parent) = profdata_path.parent() {
        std::fs::create_dir_all(parent).map_err(RunError::Io)?;
    }
    let tool = llvm_tool_path_from_rustc(repo_root, toolchain, "llvm-profdata").ok_or_else(|| {
        RunError::MissingRunner {
            runner: "llvm-profdata".to_string(),
            hint: format!(
                "expected `llvm-profdata` via rustup; try `rustup component add llvm-tools-preview --toolchain {toolchain}`"
            ),
        }
    })?;
    let mut cmd = std::process::Command::new(tool);
    cmd.current_dir(repo_root);
    cmd.args(["merge", "-sparse"]);
    cmd.arg("-o").arg(profdata_path);
    profraw_files.iter().for_each(|p| {
        cmd.arg(p);
    });
    let status = cmd.status().map_err(RunError::SpawnFailed)?;
    status
        .success()
        .then_some(())
        .ok_or(RunError::CommandFailed {
            message: "llvm-profdata merge failed".to_string(),
        })
}

pub(crate) fn export_llvm_cov_reports(
    repo_root: &Path,
    toolchain: &str,
    profdata_path: &Path,
    objects: &[PathBuf],
    lcov_path: &Path,
    llvm_cov_json_path: &Path,
) -> Result<(), RunError> {
    if let Some(parent) = lcov_path.parent() {
        std::fs::create_dir_all(parent).map_err(RunError::Io)?;
    }
    export_llvm_cov_lcov(repo_root, toolchain, profdata_path, objects, lcov_path)?;
    export_llvm_cov_json(
        repo_root,
        toolchain,
        profdata_path,
        objects,
        llvm_cov_json_path,
    )?;
    Ok(())
}

fn export_llvm_cov_lcov(
    repo_root: &Path,
    toolchain: &str,
    profdata_path: &Path,
    objects: &[PathBuf],
    out_path: &Path,
) -> Result<(), RunError> {
    export_llvm_cov_with_format(
        repo_root,
        toolchain,
        profdata_path,
        objects,
        Some("lcov"),
        out_path,
    )
}

fn export_llvm_cov_json(
    repo_root: &Path,
    toolchain: &str,
    profdata_path: &Path,
    objects: &[PathBuf],
    out_path: &Path,
) -> Result<(), RunError> {
    // llvm-cov export defaults to JSON on many toolchains. We intentionally do NOT pass
    // a `--format=json` flag because some llvm-cov builds reject it.
    export_llvm_cov_with_format(repo_root, toolchain, profdata_path, objects, None, out_path)
}

fn export_llvm_cov_with_format(
    repo_root: &Path,
    toolchain: &str,
    profdata_path: &Path,
    objects: &[PathBuf],
    format: Option<&str>,
    out_path: &Path,
) -> Result<(), RunError> {
    let file = std::fs::File::create(out_path).map_err(RunError::Io)?;
    let tool = llvm_tool_path_from_rustc(repo_root, toolchain, "llvm-cov").ok_or_else(|| {
        RunError::MissingRunner {
            runner: "llvm-cov".to_string(),
            hint: format!(
                "expected `llvm-cov` via rustup; try `rustup component add llvm-tools-preview --toolchain {toolchain}`"
            ),
        }
    })?;
    let mut cmd = std::process::Command::new(tool);
    cmd.current_dir(repo_root);
    cmd.args(build_llvm_cov_export_args(format, profdata_path, objects));
    let output = cmd
        .stdout(file)
        .stderr(Stdio::piped())
        .output()
        .map_err(RunError::Io)?;
    output
        .status
        .success()
        .then_some(())
        .ok_or(RunError::CommandFailed {
            message: format!(
                "llvm-cov export failed (exit={}):\n{}",
                output.status.code().unwrap_or(1),
                String::from_utf8_lossy(&output.stderr)
            ),
        })
}

pub(crate) fn build_llvm_cov_export_args(
    format: Option<&str>,
    profdata_path: &Path,
    objects: &[PathBuf],
) -> Vec<std::ffi::OsString> {
    let mut args: Vec<std::ffi::OsString> = Vec::with_capacity(8 + (objects.len() * 2));
    args.push(std::ffi::OsString::from("export"));
    if let Some(fmt) = format {
        args.push(std::ffi::OsString::from(format!("-format={fmt}")));
    }
    args.push(std::ffi::OsString::from(format!(
        "-instr-profile={}",
        profdata_path.to_string_lossy()
    )));
    objects.iter().for_each(|object| {
        args.push(std::ffi::OsString::from("-object"));
        args.push(object.as_os_str().to_os_string());
    });
    args
}

fn list_profraw_files_recursive(root: &Path) -> Result<Vec<PathBuf>, RunError> {
    let mut out: Vec<PathBuf> = vec![];
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(RunError::Io)?;
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(ty) = entry.file_type() else {
                continue;
            };
            if ty.is_dir() {
                stack.push(path);
                continue;
            }
            if !ty.is_file() {
                continue;
            }
            if path.extension().and_then(|s| s.to_str()) == Some("profraw") {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

pub(crate) fn llvm_profile_file_pattern(profraw_dir: &Path, prefix: &str) -> PathBuf {
    profraw_dir.join(format!("{prefix}-%m-%p.profraw"))
}

pub(crate) fn should_abort_coverage_after_run(
    args: &ParsedArgs,
    model: &crate::test_model::TestRunModel,
) -> bool {
    args.coverage_abort_on_failure
        && (model.aggregated.num_failed_tests > 0 || model.aggregated.num_failed_test_suites > 0)
}

pub(crate) fn should_collect_rust_coverage(args: &ParsedArgs) -> bool {
    args.collect_coverage && args.coverage_ui != headlamp_core::config::CoverageUi::Jest
}
