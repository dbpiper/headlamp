use clap::Parser;
use indexmap::IndexSet;

use crate::config::{ChangedMode, CoverageMode, CoverageThresholds, CoverageUi};
use crate::selection::dependency_language::DependencyLanguageId;

use super::cli::HeadlampCli;
use super::helpers::{
    infer_glob_from_selection_path, is_path_like, is_test_like_token, parse_changed_mode_string,
    parse_coverage_detail, parse_coverage_mode, parse_coverage_ui,
};
use super::tokens::split_headlamp_tokens;
use super::types::{CoverageDetail, DEFAULT_EXCLUDE, DEFAULT_INCLUDE, ParsedArgs};

pub fn derive_args(cfg_tokens: &[String], argv: &[String], is_tty: bool) -> ParsedArgs {
    let tokens = combined_tokens(cfg_tokens, argv);
    let (hl_tokens, passthrough) = split_headlamp_tokens(&tokens);
    let mut clap_argv = vec!["headlamp".to_string()];
    clap_argv.extend(hl_tokens);

    let parsed_cli = HeadlampCli::try_parse_from(&clap_argv).unwrap_or_default();
    derive_args_from_parsed_cli(parsed_cli, passthrough, is_tty)
}

#[derive(Debug)]
struct CommonArgs {
    collect_coverage: bool,
    coverage_abort_on_failure: bool,
    only_failures: bool,
    show_logs: bool,
    sequential: bool,
    ci: bool,
    watch: bool,
    verbose: bool,
    no_cache: bool,
    bootstrap_command: Option<String>,
    coverage_ui: CoverageUi,
    include_globs: Vec<String>,
    exclude_globs: Vec<String>,
    editor_cmd: Option<String>,
    workspace_root: Option<String>,
    coverage_thresholds: Option<CoverageThresholds>,
    coverage_detail: Option<CoverageDetail>,
    coverage_show_code: bool,
    coverage_mode: CoverageMode,
    coverage_max_files: Option<u32>,
    coverage_max_hotspots: Option<u32>,
    coverage_page_fit: bool,
    changed: Option<ChangedMode>,
    changed_depth: Option<u32>,
    dependency_language: Option<DependencyLanguageId>,
}

#[derive(Debug)]
struct SelectionParse {
    selection_specified: bool,
    selection_paths: Vec<String>,
    runner_args: Vec<String>,
}

fn combined_tokens(cfg_tokens: &[String], argv: &[String]) -> Vec<String> {
    cfg_tokens
        .iter()
        .chain(argv.iter())
        .cloned()
        .collect::<Vec<_>>()
}

fn derive_args_from_parsed_cli(
    parsed_cli: HeadlampCli,
    passthrough: Vec<String>,
    is_tty: bool,
) -> ParsedArgs {
    let common = parse_common_flags(&parsed_cli, is_tty);
    let selection = parse_selection_from_passthrough(passthrough, common.changed.is_some());
    build_parsed_args(common, selection)
}

fn parse_common_flags(parsed_cli: &HeadlampCli, is_tty: bool) -> CommonArgs {
    let ci = parsed_cli.ci;
    CommonArgs {
        collect_coverage: parsed_cli.coverage,
        coverage_abort_on_failure: parsed_cli.coverage_abort_on_failure,
        only_failures: parsed_cli.only_failures,
        show_logs: parsed_cli.show_logs,
        sequential: parsed_cli.sequential,
        ci,
        watch: !ci && (parsed_cli.watch || parsed_cli.watch_all),
        verbose: parsed_cli.verbose,
        no_cache: parsed_cli.no_cache,
        bootstrap_command: parsed_cli.bootstrap_command.clone(),
        coverage_ui: coverage_ui_from_cli(parsed_cli),
        include_globs: parsed_cli.coverage_include.clone(),
        exclude_globs: parsed_cli.coverage_exclude.clone(),
        editor_cmd: parsed_cli.coverage_editor.clone(),
        workspace_root: parsed_cli.coverage_root.clone(),
        coverage_thresholds: coverage_thresholds_from_cli(parsed_cli),
        coverage_detail: parsed_cli
            .coverage_detail
            .as_deref()
            .and_then(parse_coverage_detail),
        coverage_show_code: parsed_cli.coverage_show_code.unwrap_or(is_tty),
        coverage_mode: coverage_mode_from_cli(parsed_cli),
        coverage_max_files: parsed_cli.coverage_max_files,
        coverage_max_hotspots: parsed_cli.coverage_max_hotspots,
        coverage_page_fit: parsed_cli.coverage_page_fit.unwrap_or(is_tty),
        changed: parsed_cli
            .changed
            .as_deref()
            .and_then(parse_changed_mode_string),
        changed_depth: parsed_cli.changed_depth,
        dependency_language: dependency_language_from_cli(parsed_cli),
    }
}

fn coverage_ui_from_cli(parsed_cli: &HeadlampCli) -> CoverageUi {
    parsed_cli
        .coverage_ui
        .as_deref()
        .or(parsed_cli.coverage_ui_alt.as_deref())
        .map(parse_coverage_ui)
        .unwrap_or(CoverageUi::Both)
}

fn dependency_language_from_cli(parsed_cli: &HeadlampCli) -> Option<DependencyLanguageId> {
    parsed_cli
        .dependency_language
        .as_deref()
        .or(parsed_cli.dependency_language_alt.as_deref())
        .and_then(DependencyLanguageId::parse)
}

fn coverage_mode_from_cli(parsed_cli: &HeadlampCli) -> CoverageMode {
    let mut mode = parsed_cli
        .coverage_mode
        .as_deref()
        .map(parse_coverage_mode)
        .unwrap_or(CoverageMode::Auto);
    if parsed_cli.coverage_compact {
        mode = CoverageMode::Compact;
    }
    mode
}

