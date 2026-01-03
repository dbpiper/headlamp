use std::path::Path;
use std::time::Instant;

use headlamp_core::args::ParsedArgs;

use super::selection;

fn write_cargo_test_run_trace(
    repo_root: &Path,
    args: &ParsedArgs,
    started_at: Instant,
    payload: serde_json::Value,
) {
    headlamp_core::diagnostics_trace::maybe_write_run_trace(
        repo_root,
        "cargo-test",
        args,
        Some(started_at),
        payload,
    );
}

pub(super) fn trace_cargo_test_early_exit(
    repo_root: &Path,
    args: &ParsedArgs,
    started_at: Instant,
    changed_files_count: usize,
    selected_test_count: Option<usize>,
) {
    write_cargo_test_run_trace(
        repo_root,
        args,
        started_at,
        serde_json::json!({
            "changed_files_count": changed_files_count,
            "selected_test_count": selected_test_count,
            "early_exit": true,
            "exit_code": 0,
        }),
    );
}

pub(super) fn normalize_and_trace_cargo_test_coverage_abort(
    repo_root: &Path,
    args: &ParsedArgs,
    started_at: Instant,
    changed_files_count: usize,
    selection: &selection::CargoSelection,
    exit_code: i32,
) -> i32 {
    let normalized = if exit_code == 0 { 0 } else { 1 };
    write_cargo_test_run_trace(
        repo_root,
        args,
        started_at,
        serde_json::json!({
            "changed_files_count": changed_files_count,
            "selected_test_count": selection.selected_test_count,
            "coverage_aborted": true,
            "exit_code": normalized,
        }),
    );
    normalized
}

pub(super) fn trace_cargo_test_final_exit(
    repo_root: &Path,
    args: &ParsedArgs,
    started_at: Instant,
    changed_files_count: usize,
    selection: &selection::CargoSelection,
    final_exit: i32,
) {
    write_cargo_test_run_trace(
        repo_root,
        args,
        started_at,
        serde_json::json!({
            "changed_files_count": changed_files_count,
            "selected_test_count": selection.selected_test_count,
            "exit_code": final_exit,
        }),
    );
}
