use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;

use crate::args::ParsedArgs;

#[derive(Debug, Clone, Serialize)]
pub struct RunTrace {
    pub schema_version: u32,
    pub runner: String,
    pub repo_root: String,
    pub started_at_unix_ms: Option<u128>,
    pub elapsed_ms: Option<u128>,
    pub args: ArgsSummary,
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArgsSummary {
    pub ci: bool,
    pub verbose: bool,
    pub watch: bool,
    pub no_cache: bool,
    pub sequential: bool,
    pub show_logs: bool,
    pub only_failures: bool,
    pub collect_coverage: bool,
    pub coverage_ui: String,
    pub changed: Option<String>,
    pub changed_depth: Option<u32>,
    pub selection_paths: Vec<String>,
    pub runner_args: Vec<String>,
}

fn diagnostics_dir() -> Option<PathBuf> {
    std::env::var("HEADLAMP_DIAGNOSTICS_DIR")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

pub fn maybe_write_run_trace(
    repo_root: &Path,
    runner: &str,
    args: &ParsedArgs,
    started_at: Option<Instant>,
    extra: serde_json::Value,
) {
    let Some(dir) = diagnostics_dir() else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let trace_path = dir.join("run_trace.json");

    let started_at_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis());
    let elapsed_ms = started_at.map(|t| t.elapsed().as_millis());

    let trace = RunTrace {
        schema_version: 1,
        runner: runner.to_string(),
        repo_root: repo_root.to_string_lossy().to_string(),
        started_at_unix_ms,
        elapsed_ms,
        args: ArgsSummary {
            ci: args.ci,
            verbose: args.verbose,
            watch: args.watch,
            no_cache: args.no_cache,
            sequential: args.sequential,
            show_logs: args.show_logs,
            only_failures: args.only_failures,
            collect_coverage: args.collect_coverage,
            coverage_ui: format!("{:?}", args.coverage_ui),
            changed: args.changed.map(|m| format!("{m:?}")),
            changed_depth: args.changed_depth,
            selection_paths: args.selection_paths.clone(),
            runner_args: args.runner_args.clone(),
        },
        extra,
    };

    if let Ok(file) = std::fs::File::create(trace_path) {
        let _ = serde_json::to_writer_pretty(file, &trace);
    }
}
