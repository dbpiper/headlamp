use std::borrow::Cow;
use std::path::Path;

use clap::Parser;
use globset::{Glob, GlobSet, GlobSetBuilder};
use indexmap::IndexSet;
use once_cell::sync::Lazy;

use crate::config::{ChangedMode, CoverageMode, CoverageThresholds, CoverageUi, HeadlampConfig};
use crate::selection::dependency_language::DependencyLanguageId;

static TEST_LIKE_GLOBSET: Lazy<GlobSet> = Lazy::new(|| {
    let mut b = GlobSetBuilder::new();
    ["**/tests/**", "**/*.{test,spec}.{ts,tsx,js,jsx}"]
        .into_iter()
        .filter_map(|g| Glob::new(g).ok())
        .for_each(|g| {
            b.add(g);
        });
    b.build().unwrap_or_else(|_| GlobSet::empty())
});

#[derive(Debug, Clone, Parser, Default)]
#[command(
    name = "headlamp",
    disable_help_flag = true,
    disable_version_flag = true
)]
struct HeadlampCli {
    #[arg(
        long = "coverage",
        default_value_t = false,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    coverage: bool,

    #[arg(
        long = "coverage.abortOnFailure",
        default_value_t = false,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    coverage_abort_on_failure: bool,

    #[arg(long = "coverage-ui")]
    coverage_ui: Option<String>,

    #[arg(long = "coverageUi")]
    coverage_ui_alt: Option<String>,

    #[arg(long = "coverage.detail")]
    coverage_detail: Option<String>,

    #[arg(
        long = "coverage.showCode",
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    coverage_show_code: Option<bool>,

    #[arg(long = "coverage.mode")]
    coverage_mode: Option<String>,

    #[arg(long = "coverage.maxFiles")]
    coverage_max_files: Option<u32>,

    #[arg(long = "coverage.maxHotspots")]
    coverage_max_hotspots: Option<u32>,

    #[arg(long = "coverage.thresholds.lines")]
    coverage_thresholds_lines: Option<f64>,

    #[arg(long = "coverage.thresholds.functions")]
    coverage_thresholds_functions: Option<f64>,

    #[arg(long = "coverage.thresholds.branches")]
    coverage_thresholds_branches: Option<f64>,

    #[arg(long = "coverage.thresholds.statements")]
    coverage_thresholds_statements: Option<f64>,

    #[arg(
        long = "coverage.pageFit",
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    coverage_page_fit: Option<bool>,

    #[arg(long = "coverage.include", value_delimiter = ',')]
    coverage_include: Vec<String>,

    #[arg(long = "coverage.exclude", value_delimiter = ',')]
    coverage_exclude: Vec<String>,

    #[arg(long = "coverage.editor")]
    coverage_editor: Option<String>,

    #[arg(long = "coverage.root")]
    coverage_root: Option<String>,

    #[arg(
        long = "onlyFailures",
        default_value_t = false,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    only_failures: bool,

    #[arg(
        long = "showLogs",
        default_value_t = false,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    show_logs: bool,

    #[arg(
        long = "sequential",
        default_value_t = false,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    sequential: bool,

    #[arg(
        long = "watch",
        default_value_t = false,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    watch: bool,

    #[arg(
        long = "watchAll",
        default_value_t = false,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    watch_all: bool,

    #[arg(
        long = "ci",
        default_value_t = false,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    ci: bool,

    #[arg(
        long = "verbose",
        default_value_t = false,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    verbose: bool,

    #[arg(
        long = "no-cache",
        alias = "noCache",
        default_value_t = false,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    no_cache: bool,

    #[arg(long = "bootstrapCommand")]
    bootstrap_command: Option<String>,

    #[arg(long = "changed", num_args = 0..=1, default_missing_value = "all")]
    changed: Option<String>,

    #[arg(long = "changed.depth")]
    changed_depth: Option<u32>,

    #[arg(long = "coverage.compact", default_value_t = false)]
    coverage_compact: bool,

    #[arg(long = "dependency-language")]
    dependency_language: Option<String>,

