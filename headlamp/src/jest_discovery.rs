use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use path_slash::PathExt;
use tempfile::NamedTempFile;

use crate::fast_related::FAST_RELATED_TIMEOUT;
use crate::fast_related::{DEFAULT_TEST_GLOBS, cached_related, find_related_tests_fast};
use crate::jest_config::append_config_arg_if_missing;
use crate::process::run_command_capture_with_timeout;
use crate::run::RunError;
use sha1::{Digest, Sha1};

const WATCH_FLAGS: [&str; 3] = ["--watch", "-w", "--watchAll"];
pub const JEST_LIST_TESTS_TIMEOUT: Duration = Duration::from_secs(20);

pub fn args_for_discovery(jest_args: &[String]) -> Vec<String> {
    let coverage_prefixes = [
        "--coverage",
        "--coverageReporters",
        "--coverageDirectory",
        "--coverage.reporter",
        "--coverage.reportsDirectory",
    ];

    let mut out: Vec<String> = vec![];
    let mut i = 0usize;
    while i < jest_args.len() {
        let tok = jest_args[i].as_str();
        if tok == "run" {
            i += 1;
            continue;
        }
        if WATCH_FLAGS.contains(&tok) {
            i += 1;
            continue;
        }

        let is_coverage = coverage_prefixes
            .iter()
            .any(|p| tok == *p || tok.starts_with(&format!("{p}=")));
        if is_coverage {
            let consumes_next = coverage_prefixes.contains(&tok)
                && i + 1 < jest_args.len()
                && !String::from(jest_args[i + 1].as_str()).starts_with('-');
            i += if consumes_next { 2 } else { 1 };
            continue;
        }

        out.push(jest_args[i].clone());
        i += 1;
    }

    if out.iter().any(|t| t == "--no-watchman") {
        out
    } else {
        out.into_iter()
            .chain([String::from("--no-watchman")])
            .collect()
    }
}

pub fn discover_jest_list_tests(
    repo_root: &Path,
    jest_bin: &Path,
    jest_args: &[String],
) -> Result<Vec<String>, RunError> {
    discover_jest_list_tests_with_timeout(repo_root, jest_bin, jest_args, JEST_LIST_TESTS_TIMEOUT)
}

pub fn discover_jest_list_tests_with_timeout(
    repo_root: &Path,
    jest_bin: &Path,
    jest_args: &[String],
    timeout: Duration,
) -> Result<Vec<String>, RunError> {
    let mut args = jest_args.to_vec();
    args.push("--listTests".to_string());
    let args = append_config_arg_if_missing(&args, repo_root);
    let display_command = format!(
        "{} {}",
        jest_bin.to_string_lossy(),
        args.iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    );
    let mut command = Command::new(jest_bin);
    command
        .args(&args)
        .current_dir(repo_root)
        .env("CI", "1")
        .env("NODE_ENV", "test");
    let out = run_command_capture_with_timeout(command, display_command, timeout)?;
    if !out.status.success() {
        let exit_code = out.status.code().unwrap_or(1);
        let stderr_text = String::from_utf8_lossy(&out.stderr);
        let message = if stderr_text.trim().is_empty() {
            format!("exit {exit_code}")
        } else {
            stderr_text.to_string()
        };
        return Err(RunError::CommandFailed { message });
    }
    // In some environments (notably when running under a PTY), Jest can emit `--listTests`
    // output on stderr instead of stdout. Prefer stdout, but fall back to stderr if stdout
    // is empty to keep discovery stable across CI/local.
    let stdout_text = String::from_utf8_lossy(&out.stdout);
    let stderr_text = String::from_utf8_lossy(&out.stderr);
    let text = if stdout_text.trim().is_empty() {
        stderr_text
    } else {
        stdout_text
    };
    Ok(text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(|l| std::path::Path::new(l).to_slash_lossy().to_string())
        .collect())
}

