use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::parity_support::assert_parity_normalized_outputs;
use crate::parity_support::{ParityBinaries, normalize};

pub struct E2eRepo {
    pub repo: PathBuf,
    pub tmp: tempfile::TempDir,
    ts_shim_dir: PathBuf,
    rs_shim_dir: PathBuf,
}

pub struct TempFile {
    pub path: PathBuf,
    existed: bool,
    original: Vec<u8>,
}

impl TempFile {
    pub fn create_or_replace(path: PathBuf, contents: &[u8]) -> Option<Self> {
        let existed = path.exists();
        let original = existed
            .then(|| std::fs::read(&path).ok())
            .flatten()
            .unwrap_or_default();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&path, contents).ok()?;
        Some(Self {
            path,
            existed,
            original,
        })
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if self.existed {
            let _ = std::fs::write(&self.path, &self.original);
        } else {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub fn discover_repo() -> Option<PathBuf> {
    let explicit = std::env::var("HEADLAMP_PARITY_REPO_PATH")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from);
    if let Some(path) = explicit {
        return path.exists().then_some(path);
    }

    let root = std::env::var("HEADLAMP_PARITY_REPO_ROOT")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/Users/david/src"));
    find_first_matching_repo(&root)
}

pub fn setup_repo(repo: PathBuf, binaries: &ParityBinaries) -> Option<E2eRepo> {
    if !(repo.join("package.json").exists()
        && repo.join("node_modules").exists()
        && repo.join(".git").exists())
    {
        return None;
    }
    if !git_is_clean(&repo) {
        return None;
    }

    let tmp = tempfile::tempdir().ok()?;
    let ts_shim_dir = tmp.path().join("shim-ts");
    let rs_shim_dir = tmp.path().join("shim-rs");
    let _ = std::fs::create_dir_all(&ts_shim_dir);
    let _ = std::fs::create_dir_all(&rs_shim_dir);
    write_headlamp_shim(&ts_shim_dir, "ts", &binaries.ts_cli)?;
    write_headlamp_shim(&rs_shim_dir, "rs", &binaries.rust_bin)?;

    Some(E2eRepo {
        repo,
        tmp,
        ts_shim_dir,
        rs_shim_dir,
    })
}

pub fn create_test_file(repo: &Path, name: &str, contents: &str) -> Option<TempFile> {
    let tests_dir = repo.join("tests");
    let dir = if tests_dir.exists() {
        tests_dir
    } else {
        repo.join("__tests__")
    };
    let path = dir.join(name);
    TempFile::create_or_replace(path, contents.as_bytes())
}

pub fn run_npm_test_dev_parity_normalized(
    e2e: &E2eRepo,
    extra_args: &[&str],
) -> (i32, String, i32, String) {
    let cache_base = e2e.tmp.path().join("cache");
    let _ = std::fs::create_dir_all(&cache_base);

    let (code_ts, out_ts) = run_npm_test_dev(
        &e2e.repo,
        &e2e.ts_shim_dir,
        &cache_base.join(".cache-ts"),
        extra_args,
    );
    let (code_rs, out_rs) = run_npm_test_dev(
        &e2e.repo,
        &e2e.rs_shim_dir,
        &cache_base.join(".cache-rs"),
        extra_args,
    );
    let n_ts = normalize(out_ts, &e2e.repo);
    let n_rs = normalize(out_rs, &e2e.repo);
    (code_ts, n_ts, code_rs, n_rs)
}

pub fn assert_parity(
    e2e: &E2eRepo,
    case: &str,
    code_ts: i32,
    out_ts: &str,
    code_rs: i32,
    out_rs: &str,
) {
    assert_parity_normalized_outputs(&e2e.repo, case, code_ts, out_ts, code_rs, out_rs);
}

fn run_npm_test_dev(
    repo: &Path,
    shim_dir: &Path,
    cache_dir: &Path,
    extra_args: &[&str],
) -> (i32, String) {
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let mut cmd = Command::new(npm);
    cmd.current_dir(repo).args(["run", "test:dev"]);
    if !extra_args.is_empty() {
        cmd.arg("--");
        extra_args.iter().for_each(|arg| {
            cmd.arg(arg);
        });
    }

    let existing_path = std::env::var("PATH").unwrap_or_default();
    let shim_path = shim_dir.to_string_lossy().to_string();
    let joined = if cfg!(windows) {
        format!("{shim_path};{existing_path}")
    } else {
        format!("{shim_path}:{existing_path}")
    };

    cmd.env("CI", "1");
    cmd.env(
        "HEADLAMP_CACHE_DIR",
        cache_dir.to_string_lossy().to_string(),
    );
    cmd.env("PATH", joined);

    let out = cmd.output().unwrap();
    let code = out.status.code().unwrap_or(1);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    (code, format!("{stdout}{stderr}"))
}

fn write_headlamp_shim(dir: &Path, impl_kind: &str, target: &Path) -> Option<()> {
    let shim = dir.join("headlamp");
    let target_text = target.to_string_lossy();
    let js = match impl_kind {
        "ts" => format!(
            "#!/usr/bin/env node\n\
const {{ spawnSync }} = require('node:child_process');\n\
const result = spawnSync('node', [{path:?}, ...process.argv.slice(2)], {{ stdio: 'inherit' }});\n\
process.exit(result.status ?? 1);\n",
            path = target_text.as_ref(),
        ),
        "rs" => format!(
            "#!/usr/bin/env node\n\
const {{ spawnSync }} = require('node:child_process');\n\
const result = spawnSync({path:?}, process.argv.slice(2), {{ stdio: 'inherit' }});\n\
process.exit(result.status ?? 1);\n",
            path = target_text.as_ref(),
        ),
        _ => return None,
    };
    std::fs::write(&shim, js).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&shim).ok()?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&shim, perms).ok()?;
    }
    Some(())
}