    #[arg(long = "dependencyLanguage")]
    dependency_language_alt: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedArgs {
    pub runner_args: Vec<String>,
    pub selection_paths: Vec<String>,
    pub selection_specified: bool,

    pub watch: bool,
    pub ci: bool,
    pub verbose: bool,
    pub no_cache: bool,

    pub collect_coverage: bool,
    pub coverage_ui: CoverageUi,
    pub coverage_abort_on_failure: bool,
    pub coverage_detail: Option<CoverageDetail>,
    pub coverage_show_code: bool,
    pub coverage_mode: CoverageMode,
    pub coverage_max_files: Option<u32>,
    pub coverage_max_hotspots: Option<u32>,
    pub coverage_page_fit: bool,
    pub coverage_thresholds: Option<CoverageThresholds>,
    pub include_globs: Vec<String>,
    pub exclude_globs: Vec<String>,
    pub editor_cmd: Option<String>,
    pub workspace_root: Option<String>,

    pub only_failures: bool,
    pub show_logs: bool,
    pub sequential: bool,
    pub bootstrap_command: Option<String>,

    pub changed: Option<ChangedMode>,
    pub changed_depth: Option<u32>,

    pub dependency_language: Option<DependencyLanguageId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageDetail {
    Auto,
    All,
    Lines(u32),
}

pub const DEFAULT_INCLUDE: [&str; 6] = [
    "**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx", "**/*.rs", "**/*.py",
];

pub const DEFAULT_EXCLUDE: [&str; 7] = [
    "**/node_modules/**",
    "**/coverage/**",
    "**/dist/**",
    "**/build/**",
    "**/migrations/**",
    "**/__mocks__/**",
    "**/tests/**",
];

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

    let changed: Option<ChangedMode> = parsed_cli
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

pub fn config_tokens(cfg: &HeadlampConfig, argv: &[String]) -> Vec<String> {
    let mut tokens: Vec<String> = vec![];

    if let Some(cmd) = cfg
        .bootstrap_command
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        tokens.push(format!("--bootstrapCommand={cmd}"));
    }
    if cfg.sequential == Some(true) {
        tokens.push("--sequential".to_string());
    }
    if cfg.watch == Some(true) {
        tokens.push("--watch".to_string());
    }
    if cfg.ci == Some(true) {
        tokens.push("--ci".to_string());
    }
    if cfg.verbose == Some(true) {
        tokens.push("--verbose".to_string());
    }
    if cfg.no_cache == Some(true) {
        tokens.push("--no-cache".to_string());
    }
    if let Some(args) = cfg.jest_args.as_ref().filter(|a| !a.is_empty()) {
        tokens.extend(args.iter().cloned());
    }

    let argv_has_coverage = argv
        .iter()
        .any(|t| t == "--coverage" || t.starts_with("--coverage="));
    let coverage_always_on = matches!(
        cfg.coverage,
        Some(crate::config::CoverageConfig::Bool(true))
    );

    let coverage_obj = match cfg.coverage {
        Some(crate::config::CoverageConfig::Obj(ref obj)) => Some(obj),
        _ => cfg.coverage_section.as_ref(),
    };

    if coverage_always_on && !argv_has_coverage {
        tokens.push("--coverage".to_string());
    }

    if coverage_always_on || argv_has_coverage {
        let abort = coverage_obj
            .and_then(|o| o.abort_on_failure)
            .or(cfg.coverage_abort_on_failure);
        if let Some(v) = abort {
            tokens.push(format!(
                "--coverage.abortOnFailure={}",
                if v { "true" } else { "false" }
            ));
        }
        let mode = coverage_obj.and_then(|o| o.mode).or(cfg.coverage_mode);
        if let Some(m) = mode {
            let s = match m {
                CoverageMode::Compact => "compact",
                CoverageMode::Full => "full",
                CoverageMode::Auto => "auto",
            };
            tokens.push(format!("--coverage.mode={s}"));
        }
        let page_fit = coverage_obj
            .and_then(|o| o.page_fit)
            .or(cfg.coverage_page_fit);
        if let Some(v) = page_fit {
            tokens.push(format!(
                "--coverage.pageFit={}",
                if v { "true" } else { "false" }
            ));
        }
        if let Some(ui) = cfg.coverage_ui {
            let s = match ui {
                CoverageUi::Jest => "jest",
                CoverageUi::Both => "both",
            };
            tokens.push(format!("--coverage-ui={s}"));
        }
        if let Some(editor) = cfg
            .editor_cmd
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            tokens.push(format!("--coverage.editor={editor}"));
        }
        if let Some(include) = cfg.include.as_ref().filter(|v| !v.is_empty()) {
            tokens.push(format!("--coverage.include={}", include.join(",")));
        }
        if let Some(exclude) = cfg.exclude.as_ref().filter(|v| !v.is_empty()) {
            tokens.push(format!("--coverage.exclude={}", exclude.join(",")));
        }
        if let Some(max_files) = cfg.coverage_max_files {
            tokens.push(format!("--coverage.maxFiles={max_files}"));
        }
        if let Some(max_hotspots) = cfg.coverage_max_hotspots {
            tokens.push(format!("--coverage.maxHotspots={max_hotspots}"));
        }
        if let Some(thresholds) = coverage_obj.and_then(|o| o.thresholds.as_ref()) {
            if let Some(v) = thresholds.lines {
                tokens.push(format!("--coverage.thresholds.lines={v}"));
            }
            if let Some(v) = thresholds.functions {
                tokens.push(format!("--coverage.thresholds.functions={v}"));
            }
            if let Some(v) = thresholds.branches {
                tokens.push(format!("--coverage.thresholds.branches={v}"));
            }
            if let Some(v) = thresholds.statements {
                tokens.push(format!("--coverage.thresholds.statements={v}"));
            }
        }
        if let Some(show) = cfg.coverage_show_code {
            tokens.push(format!(
                "--coverage.showCode={}",
                if show { "true" } else { "false" }
            ));
        }
        if let Some(detail) = cfg.coverage_detail.as_ref() {
            match detail {
                serde_json::Value::String(s) if s == "all" => {
                    tokens.push("--coverage.detail=all".to_string())
                }
                serde_json::Value::String(s) if s == "auto" => {
                    tokens.push("--coverage.detail=auto".to_string())
                }
                serde_json::Value::Number(n) if n.as_u64().is_some() => {
                    tokens.push(format!("--coverage.detail={}", n.as_u64().unwrap()))
                }
                _ => {}
            }
        }
    }