pub fn discover_jest_list_tests_cached_with_timeout(
    cwd: &Path,
    jest_bin: &Path,
    jest_args: &[String],
    no_cache: bool,
    timeout: Duration,
) -> Result<Vec<String>, RunError> {
    if no_cache {
        return discover_jest_list_tests_with_timeout(cwd, jest_bin, jest_args, timeout);
    }
    let cache_root = crate::fast_related::default_cache_root();
    let repo_key = crate::fast_related::stable_repo_key_hash_12(cwd);
    let dir = cache_root.join(repo_key);
    let file = dir.join("jest-list.json");

    let head = crate::fast_related::git_short_head(cwd).unwrap_or_else(|| "nohead".to_string());
    let repo_identity = crate::fast_related::stable_repo_key_hash_12(cwd);
    let status_hash = git_test_status_hash(cwd);
    let key = format!(
        "{head}{status_hash}::{repo_identity}::{}",
        jest_args
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    );

    let mut bag: std::collections::BTreeMap<String, Vec<String>> =
        read_json_map(&file).unwrap_or_default();
    if let Some(hit) = bag.get(&key)
        && !hit.is_empty()
    {
        return Ok(hit.clone());
    };

    let listed = match discover_jest_list_tests_with_timeout(cwd, jest_bin, jest_args, timeout) {
        Ok(v) => Ok(v),
        Err(RunError::TimedOut { .. }) => {
            // Don't silently treat a timeout as "no tests" and definitely don't cache it.
            // Under load, a single transient timeout would otherwise produce non-deterministic
            // results (and can poison the cache). Retry once to keep behavior stable.
            discover_jest_list_tests_with_timeout(cwd, jest_bin, jest_args, timeout)
        }
        Err(e) => Err(e),
    }?;

    let mut uniq = listed;
    uniq.sort();
    uniq.dedup();
    if !uniq.is_empty() {
        bag.insert(key, uniq.clone());
        if std::fs::create_dir_all(&dir).is_ok() {
            let _ = std::fs::remove_file(&file);
            if let Ok(mut tmp) = NamedTempFile::new_in(&dir) {
                use std::io::Write;
                let _ = serde_json::to_writer(&mut tmp, &bag);
                let _ = tmp.flush();
                let _ = tmp.persist(&file);
            }
        }
    }

    Ok(uniq)
}

pub fn discover_jest_list_tests_resilient_with_timeout(
    repo_root: &Path,
    jest_bin: &Path,
    jest_args: &[String],
    related_production_paths_abs: &[String],
    exclude_globs: &[String],
    no_cache: bool,
    timeout: Duration,
) -> Result<Vec<String>, RunError> {
    match discover_jest_list_tests_with_timeout(repo_root, jest_bin, jest_args, timeout) {
        Ok(listed) => Ok(listed),
        Err(RunError::TimedOut { .. }) if related_production_paths_abs.is_empty() => Ok(vec![]),
        Err(RunError::TimedOut { .. }) => {
            let mut key_parts = related_production_paths_abs
                .iter()
                .map(|abs| {
                    Path::new(abs)
                        .strip_prefix(repo_root)
                        .ok()
                        .map(|p| p.to_slash_lossy().to_string())
                        .unwrap_or_else(|| Path::new(abs).to_slash_lossy().to_string())
                })
                .collect::<Vec<_>>();
            key_parts.sort();
            let selection_key = key_parts.join("|");
            cached_related(repo_root, &selection_key, no_cache, || {
                find_related_tests_fast(
                    repo_root,
                    related_production_paths_abs,
                    &DEFAULT_TEST_GLOBS,
                    exclude_globs,
                    FAST_RELATED_TIMEOUT,
                )
            })
        }
        Err(other) => Err(other),
    }
}

pub fn discover_jest_list_tests_for_project(
    repo_root: &Path,
    jest_bin: &Path,
    jest_args: &[String],
    cfg_token: &str,
    cwd: &Path,
) -> Result<Vec<String>, RunError> {
    discover_jest_list_tests_for_project_with_timeout(
        repo_root,
        jest_bin,
        jest_args,
        cfg_token,
        cwd,
        JEST_LIST_TESTS_TIMEOUT,
    )
}

