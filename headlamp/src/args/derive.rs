use clap::Parser;
use indexmap::IndexSet;

use crate::config::{CoverageMode, CoverageThresholds, CoverageUi};
use crate::selection::dependency_language::DependencyLanguageId;

use super::cli::HeadlampCli;
use super::helpers::{
    infer_glob_from_selection_path, is_path_like, is_test_like_token, parse_changed_mode_string,
    parse_coverage_detail, parse_coverage_mode, parse_coverage_ui,
};
use super::tokens::split_headlamp_tokens;
use super::types::{CoverageDetail, DEFAULT_EXCLUDE, DEFAULT_INCLUDE, ParsedArgs};

pub fn derive_args(cfg_tokens: &[String], argv: &[String], is_tty: bool) -> ParsedArgs {
    let tokens = cfg_tokens
        .iter()
        .chain(argv.iter())
        .cloned()
        .collect::<Vec<_>>();

    let (hl_tokens, passthrough) = split_headlamp_tokens(&tokens);
    let mut clap_argv = vec!["headlamp".to_string()];
    clap_argv.extend(hl_tokens);

    let parsed_cli = HeadlampCli::try_parse_from(&clap_argv).unwrap_or_default();

    let collect_coverage = parsed_cli.coverage;
    let coverage_abort_on_failure = parsed_cli.coverage_abort_on_failure;
    let only_failures = parsed_cli.only_failures;
    let show_logs = parsed_cli.show_logs;
    let sequential = parsed_cli.sequential;
    let ci = parsed_cli.ci;
    let watch = !ci && (parsed_cli.watch || parsed_cli.watch_all);
    let verbose = parsed_cli.verbose;
    let no_cache = parsed_cli.no_cache;
    let bootstrap_command: Option<String> = parsed_cli.bootstrap_command;

    let coverage_ui = parsed_cli
        .coverage_ui
        .or(parsed_cli.coverage_ui_alt)
        .as_deref()
        .map(parse_coverage_ui)
        .unwrap_or(CoverageUi::Both);

    let include_globs: Vec<String> = parsed_cli.coverage_include;
    let exclude_globs: Vec<String> = parsed_cli.coverage_exclude;
    let editor_cmd: Option<String> = parsed_cli.coverage_editor;
    let workspace_root: Option<String> = parsed_cli.coverage_root;

    let coverage_thresholds = {
        let any = parsed_cli.coverage_thresholds_lines.is_some()
            || parsed_cli.coverage_thresholds_functions.is_some()
            || parsed_cli.coverage_thresholds_branches.is_some()
            || parsed_cli.coverage_thresholds_statements.is_some();
        any.then_some(CoverageThresholds {
            lines: parsed_cli.coverage_thresholds_lines,
            functions: parsed_cli.coverage_thresholds_functions,
            branches: parsed_cli.coverage_thresholds_branches,
            statements: parsed_cli.coverage_thresholds_statements,
        })
    };

    let mut coverage_detail: Option<CoverageDetail> = parsed_cli
        .coverage_detail
        .as_deref()
        .and_then(parse_coverage_detail);

    let coverage_show_code: bool = parsed_cli.coverage_show_code.unwrap_or(is_tty);

    let mut coverage_mode: CoverageMode = parsed_cli
        .coverage_mode
        .as_deref()
        .map(parse_coverage_mode)
        .unwrap_or(CoverageMode::Auto);

    if parsed_cli.coverage_compact {
        coverage_mode = CoverageMode::Compact;
    }

    let coverage_max_files: Option<u32> = parsed_cli.coverage_max_files;
    let coverage_max_hotspots: Option<u32> = parsed_cli.coverage_max_hotspots;
    let coverage_page_fit: bool = parsed_cli.coverage_page_fit.unwrap_or(is_tty);

    let changed = parsed_cli
        .changed
        .as_deref()
        .and_then(parse_changed_mode_string);
    let changed_depth: Option<u32> = parsed_cli.changed_depth;
    let dependency_language: Option<DependencyLanguageId> = parsed_cli
        .dependency_language
        .or(parsed_cli.dependency_language_alt)
        .as_deref()
        .and_then(DependencyLanguageId::parse);

    let mut selection_specified = changed.is_some();
    let mut selection_paths: Vec<String> = vec![];
    let mut runner_args: Vec<String> = vec![];

    let selection_hint_flags_take_value: [&str; 3] =
        ["--testPathPattern", "--testNamePattern", "-t"];
    let selection_hint_flags_with_equals_prefix: [&str; 2] =
        ["--testPathPattern=", "--testNamePattern="];

    let mut pending_value_for_runner_flag: Option<&'static str> = None;
    for tok in passthrough {
        if tok == "--" {
            selection_specified = true;
            continue;
        }

        if pending_value_for_runner_flag.is_some() {
            selection_specified = true;
            runner_args.push(tok);
            pending_value_for_runner_flag = None;
            continue;
        }

        if selection_hint_flags_take_value.iter().any(|f| *f == tok) {
            selection_specified = true;
            runner_args.push(tok.clone());
            pending_value_for_runner_flag = Some(match tok.as_str() {
                "--testPathPattern" => "--testPathPattern",
                "--testNamePattern" => "--testNamePattern",
                "-t" => "-t",
                _ => "-t",
            });
            continue;
        }

        if selection_hint_flags_with_equals_prefix
            .iter()
            .any(|prefix| tok.starts_with(prefix))
        {
            selection_specified = true;
            runner_args.push(tok);
            continue;
        }

        if is_path_like(&tok) || is_test_like_token(&tok) {
            selection_specified = true;
            selection_paths.push(tok);
        } else {
            runner_args.push(tok);
        }
    }

    let selection_looks_like_test_path = selection_paths.iter().any(|p| is_test_like_token(p));
    let inferred_from_selection = selection_paths
        .iter()
        .filter(|p| is_path_like(p))
        .map(|p| infer_glob_from_selection_path(p))
        .collect::<Vec<_>>();

    let include_globs_final = if !include_globs.is_empty() {
        include_globs
    } else if selection_looks_like_test_path {
        DEFAULT_INCLUDE
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    } else if !inferred_from_selection.is_empty() {
        inferred_from_selection
            .into_iter()
            .collect::<IndexSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    } else {
        DEFAULT_INCLUDE
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    };

    let exclude_globs_final = if !exclude_globs.is_empty() {
        exclude_globs
    } else {
        DEFAULT_EXCLUDE
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    };

    if coverage_detail.is_none() && selection_specified {
        coverage_detail = Some(CoverageDetail::Auto);
    }
    if matches!(coverage_mode, CoverageMode::Auto) && selection_specified {
        coverage_mode = CoverageMode::Compact;
    }

    ParsedArgs {
        runner_args,
        selection_paths: selection_paths
            .into_iter()
            .collect::<IndexSet<_>>()
            .into_iter()
            .collect::<Vec<_>>(),
        selection_specified,
        watch,
        ci,
        verbose,
        no_cache,
        collect_coverage,
        coverage_ui,
        coverage_abort_on_failure,
        coverage_detail,
        coverage_show_code,
        coverage_mode,
        coverage_max_files,
        coverage_max_hotspots,
        coverage_page_fit,
        coverage_thresholds,
        include_globs: include_globs_final,
        exclude_globs: exclude_globs_final,
        editor_cmd,
        workspace_root,
        only_failures,
        show_logs,
        sequential,
        bootstrap_command,
        changed,
        changed_depth,
        dependency_language,
    }
}