    let changed_from_cli = argv
        .iter()
        .find_map(|t| t.strip_prefix("--changed=").map(|s| s.to_string()))
        .or_else(|| {
            argv.iter()
                .position(|t| t == "--changed")
                .and_then(|idx| argv.get(idx + 1).cloned())
        });

    let (changed_obj, changed_mode_config) = match cfg.changed {
        Some(crate::config::ChangedConfig::Obj(ref obj)) => (Some(obj), None),
        Some(crate::config::ChangedConfig::Mode(mode)) => (None, Some(mode)),
        None => (cfg.changed_section.as_ref(), None),
    };

    let active_changed_mode = changed_from_cli
        .as_deref()
        .and_then(parse_changed_mode_string)
        .or(changed_mode_config);

    if let Some(mode) = active_changed_mode {
        let default_depth = changed_obj.and_then(|o| o.depth);
        let override_depth = changed_obj.and_then(|o| depth_for_mode(o, mode));
        let final_depth = override_depth.or(default_depth);
        if let Some(depth) = final_depth {
            tokens.push(format!("--changed.depth={depth}"));
        }
        if changed_from_cli.is_none() {
            tokens.push(format!("--changed={}", changed_mode_to_string(mode)));
        }
    }

    tokens
}

fn split_headlamp_tokens(tokens: &[String]) -> (Vec<String>, Vec<String>) {
    let mut hl: Vec<String> = vec![];
    let mut pass: Vec<String> = vec![];

    static HEADLAMP_FLAGS: Lazy<std::collections::HashSet<&'static str>> = Lazy::new(|| {
        [
            "--coverage",
            "--coverage.abortOnFailure",
            "--coverage-ui",
            "--coverageUi",
            "--coverage.detail",
            "--coverage.showCode",
            "--coverage.mode",
            "--coverage.compact",
            "--coverage.maxFiles",
            "--coverage.maxHotspots",
            "--coverage.thresholds.lines",
            "--coverage.thresholds.functions",
            "--coverage.thresholds.branches",
            "--coverage.thresholds.statements",
            "--coverage.pageFit",
            "--coverage.include",
            "--coverage.exclude",
            "--coverage.editor",
            "--coverage.root",
            "--onlyFailures",
            "--showLogs",
            "--sequential",
            "--watch",
            "--watchAll",
            "--ci",
            "--verbose",
            "--no-cache",
            "--noCache",
            "--bootstrapCommand",
            "--changed",
            "--changed.depth",
        ]
        .into_iter()
        .collect()
    });

    static TAKES_VALUE: Lazy<std::collections::HashSet<&'static str>> = Lazy::new(|| {
        [
            "--bootstrapCommand",
            "--coverage-ui",
            "--coverageUi",
            "--coverage.detail",
            "--coverage.showCode",
            "--coverage.mode",
            "--coverage.maxFiles",
            "--coverage.maxHotspots",
            "--coverage.thresholds.lines",
            "--coverage.thresholds.functions",
            "--coverage.thresholds.branches",
            "--coverage.thresholds.statements",
            "--coverage.pageFit",
            "--coverage.include",
            "--coverage.exclude",
            "--coverage.editor",
            "--coverage.root",
            "--changed",
            "--changed.depth",
        ]
        .into_iter()
        .collect()
    });

    static BOOL_FLAGS: Lazy<std::collections::HashSet<&'static str>> = Lazy::new(|| {
        [
            "--coverage",
            "--coverage.abortOnFailure",
            "--onlyFailures",
            "--showLogs",
            "--sequential",
            "--watch",
            "--watchAll",
            "--ci",
            "--verbose",
            "--no-cache",
            "--noCache",
            "--coverage.showCode",
            "--coverage.pageFit",
        ]
        .into_iter()
        .collect()
    });

    let is_headlamp = |t: &str| HEADLAMP_FLAGS.contains(base_flag(t));
    let takes_value = |t: &str| TAKES_VALUE.contains(base_flag(t));
    let is_bool_flag = |t: &str| BOOL_FLAGS.contains(base_flag(t));
    let is_bool_literal = |t: &str| {
        matches!(
            t.trim().to_ascii_lowercase().as_str(),
            "true" | "false" | "1" | "0"
        )
    };

    let mut i = 0usize;
    while i < tokens.len() {
        let tok = tokens[i].as_str();
        if tok == "--" {
            pass.extend(tokens[i..].iter().cloned());
            break;
        }
        if is_headlamp(tok) {
            hl.push(tokens[i].clone());
            if (takes_value(tok) || is_bool_flag(tok))
                && !tok.contains('=')
                && let Some(next) = tokens.get(i + 1)
            {
                if tok == "--changed" {
                    // `--changed` optionally consumes a value; if next looks like a flag, keep default.
                    if !next.starts_with('-') {
                        hl.push(next.clone());
                        i += 1;
                    }
                } else if is_bool_flag(tok) {
                    if is_bool_literal(next) {
                        hl.push(next.clone());
                        i += 1;
                    }
                } else if !next.starts_with('-') {
                    hl.push(next.clone());
                    i += 1;
                }
            };
        } else {
            pass.push(tokens[i].clone());
        }
        i += 1;
    }

    (hl, pass)
}

