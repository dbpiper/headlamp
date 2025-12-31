use std::sync::LazyLock;

use crate::config::{CoverageMode, CoverageUi, HeadlampConfig};

use super::helpers::{
    base_flag, changed_mode_to_string, depth_for_mode, parse_changed_mode_string,
};

static HEADLAMP_FLAGS: LazyLock<std::collections::HashSet<&'static str>> = LazyLock::new(|| {
    [
        "--keep-artifacts",
        "--keepArtifacts",
        "--coverage",
        "--coverage-abort-on-failure",
        "--coverage.abortOnFailure",
        "--coverage-ui",
        "--coverageUi",
        "--coverage-detail",
        "--coverage.detail",
        "--coverage-show-code",
        "--coverage.showCode",
        "--coverage-mode",
        "--coverage.mode",
        "--coverage-compact",
        "--coverage.compact",
        "--coverage-max-files",
        "--coverage.maxFiles",
        "--coverage-max-hotspots",
        "--coverage.maxHotspots",
        "--coverage-thresholds-lines",
        "--coverage.thresholds.lines",
        "--coverage-thresholds-functions",
        "--coverage.thresholds.functions",
        "--coverage-thresholds-branches",
        "--coverage.thresholds.branches",
        "--coverage-thresholds-statements",
        "--coverage.thresholds.statements",
        "--coverage-page-fit",
        "--coverage.pageFit",
        "--coverage-include",
        "--coverage.include",
        "--coverage-exclude",
        "--coverage.exclude",
        "--coverage-editor",
        "--coverage.editor",
        "--coverage-root",
        "--coverage.root",
        "--only-failures",
        "--onlyFailures",
        "--show-logs",
        "--showLogs",
        "--sequential",
        "--watch",
        "--watch-all",
        "--watchAll",
        "--ci",
        "--verbose",
        "--no-cache",
        "--noCache",
        "--bootstrap-command",
        "--bootstrapCommand",
        "--changed",
        "--changed-depth",
        "--changed.depth",
        "--dependency-language",
        "--dependencyLanguage",
    ]
    .into_iter()
    .collect()
});

static TAKES_VALUE: LazyLock<std::collections::HashSet<&'static str>> = LazyLock::new(|| {
    [
        "--bootstrap-command",
        "--bootstrapCommand",
        "--coverage-ui",
        "--coverageUi",
        "--coverage-detail",
        "--coverage.detail",
        "--coverage-show-code",
        "--coverage.showCode",
        "--coverage-mode",
        "--coverage.mode",
        "--coverage-max-files",
        "--coverage.maxFiles",
        "--coverage-max-hotspots",
        "--coverage.maxHotspots",
        "--coverage-thresholds-lines",
        "--coverage.thresholds.lines",
        "--coverage-thresholds-functions",
        "--coverage.thresholds.functions",
        "--coverage-thresholds-branches",
        "--coverage.thresholds.branches",
        "--coverage-thresholds-statements",
        "--coverage.thresholds.statements",
        "--coverage-page-fit",
        "--coverage.pageFit",
        "--coverage-include",
        "--coverage.include",
        "--coverage-exclude",
        "--coverage.exclude",
        "--coverage-editor",
        "--coverage.editor",
        "--coverage-root",
        "--coverage.root",
        "--changed",
        "--changed-depth",
        "--changed.depth",
        "--dependency-language",
        "--dependencyLanguage",
    ]
    .into_iter()
    .collect()
});

static BOOL_FLAGS: LazyLock<std::collections::HashSet<&'static str>> = LazyLock::new(|| {
    [
        "--keep-artifacts",
        "--keepArtifacts",
        "--coverage",
        "--coverage-abort-on-failure",
        "--coverage.abortOnFailure",
        "--only-failures",
        "--onlyFailures",
        "--show-logs",
        "--showLogs",
        "--sequential",
        "--watch",
        "--watch-all",
        "--watchAll",
        "--ci",
        "--verbose",
        "--no-cache",
        "--noCache",
        "--coverage-show-code",
        "--coverage.showCode",
        "--coverage-page-fit",
        "--coverage.pageFit",
    ]
    .into_iter()
    .collect()
});

pub fn config_tokens(cfg: &HeadlampConfig, argv: &[String]) -> Vec<String> {
    let mut tokens: Vec<String> = vec![];
    append_basic_config_tokens(&mut tokens, cfg);
    append_coverage_config_tokens(&mut tokens, cfg, argv);
    append_changed_config_tokens(&mut tokens, cfg, argv);
    tokens
}

fn append_basic_config_tokens(tokens: &mut Vec<String>, cfg: &HeadlampConfig) {
    trimmed(cfg.bootstrap_command.as_deref())
        .into_iter()
        .for_each(|cmd| tokens.push(format!("--bootstrap-command={cmd}")));
    push_bool_flag(tokens, cfg.keep_artifacts == Some(true), "--keep-artifacts");
    push_bool_flag(tokens, cfg.sequential == Some(true), "--sequential");
    push_bool_flag(tokens, cfg.watch == Some(true), "--watch");
    push_bool_flag(tokens, cfg.ci == Some(true), "--ci");
    push_bool_flag(tokens, cfg.verbose == Some(true), "--verbose");
    push_bool_flag(tokens, cfg.no_cache == Some(true), "--no-cache");
    cfg.jest_args
        .as_ref()
        .filter(|a| !a.is_empty())
        .into_iter()
        .flat_map(|args| args.iter())
        .cloned()
        .for_each(|arg| tokens.push(arg));
}

