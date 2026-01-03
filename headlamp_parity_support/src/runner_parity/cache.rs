use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use super::{CachedRunnerParitySide, RunnerId, RunnerParityCacheKey};

type RunnerParityRunCache =
    Mutex<HashMap<RunnerParityCacheKey, Arc<OnceLock<Arc<CachedRunnerParitySide>>>>>;

#[derive(Debug, Clone, Copy)]
pub(crate) struct RunAndNormalizeCachedRequest<'a> {
    pub repo: &'a Path,
    pub repo_cache_key: &'a str,
    pub case_id: &'a str,
    pub headlamp_bin: &'a Path,
    pub columns: usize,
    pub runner: RunnerId,
    pub args: &'a [&'a str],
    pub extra_env: &'a [(&'a str, String)],
}

fn runner_parity_run_cache() -> &'static RunnerParityRunCache {
    static CACHE: OnceLock<RunnerParityRunCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn sorted_extra_env(extra_env: &[(&str, String)]) -> Vec<(String, String)> {
    let mut out = extra_env
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect::<Vec<_>>();
    out.sort_by(|(ka, va), (kb, vb)| ka.cmp(kb).then_with(|| va.cmp(vb)));
    out
}

fn mk_runner_parity_cache_key(
    repo_cache_key: &str,
    runner: RunnerId,
    columns: usize,
    args: &[&str],
    extra_env: &[(&str, String)],
) -> RunnerParityCacheKey {
    RunnerParityCacheKey {
        repo: repo_cache_key.to_string(),
        runner,
        columns,
        args: args.iter().map(|s| (*s).to_string()).collect(),
        extra_env: sorted_extra_env(extra_env),
    }
}

pub(crate) fn run_and_normalize_cached(
    request: RunAndNormalizeCachedRequest<'_>,
) -> Arc<CachedRunnerParitySide> {
    let key = mk_runner_parity_cache_key(
        request.repo_cache_key,
        request.runner,
        request.columns,
        request.args,
        request.extra_env,
    );
    let cell = {
        let mut locked = runner_parity_run_cache().lock().unwrap();
        locked
            .entry(key)
            .or_insert_with(|| Arc::new(OnceLock::new()))
            .clone()
    };
    cell.get_or_init(|| {
        Arc::new(run_and_normalize(
            request.repo,
            request.case_id,
            request.headlamp_bin,
            request.columns,
            request.runner,
            request.args,
            request.extra_env,
        ))
    })
    .clone()
}

fn run_and_normalize(
    repo: &Path,
    case_id: &str,
    headlamp_bin: &Path,
    columns: usize,
    runner: RunnerId,
    args: &[&str],
    extra_env: &[(&str, String)],
) -> CachedRunnerParitySide {
    if runner == RunnerId::Jest {
        super::jest_bin::ensure_repo_local_jest_bin(repo);
    }
    let (spec, exit, raw) = {
        let _timing = crate::timing::TimingGuard::start(format!(
            "runner exec case_repo={} runner={}",
            repo.to_string_lossy(),
            runner.as_runner_label()
        ));
        crate::parity_run::run_headlamp_with_args_tty_env(
            repo,
            headlamp_bin,
            columns,
            runner.as_runner_flag_value(),
            args,
            extra_env,
            Some(case_id),
        )
    };
    let raw_bytes = raw.len();
    let raw_lines = raw.lines().count();
    let (normalized, normalization_meta) = {
        let _timing = crate::timing::TimingGuard::start(format!(
            "runner normalize case_repo={} runner={}",
            repo.to_string_lossy(),
            runner.as_runner_label()
        ));
        crate::normalize::normalize_tty_ui_runner_parity_with_meta(raw.clone(), repo)
    };
    let normalized_bytes = normalized.len();
    let normalized_lines = normalized.lines().count();
    CachedRunnerParitySide {
        spec,
        exit,
        raw,
        normalized,
        meta: crate::parity_meta::ParitySideMeta {
            raw_bytes,
            raw_lines,
            normalized_bytes,
            normalized_lines,
            normalization: normalization_meta,
        },
    }
}
