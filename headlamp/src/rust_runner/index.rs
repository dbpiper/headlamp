use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::args::ParsedArgs;
use crate::cargo::selection::CargoSelection;
use crate::run::RunError;

use super::cargo_build::{BuiltTestBinary, build_test_binaries_via_cargo_no_run};

#[derive(Debug, Clone)]
pub(crate) struct TestBinary {
    pub(crate) executable: PathBuf,
    pub(crate) suite_source_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedBinaryIndex {
    repo_root: String,
    fingerprint: String,
    binaries: Vec<CachedBinary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedBinary {
    executable: String,
    suite_source_path: String,
}

pub(crate) fn load_or_build_binary_index(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    selection: &CargoSelection,
) -> Result<Vec<TestBinary>, RunError> {
    let cache_file = rust_cache_file(session);
    if args.no_cache {
        let built = build_and_persist(repo_root, args, session, selection, &cache_file)?;
        return Ok(built);
    }

    let fingerprint = compute_fingerprint(repo_root, selection);
    let repo_root_key = normalize_repo_root_key(repo_root);
    if let Some(hit) = try_load_cache(&cache_file, &repo_root_key, &fingerprint) {
        return Ok(hit);
    }
    let built = build_and_persist(repo_root, args, session, selection, &cache_file)?;
    Ok(built)
}

fn build_and_persist(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
    selection: &CargoSelection,
    cache_file: &Path,
) -> Result<Vec<TestBinary>, RunError> {
    let built = build_test_binaries_via_cargo_no_run(
        repo_root,
        args,
        session,
        &selection.extra_cargo_args,
    )?;
    let binaries = built.into_iter().map(map_built_binary).collect::<Vec<_>>();
    let fingerprint = compute_fingerprint(repo_root, selection);
    let cached = CachedBinaryIndex {
        repo_root: normalize_repo_root_key(repo_root),
        fingerprint,
        binaries: binaries
            .iter()
            .map(|b| CachedBinary {
                executable: b.executable.to_string_lossy().to_string(),
                suite_source_path: b.suite_source_path.clone(),
            })
            .collect(),
    };

    let _ = std::fs::create_dir_all(cache_file.parent().unwrap_or_else(|| Path::new(".")));
    let json = serde_json::to_vec(&cached).map_err(|e| RunError::Io(std::io::Error::other(e)))?;
    std::fs::write(cache_file, json).map_err(RunError::Io)?;
    Ok(binaries)
}

fn try_load_cache(
    cache_file: &Path,
    expected_repo_root: &str,
    expected_fingerprint: &str,
) -> Option<Vec<TestBinary>> {
    let bytes = std::fs::read(cache_file).ok()?;
    let cached: CachedBinaryIndex = serde_json::from_slice(&bytes).ok()?;
    if cached.repo_root != expected_repo_root {
        return None;
    }
    if cached.fingerprint != expected_fingerprint {
        return None;
    }
    let mut binaries = cached
        .binaries
        .into_iter()
        .map(|b| TestBinary {
            executable: PathBuf::from(b.executable),
            suite_source_path: b.suite_source_path,
        })
        .collect::<Vec<_>>();
    if binaries.is_empty() {
        return None;
    }
    if binaries.iter().any(|b| !b.executable.exists()) {
        return None;
    }
    binaries.sort_by(|a, b| a.executable.cmp(&b.executable));
    Some(binaries)
}

fn rust_cache_file(session: &crate::session::RunSession) -> PathBuf {
    let base = std::env::var_os("HEADLAMP_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| session.subdir("cache"));
    base.join("rust").join("binary_index.json")
}

fn normalize_repo_root_key(repo_root: &Path) -> String {
    dunce::canonicalize(repo_root)
        .unwrap_or_else(|_| repo_root.to_path_buf())
        .to_string_lossy()
        .to_string()
}

fn compute_fingerprint(repo_root: &Path, selection: &CargoSelection) -> String {
    use sha1::Digest as _;

    let mut hasher = sha1::Sha1::new();
    if crate::cargo::paths::nightly_rustc_exists(repo_root) {
        hasher.update(b"toolchain=nightly");
    } else {
        hasher.update(b"toolchain=stable");
    }
    if let Ok(lock_bytes) = std::fs::read(repo_root.join("Cargo.lock")) {
        hasher.update(lock_bytes);
    }
    if let Ok(toml_bytes) = std::fs::read(repo_root.join("Cargo.toml")) {
        hasher.update(toml_bytes);
    }
    selection
        .extra_cargo_args
        .iter()
        .for_each(|arg| hasher.update(arg.as_bytes()));
    hex::encode(hasher.finalize())
}

fn map_built_binary(built: BuiltTestBinary) -> TestBinary {
    TestBinary {
        executable: built.executable,
        suite_source_path: built.suite_source_path,
    }
}

#[cfg(test)]
mod index_test;