fn append_coverage_config_tokens(tokens: &mut Vec<String>, cfg: &HeadlampConfig, argv: &[String]) {
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
    if !(coverage_always_on || argv_has_coverage) {
        return;
    }

    append_coverage_behavior_tokens(tokens, cfg, coverage_obj);
    append_coverage_threshold_tokens(tokens, coverage_obj);
    append_coverage_detail_token(tokens, cfg);
}

fn append_coverage_behavior_tokens(
    tokens: &mut Vec<String>,
    cfg: &HeadlampConfig,
    coverage_obj: Option<&crate::config::CoverageSection>,
) {
    let abort = coverage_obj
        .and_then(|o| o.abort_on_failure)
        .or(cfg.coverage_abort_on_failure);
    abort.into_iter().for_each(|v| {
        tokens.push(format!("--coverage-abort-on-failure={}", bool_str(v)));
    });

    let mode = coverage_obj.and_then(|o| o.mode).or(cfg.coverage_mode);
    mode.into_iter()
        .for_each(|m| tokens.push(format!("--coverage-mode={}", coverage_mode_str(m))));

    let page_fit = coverage_obj
        .and_then(|o| o.page_fit)
        .or(cfg.coverage_page_fit);
    page_fit
        .into_iter()
        .for_each(|v| tokens.push(format!("--coverage-page-fit={}", bool_str(v))));

    cfg.coverage_ui
        .into_iter()
        .for_each(|ui| tokens.push(format!("--coverage-ui={}", coverage_ui_str(ui))));
    trimmed(cfg.editor_cmd.as_deref())
        .into_iter()
        .for_each(|editor| tokens.push(format!("--coverage-editor={editor}")));
    cfg.include
        .as_ref()
        .filter(|v| !v.is_empty())
        .into_iter()
        .for_each(|include| tokens.push(format!("--coverage-include={}", include.join(","))));
    cfg.exclude
        .as_ref()
        .filter(|v| !v.is_empty())
        .into_iter()
        .for_each(|exclude| tokens.push(format!("--coverage-exclude={}", exclude.join(","))));
    cfg.coverage_max_files
        .into_iter()
        .for_each(|max_files| tokens.push(format!("--coverage-max-files={max_files}")));
    cfg.coverage_max_hotspots
        .into_iter()
        .for_each(|max_hotspots| tokens.push(format!("--coverage-max-hotspots={max_hotspots}")));
    cfg.coverage_show_code
        .into_iter()
        .for_each(|show| tokens.push(format!("--coverage-show-code={}", bool_str(show))));
}

fn append_coverage_threshold_tokens(
    tokens: &mut Vec<String>,
    coverage_obj: Option<&crate::config::CoverageSection>,
) {
    let Some(thresholds) = coverage_obj.and_then(|o| o.thresholds.as_ref()) else {
        return;
    };
    thresholds
        .lines
        .into_iter()
        .for_each(|v| tokens.push(format!("--coverage-thresholds-lines={v}")));
    thresholds.functions.into_iter().for_each(|v| {
        tokens.push(format!("--coverage-thresholds-functions={v}"));
    });
    thresholds.branches.into_iter().for_each(|v| {
        tokens.push(format!("--coverage-thresholds-branches={v}"));
    });
    thresholds.statements.into_iter().for_each(|v| {
        tokens.push(format!("--coverage-thresholds-statements={v}"));
    });
}

fn append_coverage_detail_token(tokens: &mut Vec<String>, cfg: &HeadlampConfig) {
    let Some(detail) = cfg.coverage_detail.as_ref() else {
        return;
    };
    match detail {
        serde_json::Value::String(s) if s == "all" => {
            tokens.push("--coverage-detail=all".to_string())
        }
        serde_json::Value::String(s) if s == "auto" => {
            tokens.push("--coverage-detail=auto".to_string())
        }
        serde_json::Value::Number(n) if n.as_u64().is_some() => {
            tokens.push(format!("--coverage-detail={}", n.as_u64().unwrap()))
        }
        _ => {}
    }
}

fn append_changed_config_tokens(tokens: &mut Vec<String>, cfg: &HeadlampConfig, argv: &[String]) {
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
    let Some(mode) = active_changed_mode else {
        return;
    };

    let default_depth = changed_obj.and_then(|o| o.depth);
    let override_depth = changed_obj.and_then(|o| depth_for_mode(o, mode));
    override_depth
        .or(default_depth)
        .into_iter()
        .for_each(|depth| tokens.push(format!("--changed-depth={depth}")));
    if changed_from_cli.is_none() {
        tokens.push(format!("--changed={}", changed_mode_to_string(mode)));
    }
}

fn push_bool_flag(tokens: &mut Vec<String>, should_push: bool, flag: &'static str) {
    if should_push {
        tokens.push(flag.to_string());
    }
}

fn trimmed(value: Option<&str>) -> Option<&str> {
    value.map(|s| s.trim()).filter(|s| !s.is_empty())
}

fn bool_str(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn coverage_mode_str(mode: CoverageMode) -> &'static str {
    match mode {
        CoverageMode::Compact => "compact",
        CoverageMode::Full => "full",
        CoverageMode::Auto => "auto",
    }
}

fn coverage_ui_str(ui: CoverageUi) -> &'static str {
    match ui {
        CoverageUi::Jest => "jest",
        CoverageUi::Both => "both",
    }
}

pub(crate) fn split_headlamp_tokens(tokens: &[String]) -> (Vec<String>, Vec<String>) {
    let mut hl: Vec<String> = vec![];
    let mut pass: Vec<String> = vec![];

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
