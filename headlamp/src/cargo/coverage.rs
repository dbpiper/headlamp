use std::path::Path;

use duct::cmd as duct_cmd;

use headlamp_core::args::ParsedArgs;
use headlamp_core::coverage::istanbul_pretty::format_istanbul_pretty_from_lcov_report;
use headlamp_core::coverage::lcov::read_repo_lcov_filtered;
use headlamp_core::coverage::print::PrintOpts;

use super::paths::headlamp_cargo_target_dir_for_duct;
use crate::profile;

pub(super) fn has_cargo_nextest(repo_root: &Path) -> bool {
    duct_cmd("cargo", ["nextest", "--version"])
        .dir(repo_root)
        .env(
            "CARGO_TARGET_DIR",
            headlamp_cargo_target_dir_for_duct(repo_root),
        )
        .stdout_capture()
        .stderr_capture()
        .unchecked()
        .run()
        .ok()
        .is_some_and(|o| o.status.success())
}

pub(super) fn has_cargo_llvm_cov(repo_root: &Path) -> bool {
    duct_cmd("cargo", ["llvm-cov", "--version"])
        .dir(repo_root)
        .env(
            "CARGO_TARGET_DIR",
            headlamp_cargo_target_dir_for_duct(repo_root),
        )
        .stdout_capture()
        .stderr_capture()
        .unchecked()
        .run()
        .ok()
        .is_some_and(|o| o.status.success())
}

pub(super) fn print_lcov(repo_root: &Path, args: &ParsedArgs) -> bool {
    let filtered = {
        let _span = profile::span("read lcov + glob filter");
        read_repo_lcov_filtered(repo_root, &args.include_globs, &args.exclude_globs)
    };
    let Some(filtered) = filtered else {
        return false;
    };
    let filtered = {
        let _span = profile::span("apply statement hits (llvm-cov json)");
        match crate::coverage::llvm_cov_json::read_repo_llvm_cov_json_statement_hits(repo_root) {
            Some(statement_hits_by_path) => crate::coverage::model::apply_statement_hits_to_report(
                filtered,
                statement_hits_by_path,
            ),
            None => filtered,
        }
    };
    let print_opts =
        PrintOpts::for_run(args, headlamp_core::format::terminal::is_output_terminal());
    let threshold_failure_lines = args.coverage_thresholds.as_ref().map(|thresholds| {
        headlamp_core::coverage::thresholds::threshold_failure_lines(
            thresholds,
            headlamp_core::coverage::thresholds::compute_totals_from_report(&filtered),
        )
    });
    let pretty = {
        let _span = profile::span("format istanbul pretty (from lcov)");
        format_istanbul_pretty_from_lcov_report(
            repo_root,
            filtered,
            &print_opts,
            &[],
            &args.include_globs,
            &args.exclude_globs,
            args.coverage_detail,
        )
    };
    println!("{pretty}");
    threshold_failure_lines.is_some_and(|lines| {
        if lines.is_empty() {
            return false;
        }
        headlamp_core::coverage::thresholds::print_threshold_failure_summary(&lines);
        true
    })
}