fn read_json(path: &Path) -> Option<Value> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<Value>(&raw).ok()
}

fn script_contains_test_dev_headlamp(script: &str) -> bool {
    let normalized = script.to_ascii_lowercase();
    normalized.contains("headlamp")
        && normalized.contains("--changed=branch")
        && normalized.contains("--coverage")
        && normalized.contains("--onlyfailures")
}

fn looks_like_target_repo(repo: &Path) -> bool {
    let pkg = repo.join("package.json");
    if !(pkg.exists() && repo.join("node_modules").exists() && repo.join(".git").exists()) {
        return false;
    }
    let Some(json) = read_json(&pkg) else {
        return false;
    };
    json.get("scripts")
        .and_then(|v| v.as_object())
        .and_then(|scripts| scripts.get("test:dev").and_then(|v| v.as_str()))
        .is_some_and(script_contains_test_dev_headlamp)
}

fn list_dirs(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return vec![];
    };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect::<Vec<_>>()
}

fn is_noise_dir(path: &Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    matches!(
        name,
        "node_modules" | "dist" | "build" | "coverage" | "target" | ".git" | ".next"
    )
}

fn find_first_matching_repo(search_root: &Path) -> Option<PathBuf> {
    let mut frontier = vec![search_root.to_path_buf()];
    let mut candidates: Vec<PathBuf> = vec![];

    for _ in 0..=4 {
        let mut next: Vec<PathBuf> = vec![];
        frontier
            .iter()
            .flat_map(|dir| list_dirs(dir))
            .filter(|dir| !is_noise_dir(dir))
            .for_each(|dir| {
                if looks_like_target_repo(&dir) {
                    candidates.push(dir);
                } else {
                    next.push(dir);
                }
            });
        frontier = next;
        if !candidates.is_empty() {
            break;
        }
    }

    candidates.sort();
    candidates.into_iter().next()
}

fn git_is_clean(repo: &Path) -> bool {
    let out = Command::new("git")
        .current_dir(repo)
        .args(["status", "--porcelain"])
        .output()
        .ok();
    out.is_some_and(|o| o.status.success() && o.stdout.is_empty())
}