fn base_flag(t: &str) -> &str {
    t.split_once('=').map(|(k, _)| k).unwrap_or(t)
}

fn parse_coverage_ui(raw: &str) -> CoverageUi {
    match raw.trim().to_ascii_lowercase().as_str() {
        "jest" => CoverageUi::Jest,
        _ => CoverageUi::Both,
    }
}

fn parse_coverage_detail(raw: &str) -> Option<CoverageDetail> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "all" => Some(CoverageDetail::All),
        "auto" => Some(CoverageDetail::Auto),
        s => s.parse::<u32>().ok().map(CoverageDetail::Lines),
    }
}

fn parse_coverage_mode(raw: &str) -> CoverageMode {
    match raw.trim().to_ascii_lowercase().as_str() {
        "compact" => CoverageMode::Compact,
        "auto" => CoverageMode::Auto,
        _ => CoverageMode::Full,
    }
}

// NOTE: `parse_changed_mode_string` is still used for parsing the clap-collected `--changed` value.

fn parse_changed_mode_string(raw: &str) -> Option<ChangedMode> {
    Some(match raw.trim().to_ascii_lowercase().as_str() {
        "staged" => ChangedMode::Staged,
        "unstaged" => ChangedMode::Unstaged,
        "branch" => ChangedMode::Branch,
        "lastcommit" | "last_commit" | "last-commit" => ChangedMode::LastCommit,
        "all" | "" => ChangedMode::All,
        _ => return None,
    })
}

fn changed_mode_to_string(mode: ChangedMode) -> &'static str {
    match mode {
        ChangedMode::All => "all",
        ChangedMode::Staged => "staged",
        ChangedMode::Unstaged => "unstaged",
        ChangedMode::Branch => "branch",
        ChangedMode::LastCommit => "lastCommit",
    }
}

fn depth_for_mode(section: &crate::config::ChangedSection, mode: ChangedMode) -> Option<u32> {
    let key = changed_mode_to_string(mode);
    let v = section.per_mode.get(key)?;
    match v {
        serde_json::Value::Number(n) => n.as_u64().map(|u| u as u32),
        serde_json::Value::Object(map) => {
            map.get("depth").and_then(|d| d.as_u64()).map(|u| u as u32)
        }
        _ => None,
    }
}

fn is_test_like_token(candidate: &str) -> bool {
    let normalized = normalize_token_path_text(candidate);
    let lower = normalized.to_ascii_lowercase();
    TEST_LIKE_GLOBSET.is_match(Path::new(&lower))
}

fn is_path_like(candidate: &str) -> bool {
    let normalized = normalize_token_path_text(candidate);
    let normalized = normalized.as_ref();
    let has_sep = normalized.contains('/');
    let ext = Path::new(normalized)
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("");
    has_sep || matches!(ext, "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs")
}

fn infer_glob_from_selection_path(path_text: &str) -> String {
    let normalized = normalize_token_path_text(path_text);
    let path_text = normalized.as_ref();
    let p = Path::new(path_text);
    let is_dir = p.extension().is_none();
    if !is_dir {
        return path_text.to_string();
    }
    let base = path_text.trim_end_matches('/').to_string();
    if base.is_empty() {
        "**/*".to_string()
    } else {
        format!("{base}/**/*")
    }
}

fn normalize_token_path_text(candidate: &str) -> Cow<'_, str> {
    if candidate.contains('\\') {
        Cow::Owned(candidate.replace('\\', "/"))
    } else {
        Cow::Borrowed(candidate)
    }
}
