mod cache;
mod fixture_repo;
mod git_utils;
mod headlamp_bin;
mod jest_bin;
mod worktrees;

pub use fixture_repo::*;
pub use headlamp_bin::*;
pub use worktrees::*;

use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::types::{ParityRunGroup, ParityRunSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunnerId {
    Jest,
    CargoTest,
    CargoNextest,
    Pytest,
}

impl RunnerId {
    pub fn as_runner_flag_value(self) -> &'static str {
        match self {
            RunnerId::Jest => "jest",
            RunnerId::CargoTest => "cargo-test",
            RunnerId::CargoNextest => "cargo-nextest",
            RunnerId::Pytest => "pytest",
        }
    }

    pub fn as_runner_label(self) -> &'static str {
        match self {
            RunnerId::Jest => "jest",
            RunnerId::CargoTest => "cargo_test",
            RunnerId::CargoNextest => "cargo_nextest",
            RunnerId::Pytest => "pytest",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CachedRunnerParitySide {
    pub spec: ParityRunSpec,
    pub exit: i32,
    pub raw: String,
    pub normalized: String,
    pub meta: crate::parity_meta::ParitySideMeta,
}

#[derive(Debug, Clone, Eq)]
pub(crate) struct RunnerParityCacheKey {
    pub repo: String,
    pub runner: RunnerId,
    pub columns: usize,
    pub args: Vec<String>,
    pub extra_env: Vec<(String, String)>,
}

impl PartialEq for RunnerParityCacheKey {
    fn eq(&self, other: &Self) -> bool {
        self.repo == other.repo
            && self.runner == other.runner
            && self.columns == other.columns
            && self.args == other.args
            && self.extra_env == other.extra_env
    }
}

impl Hash for RunnerParityCacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.repo.hash(state);
        self.runner.hash(state);
        self.columns.hash(state);
        self.args.hash(state);
        self.extra_env.hash(state);
    }
}

pub fn assert_runner_parity_tty_snapshot_all_four_env(
    repo: &Path,
    headlamp_bin: &Path,
    case: &str,
    runners: &[(RunnerId, &[&str])],
    extra_env: &[(&str, String)],
) {
    let canonical =
        runner_parity_tty_all_four_canonical_env(repo, headlamp_bin, case, runners, extra_env);
    let snapshot_name = snapshot_name_from_case(case);
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path(Path::new("tests/snapshots/runner_parity"));
    {
        let _timing = crate::timing::TimingGuard::start(format!("case snapshot case={case}"));
        settings.bind(|| {
            insta::assert_snapshot!(snapshot_name, canonical);
        });
    }
}

pub fn runner_parity_tty_all_four_canonical_env(
    repo: &Path,
    headlamp_bin: &Path,
    case: &str,
    runners: &[(RunnerId, &[&str])],
    extra_env: &[(&str, String)],
) -> String {
    let _timing = crate::timing::TimingGuard::start(format!("case total case={case}"));
    let repo_cache_key = format!(
        "{}:{}",
        headlamp::fast_related::stable_repo_key_hash_12(repo),
        git_utils::repo_state_token(repo)
    );
    let columns = 120;
    let sides = std::thread::scope(|scope| {
        runners
            .iter()
            .map(|(runner, args)| {
                let runner_id = *runner;
                let runner_args = *args;
                let repo_cache_key = repo_cache_key.clone();
                scope.spawn(move || {
                    cache::run_and_normalize_cached(
                        repo,
                        repo_cache_key.as_str(),
                        headlamp_bin,
                        columns,
                        runner_id,
                        runner_args,
                        extra_env,
                    )
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });

    {
        let _timing = crate::timing::TimingGuard::start(format!("case compare case={case}"));
        let compare = crate::parity_meta::ParityCompareInput {
            sides: sides
                .iter()
                .map(|side| crate::parity_meta::ParityCompareSideInput {
                    label: side.spec.side_label.clone(),
                    exit: side.exit,
                    raw: side.raw.clone(),
                    normalized: side.normalized.clone(),
                    meta: side.meta.clone(),
                })
                .collect(),
        };
        let run_group = ParityRunGroup {
            sides: sides.iter().map(|side| side.spec.clone()).collect(),
        };
        crate::diagnostics_assert::assert_parity_with_diagnostics(
            repo,
            case,
            &compare,
            Some(&run_group),
        );
    }

    sides
        .first()
        .map(|s| s.normalized.clone())
        .unwrap_or_default()
}

pub fn assert_runner_parity_tty_all_four(
    repo: &Path,
    headlamp_bin: &Path,
    case: &str,
    runners: &[(RunnerId, &[&str])],
) {
    assert_runner_parity_tty_all_four_env(repo, headlamp_bin, case, runners, &[]);
}

pub fn assert_runner_parity_tty_all_four_env(
    repo: &Path,
    headlamp_bin: &Path,
    case: &str,
    runners: &[(RunnerId, &[&str])],
    extra_env: &[(&str, String)],
) {
    let columns = 120;
    let mut run_specs: Vec<ParityRunSpec> = vec![];
    let mut sides: Vec<crate::parity_meta::ParityCompareSideInput> = vec![];
    runners.iter().for_each(|(runner, args)| {
        let (spec, exit, raw) = crate::parity_run::run_headlamp_with_args_tty_env(
            repo,
            headlamp_bin,
            columns,
            runner.as_runner_flag_value(),
            args,
            extra_env,
        );
        let raw_bytes = raw.len();
        let raw_lines = raw.lines().count();
        let (normalized, normalization_meta) =
            crate::normalize::normalize_tty_ui_runner_parity_with_meta(raw.clone(), repo);
        let normalized_bytes = normalized.len();
        let normalized_lines = normalized.lines().count();
        let side_label = spec.side_label.clone();
        run_specs.push(spec);
        sides.push(crate::parity_meta::ParityCompareSideInput {
            label: side_label,
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
        });
    });

    let compare = crate::parity_meta::ParityCompareInput { sides };
    let run_group = ParityRunGroup { sides: run_specs };
    crate::diagnostics_assert::assert_parity_with_diagnostics(
        repo,
        case,
        &compare,
        Some(&run_group),
    );
}

fn snapshot_name_from_case(case: &str) -> String {
    case.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' => c.to_ascii_lowercase(),
            _ => '_',
        })
        .collect::<String>()
}