fn coverage_thresholds_from_cli(parsed_cli: &HeadlampCli) -> Option<CoverageThresholds> {
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
}

fn build_parsed_args(common: CommonArgs, selection: SelectionParse) -> ParsedArgs {
    let (include_globs_final, exclude_globs_final) = globs_final(&common, &selection);
    let (coverage_detail, coverage_mode) = coverage_defaults(
        common.coverage_detail,
        common.coverage_mode,
        selection.selection_specified,
    );

    ParsedArgs {
        runner_args: selection.runner_args,
        selection_paths: selection
            .selection_paths
            .into_iter()
            .collect::<IndexSet<_>>()
            .into_iter()
            .collect::<Vec<_>>(),
        selection_specified: selection.selection_specified,
        watch: common.watch,
        ci: common.ci,
        verbose: common.verbose,
        no_cache: common.no_cache,
        collect_coverage: common.collect_coverage,
        coverage_ui: common.coverage_ui,
        coverage_abort_on_failure: common.coverage_abort_on_failure,
        coverage_detail,
        coverage_show_code: common.coverage_show_code,
        coverage_mode,
        coverage_max_files: common.coverage_max_files,
        coverage_max_hotspots: common.coverage_max_hotspots,
        coverage_page_fit: common.coverage_page_fit,
        coverage_thresholds: common.coverage_thresholds,
        include_globs: include_globs_final,
        exclude_globs: exclude_globs_final,
        editor_cmd: common.editor_cmd,
        workspace_root: common.workspace_root,
        only_failures: common.only_failures,
        show_logs: common.show_logs,
        sequential: common.sequential,
        bootstrap_command: common.bootstrap_command,
        changed: common.changed,
        changed_depth: common.changed_depth,
        dependency_language: common.dependency_language,
    }
}

fn globs_final(common: &CommonArgs, selection: &SelectionParse) -> (Vec<String>, Vec<String>) {
    let inferred_from_selection = selection
        .selection_paths
        .iter()
        .filter(|p| is_path_like(p))
        .map(|p| infer_glob_from_selection_path(p))
        .collect::<Vec<_>>();
    let include = include_globs_final(
        &common.include_globs,
        selection
            .selection_paths
            .iter()
            .any(|p| is_test_like_token(p)),
        inferred_from_selection,
    );
    let exclude = exclude_globs_final(&common.exclude_globs);
    (include, exclude)
}

fn coverage_defaults(
    coverage_detail: Option<CoverageDetail>,
    coverage_mode: CoverageMode,
    selection_specified: bool,
) -> (Option<CoverageDetail>, CoverageMode) {
    (
        coverage_detail_final(coverage_detail, selection_specified),
        coverage_mode_final(coverage_mode, selection_specified),
    )
}

fn parse_selection_from_passthrough(
    passthrough: Vec<String>,
    selection_specified_from_changed: bool,
) -> SelectionParse {
    let mut selection_specified = selection_specified_from_changed;
    let mut selection_paths: Vec<String> = vec![];
    let mut runner_args: Vec<String> = vec![];
    let mut pending_value_for_runner_flag: Option<&'static str> = None;

    for tok in passthrough {
        let (next_pending, handled) = handle_passthrough_token(&tok, pending_value_for_runner_flag);
        if handled {
            selection_specified = true;
        }
        if let Some(flag) = next_pending {
            runner_args.push(tok);
            pending_value_for_runner_flag = Some(flag);
            continue;
        }
        if pending_value_for_runner_flag.take().is_some() {
            runner_args.push(tok);
            continue;
        }
        if tok == "--" {
            continue;
        }
        if is_path_like(&tok) || is_test_like_token(&tok) {
            selection_specified = true;
            selection_paths.push(tok);
        } else {
            runner_args.push(tok);
        }
    }

    SelectionParse {
        selection_specified,
        selection_paths,
        runner_args,
    }
}

fn handle_passthrough_token(
    tok: &str,
    pending_value_for_runner_flag: Option<&'static str>,
) -> (Option<&'static str>, bool) {
    if tok == "--" {
        return (None, true);
    }
    if pending_value_for_runner_flag.is_some() {
        return (None, true);
    }
    if matches!(tok, "--testPathPattern" | "--testNamePattern" | "-t") {
        return (Some("-t"), true);
    }
    if tok.starts_with("--testPathPattern=") || tok.starts_with("--testNamePattern=") {
        return (None, true);
    }
    (None, false)
}

fn include_globs_final(
    include_globs: &[String],
    selection_looks_like_test_path: bool,
    inferred_from_selection: Vec<String>,
) -> Vec<String> {
    if !include_globs.is_empty() {
        return include_globs.to_vec();
    }
    if selection_looks_like_test_path {
        return DEFAULT_INCLUDE
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
    }
    if !inferred_from_selection.is_empty() {
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
    }
}

fn exclude_globs_final(exclude_globs: &[String]) -> Vec<String> {
    if !exclude_globs.is_empty() {
        exclude_globs.to_vec()
    } else {
        DEFAULT_EXCLUDE
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    }
}

fn coverage_detail_final(
    coverage_detail: Option<CoverageDetail>,
    selection_specified: bool,
) -> Option<CoverageDetail> {
    coverage_detail.or_else(|| selection_specified.then_some(CoverageDetail::Auto))
}

fn coverage_mode_final(coverage_mode: CoverageMode, selection_specified: bool) -> CoverageMode {
    if matches!(coverage_mode, CoverageMode::Auto) && selection_specified {
        CoverageMode::Compact
    } else {
        coverage_mode
    }
}
