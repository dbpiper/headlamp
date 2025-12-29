use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::seed_match::SeedMatcher;
use crate::{fast_related, process, run::RunError};

const PYTEST_COLLECT_TIMEOUT: Duration = Duration::from_secs(25);

pub fn discover_pytest_test_files(
    repo_root: &Path,
    no_cache: bool,
) -> Result<Vec<PathBuf>, RunError> {
    if no_cache {
        return discover_pytest_test_files_with_timeout(repo_root, PYTEST_COLLECT_TIMEOUT);
    }

    let cache_root = fast_related::default_cache_root();
    let repo_key = fast_related::stable_repo_key_hash_12(repo_root);
    let dir = cache_root.join(repo_key);
    let file = dir.join("pytest-collect.json");

    let head = fast_related::git_short_head(repo_root).unwrap_or_else(|| "nohead".to_string());
    let key = format!(
        "{head}::{}",
        fast_related::stable_repo_key_hash_12(repo_root)
    );

    let mut bag: std::collections::BTreeMap<String, Vec<String>> =
        read_json_map(&file).unwrap_or_default();
    if let Some(hit) = bag.get(&key)
        && !hit.is_empty()
    {
        let paths = hit
            .iter()
            .map(|rel| repo_root.join(rel))
            .filter(|abs| abs.exists())
            .collect::<Vec<_>>();
        return Ok(paths);
    };

    let discovered = discover_pytest_test_files_with_timeout(repo_root, PYTEST_COLLECT_TIMEOUT)?;
    let mut rels = discovered
        .iter()
        .filter_map(|abs| abs.strip_prefix(repo_root).ok())
        .map(|rel| rel.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    rels.sort();
    rels.dedup();
    bag.insert(key, rels);
    if std::fs::create_dir_all(&dir).is_ok() {
        let _ = std::fs::remove_file(&file);
        if let Ok(mut tmp) = tempfile::NamedTempFile::new_in(&dir) {
            use std::io::Write;
            let _ = serde_json::to_writer(&mut tmp, &bag);
            let _ = tmp.flush();
            let _ = tmp.persist(&file);
        }
    }

    Ok(discovered)
}

fn discover_pytest_test_files_with_timeout(
    repo_root: &Path,
    timeout: Duration,
) -> Result<Vec<PathBuf>, RunError> {
    let mut command = Command::new(if cfg!(windows) {
        "pytest.exe"
    } else {
        "pytest"
    });
    let pythonpath = crate::pythonpath::build_pytest_pythonpath(
        repo_root,
        &[],
        std::env::var("PYTHONPATH").ok(),
    );
    command
        .args(["--collect-only", "-q"])
        .current_dir(repo_root)
        .env("CI", "1")
        .env("PYTHONPATH", pythonpath);
    let display_command = "pytest --collect-only -q".to_string();
    let out = process::run_command_capture_with_timeout(command, display_command, timeout)?;
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
    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(parse_pytest_collect_output(repo_root, &stdout))
}

fn parse_pytest_collect_output(repo_root: &Path, stdout: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = stdout
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .filter_map(|l| l.split_once("::").map(|(file, _)| file).or(Some(l)))
        .filter(|file| file.ends_with(".py"))
        .map(|file| repo_root.join(file))
        .filter(|abs| abs.exists())
        .collect();
    out.sort();
    out.dedup();
    out
}

fn read_json_map(path: &Path) -> Option<std::collections::BTreeMap<String, Vec<String>>> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<std::collections::BTreeMap<String, Vec<String>>>(&raw).ok()
}

pub fn changed_seeds(repo_root: &Path, changed: &[PathBuf]) -> Vec<String> {
    changed
        .iter()
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("py"))
        .filter_map(|p| p.strip_prefix(repo_root).ok())
        .flat_map(|rel| {
            use path_slash::PathExt;
            let rel = rel.to_slash_lossy();
            let no_ext = rel.strip_suffix(".py").unwrap_or(&rel);
            let module = no_ext.replace('/', ".");
            let base = no_ext.split('/').next_back().unwrap_or(no_ext).to_string();
            vec![module, base]
        })
        .collect::<Vec<_>>()
}

pub fn filter_tests_by_seeds(tests: &[PathBuf], seeds: &[String]) -> Vec<PathBuf> {
    let Some(matcher) = SeedMatcher::new(seeds) else {
        return vec![];
    };
    tests
        .iter()
        .filter(|&p| matcher.is_match_file_name_or_body(p))
        .cloned()
        .collect()
}
