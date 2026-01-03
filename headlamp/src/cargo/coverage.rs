use std::path::Path;

use duct::cmd as duct_cmd;

use headlamp_core::args::ParsedArgs;
use headlamp_core::coverage::istanbul_pretty::format_istanbul_pretty_from_lcov_report;
use headlamp_core::coverage::lcov::read_lcov_filtered_from_path;
use headlamp_core::coverage::print::PrintOpts;

use super::paths::headlamp_cargo_target_dir_for_duct;
use crate::profile;

pub(super) fn has_cargo_nextest(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
) -> bool {
    duct_cmd("cargo", ["nextest", "--version"])
        .dir(repo_root)
        .env(
            "CARGO_TARGET_DIR",
            headlamp_cargo_target_dir_for_duct(args.keep_artifacts, repo_root, session),
        )
        .stdout_capture()
        .stderr_capture()
        .unchecked()
        .run()
        .ok()
        .is_some_and(|o| o.status.success())
}

pub(crate) fn print_lcov(
    repo_root: &Path,
    args: &ParsedArgs,
    session: &crate::session::RunSession,
) -> bool {
    let lcov_path = if args.keep_artifacts {
        repo_root.join("coverage").join("lcov.info")
    } else {
        session.subdir("coverage").join("rust").join("lcov.info")
    };
    let llvm_cov_json_path = if args.keep_artifacts {
        repo_root.join("coverage").join("coverage.json")
    } else {
        session
            .subdir("coverage")
            .join("rust")
            .join("coverage.json")
    };
    let filtered = {
        let _span = profile::span("read lcov + glob filter");
        read_lcov_filtered_from_path(
            repo_root,
            &lcov_path,
            &args.include_globs,
            &args.exclude_globs,
        )
    };
    let Some(filtered) = filtered else {
        return false;
    };
    let filtered = {
        let _span = profile::span("apply statement hits (llvm-cov json)");
        match crate::coverage::llvm_cov_json::read_llvm_cov_json_statement_hits_from_path(
            repo_root,
            &llvm_cov_json_path,
        ) {
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