pub fn discover_jest_list_tests_for_project_with_timeout(
    _repo_root: &Path,
    jest_bin: &Path,
    jest_args: &[String],
    cfg_token: &str,
    cwd: &Path,
    timeout: Duration,
) -> Result<Vec<String>, RunError> {
    let mut args = jest_args.to_vec();
    args.extend(["--config".to_string(), cfg_token.to_string()]);
    args.push("--listTests".to_string());
    let display_command = format!(
        "{} {}",
        jest_bin.to_string_lossy(),
        args.iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    );
    let mut command = Command::new(jest_bin);
    command
        .args(&args)
        .current_dir(cwd)
        .env("CI", "1")
        .env("NODE_ENV", "test");
    let out = run_command_capture_with_timeout(command, display_command, timeout)?;
    if !out.status.success() {
        let exit_code = out.status.code().unwrap_or(1);
        let stderr_text = String::from_utf8_lossy(&out.stderr);
        let message = if stderr_text.trim().is_empty() {
            format!("exit {exit_code}")
        } else {
            stderr_text.to_string()
        };
        return Err(RunError::CommandFailed { message });
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(|l| std::path::Path::new(l).to_slash_lossy().to_string())
        .collect())
}

pub fn discover_jest_list_tests_for_project_with_patterns_with_timeout(
    _repo_root: &Path,
    jest_bin: &Path,
    jest_args: &[String],
    cfg_token: &str,
    cwd: &Path,
    patterns: &[String],
    timeout: Duration,
) -> Result<Vec<String>, RunError> {
    let mut args = jest_args.to_vec();
    args.extend(["--config".to_string(), cfg_token.to_string()]);
    args.push("--listTests".to_string());
    args.extend(patterns.iter().cloned());

    let display_command = format!(
        "{} {}",
        jest_bin.to_string_lossy(),
        args.iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    );
    let mut command = Command::new(jest_bin);
    command
        .args(&args)
        .current_dir(cwd)
        .env("CI", "1")
        .env("NODE_ENV", "test");
    let out = run_command_capture_with_timeout(command, display_command, timeout)?;
    if !out.status.success() {
        let exit_code = out.status.code().unwrap_or(1);
        let stderr_text = String::from_utf8_lossy(&out.stderr);
        let message = if stderr_text.trim().is_empty() {
            format!("exit {exit_code}")
        } else {
            stderr_text.to_string()
        };
        return Err(RunError::CommandFailed { message });
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(|l| std::path::Path::new(l).to_slash_lossy().to_string())
        .collect())
}

pub fn jest_bin(repo_root: &Path) -> PathBuf {
    repo_root
        .join("node_modules")
        .join(".bin")
        .join(if cfg!(windows) { "jest.cmd" } else { "jest" })
}

fn read_json_map(path: &Path) -> Option<std::collections::BTreeMap<String, Vec<String>>> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<std::collections::BTreeMap<String, Vec<String>>>(&raw).ok()
}

fn git_test_status_hash(cwd: &Path) -> String {
    let status_paths = git_status_porcelain_paths(cwd);
    if status_paths.is_empty() {
        return String::new();
    }

    let mut classifier = headlamp_core::project::classify::ProjectClassifier::for_path(
        headlamp_core::selection::dependency_language::DependencyLanguageId::TsJs,
        cwd,
    );

    let mut test_paths: Vec<String> = status_paths
        .into_iter()
        .filter(|rel| {
            let abs = cwd.join(rel);
            matches!(
                classifier.classify_abs_path(&abs),
                headlamp_core::project::classify::FileKind::Test
                    | headlamp_core::project::classify::FileKind::Mixed
            )
        })
        .collect();
    test_paths.sort();
    test_paths.dedup();

    if test_paths.is_empty() {
        return String::new();
    }
    let joined = test_paths.join("\n");
    let mut h = Sha1::new();
    h.update(joined.as_bytes());
    let hex = hex::encode(h.finalize());
    let short: String = hex.chars().take(8).collect();
    format!(":{short}")
}

fn git_status_porcelain_paths(cwd: &Path) -> Vec<String> {
    let out = crate::git::git_command_in_repo(cwd)
        .args(["status", "--porcelain"])
        .output()
        .ok();
    let stdout = out
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    stdout
        .lines()
        .map(str::trim)
        .filter(|line| line.len() >= 4)
        .filter_map(|line| line.get(3..))
        .map(|path_part| path_part.trim())
        .filter(|path_part| !path_part.is_empty())
        .map(|path_part| {
            path_part
                .rsplit(" -> ")
                .next()
                .unwrap_or(path_part)
                .to_string()
        })
        .collect::<Vec<_>>()
}
