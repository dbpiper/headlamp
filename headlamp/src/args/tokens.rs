use once_cell::sync::Lazy;

use crate::config::{CoverageMode, CoverageUi, HeadlampConfig};

use super::helpers::{
    base_flag, changed_mode_to_string, depth_for_mode, parse_changed_mode_string,
};

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

pub(crate) fn split_headlamp_tokens(tokens: &[String]) -> (Vec<String>, Vec<String>) {
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
