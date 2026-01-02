#[derive(Debug, Clone, Default)]
pub(super) struct HeadlampCli {
    pub(super) keep_artifacts: bool,
    pub(super) coverage: bool,
    pub(super) coverage_abort_on_failure: bool,
    pub(super) coverage_ui: Option<String>,
    pub(super) coverage_detail: Option<String>,
    pub(super) coverage_show_code: Option<bool>,
    pub(super) coverage_mode: Option<String>,
    pub(super) coverage_max_files: Option<u32>,
    pub(super) coverage_max_hotspots: Option<u32>,
    pub(super) coverage_thresholds_lines: Option<f64>,
    pub(super) coverage_thresholds_functions: Option<f64>,
    pub(super) coverage_thresholds_branches: Option<f64>,
    pub(super) coverage_thresholds_statements: Option<f64>,
    pub(super) coverage_page_fit: Option<bool>,
    pub(super) coverage_include: Vec<String>,
    pub(super) coverage_exclude: Vec<String>,
    pub(super) coverage_editor: Option<String>,
    pub(super) coverage_root: Option<String>,
    pub(super) only_failures: bool,
    pub(super) show_logs: bool,
    pub(super) sequential: bool,
    pub(super) watch: bool,
    pub(super) watch_all: bool,
    pub(super) ci: bool,
    pub(super) verbose: bool,
    pub(super) quiet: bool,
    pub(super) no_cache: bool,
    pub(super) bootstrap_command: Option<String>,
    pub(super) changed: Option<String>,
    pub(super) changed_depth: Option<u32>,
    pub(super) coverage_compact: bool,
    pub(super) dependency_language: Option<String>,
}

#[derive(Debug)]
pub(super) struct HeadlampCliParseError {
    message: String,
}

impl std::fmt::Display for HeadlampCliParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for HeadlampCliParseError {}

impl HeadlampCli {
    pub(super) fn parse_lenient(tokens: &[String]) -> Self {
        match Self::parse(tokens) {
            Ok(parsed) => parsed,
            Err(error) => {
                let _ = error.message;
                Self::default()
            }
        }
    }

    fn parse(tokens: &[String]) -> Result<Self, HeadlampCliParseError> {
        let mut parsed = HeadlampCli::default();
        let mut index = 0usize;
        while index < tokens.len() {
            let token = &tokens[index];
            let Some((raw_flag, raw_value)) = split_long_flag_token(token) else {
                index += 1;
                continue;
            };
            let flag = normalize_flag_name(raw_flag);
            let (next_token_text, has_next) = tokens
                .get(index + 1)
                .map(|t| (t.as_str(), true))
                .unwrap_or(("", false));
            let consumed_next =
                apply_flag_to_cli(&mut parsed, flag, raw_value, next_token_text, has_next)?;
            index += 1 + consumed_next;
        }
        Ok(parsed)
    }
}

fn apply_flag_to_cli(
    parsed: &mut HeadlampCli,
    flag: &str,
    raw_value: Option<&str>,
    next_token_text: &str,
    has_next: bool,
) -> Result<usize, HeadlampCliParseError> {
    if let Some(used_next) = apply_bool_flag(parsed, flag, raw_value, next_token_text, has_next)? {
        return Ok(used_next);
    }
    if let Some(used_next) =
        apply_bool_option_flag(parsed, flag, raw_value, next_token_text, has_next)?
    {
        return Ok(used_next);
    }
    if let Some(used_next) = apply_string_flag(parsed, flag, raw_value, next_token_text, has_next)?
    {
        return Ok(used_next);
    }
    if let Some(used_next) = apply_u32_flag(parsed, flag, raw_value, next_token_text, has_next)? {
        return Ok(used_next);
    }
    if let Some(used_next) = apply_f64_flag(parsed, flag, raw_value, next_token_text, has_next)? {
        return Ok(used_next);
    }
    if flag == "coverage-compact" {
        parsed.coverage_compact = true;
    }
    Ok(0)
}

fn apply_bool_flag(
    parsed: &mut HeadlampCli,
    flag: &str,
    raw_value: Option<&str>,
    next_token_text: &str,
    has_next: bool,
) -> Result<Option<usize>, HeadlampCliParseError> {
    let (value, used_next) = match flag {
        "keep-artifacts" => parse_bool_with_optional_value(raw_value, next_token_text, has_next)?,
        "coverage" => parse_bool_with_optional_value(raw_value, next_token_text, has_next)?,
        "coverage-abort-on-failure" => {
            parse_bool_with_optional_value(raw_value, next_token_text, has_next)?
        }
        "only-failures" => parse_bool_with_optional_value(raw_value, next_token_text, has_next)?,
        "show-logs" => parse_bool_with_optional_value(raw_value, next_token_text, has_next)?,
        "sequential" => parse_bool_with_optional_value(raw_value, next_token_text, has_next)?,
        "watch" => parse_bool_with_optional_value(raw_value, next_token_text, has_next)?,
        "watch-all" => parse_bool_with_optional_value(raw_value, next_token_text, has_next)?,
        "ci" => parse_bool_with_optional_value(raw_value, next_token_text, has_next)?,
        "verbose" => parse_bool_with_optional_value(raw_value, next_token_text, has_next)?,
        "quiet" => parse_bool_with_optional_value(raw_value, next_token_text, has_next)?,
        "no-cache" => parse_bool_with_optional_value(raw_value, next_token_text, has_next)?,
        _ => return Ok(None),
    };

    match flag {
        "keep-artifacts" => parsed.keep_artifacts = value,
        "coverage" => parsed.coverage = value,
        "coverage-abort-on-failure" => parsed.coverage_abort_on_failure = value,
        "only-failures" => parsed.only_failures = value,
        "show-logs" => parsed.show_logs = value,
        "sequential" => parsed.sequential = value,
        "watch" => parsed.watch = value,
        "watch-all" => parsed.watch_all = value,
        "ci" => parsed.ci = value,
        "verbose" => parsed.verbose = value,
        "quiet" => parsed.quiet = value,
        "no-cache" => parsed.no_cache = value,
        _ => {}
    }
    Ok(Some(used_next))
}

fn apply_bool_option_flag(
    parsed: &mut HeadlampCli,
    flag: &str,
    raw_value: Option<&str>,
    next_token_text: &str,
    has_next: bool,
) -> Result<Option<usize>, HeadlampCliParseError> {
    let (value, used_next) = match flag {
        "coverage-show-code" => {
            parse_bool_with_optional_value(raw_value, next_token_text, has_next)?
        }
        "coverage-page-fit" => {
            parse_bool_with_optional_value(raw_value, next_token_text, has_next)?
        }
        _ => return Ok(None),
    };
    match flag {
        "coverage-show-code" => parsed.coverage_show_code = Some(value),
        "coverage-page-fit" => parsed.coverage_page_fit = Some(value),
        _ => {}
    }
    Ok(Some(used_next))
}

fn apply_string_flag(
    parsed: &mut HeadlampCli,
    flag: &str,
    raw_value: Option<&str>,
    next_token_text: &str,
    has_next: bool,
) -> Result<Option<usize>, HeadlampCliParseError> {
    if flag == "changed" {
        let (value, used_next) =
            parse_optional_string_with_default(raw_value, next_token_text, has_next, "all");
        parsed.changed = Some(value);
        return Ok(Some(used_next));
    }

    let (value, used_next) = match flag {
        "coverage-ui" => parse_string_value(raw_value, next_token_text, has_next)?,
        "coverage-detail" => parse_string_value(raw_value, next_token_text, has_next)?,
        "coverage-mode" => parse_string_value(raw_value, next_token_text, has_next)?,
        "coverage-editor" => parse_string_value(raw_value, next_token_text, has_next)?,
        "coverage-root" => parse_string_value(raw_value, next_token_text, has_next)?,
        "bootstrap-command" => parse_string_value(raw_value, next_token_text, has_next)?,
        "dependency-language" => parse_string_value(raw_value, next_token_text, has_next)?,
        "coverage-include" => parse_string_value(raw_value, next_token_text, has_next)?,
        "coverage-exclude" => parse_string_value(raw_value, next_token_text, has_next)?,
        _ => return Ok(None),
    };

    match flag {
        "coverage-ui" => parsed.coverage_ui = Some(value),
        "coverage-detail" => parsed.coverage_detail = Some(value),
        "coverage-mode" => parsed.coverage_mode = Some(value),
        "coverage-editor" => parsed.coverage_editor = Some(value),
        "coverage-root" => parsed.coverage_root = Some(value),
        "bootstrap-command" => parsed.bootstrap_command = Some(value),
        "dependency-language" => parsed.dependency_language = Some(value),
        "coverage-include" => extend_comma_delimited(&mut parsed.coverage_include, &value),
        "coverage-exclude" => extend_comma_delimited(&mut parsed.coverage_exclude, &value),
        _ => {}
    }
    Ok(Some(used_next))
}

fn apply_u32_flag(
    parsed: &mut HeadlampCli,
    flag: &str,
    raw_value: Option<&str>,
    next_token_text: &str,
    has_next: bool,
) -> Result<Option<usize>, HeadlampCliParseError> {
    let (value, used_next) = match flag {
        "changed-depth" => parse_u32_value(raw_value, next_token_text, has_next)?,
        "coverage-max-files" => parse_u32_value(raw_value, next_token_text, has_next)?,
        "coverage-max-hotspots" => parse_u32_value(raw_value, next_token_text, has_next)?,
        _ => return Ok(None),
    };

    match flag {
        "changed-depth" => parsed.changed_depth = Some(value),
        "coverage-max-files" => parsed.coverage_max_files = Some(value),
        "coverage-max-hotspots" => parsed.coverage_max_hotspots = Some(value),
        _ => {}
    }
    Ok(Some(used_next))
}

fn apply_f64_flag(
    parsed: &mut HeadlampCli,
    flag: &str,
    raw_value: Option<&str>,
    next_token_text: &str,
    has_next: bool,
) -> Result<Option<usize>, HeadlampCliParseError> {
    let (value, used_next) = match flag {
        "coverage-thresholds-lines" => parse_f64_value(raw_value, next_token_text, has_next)?,
        "coverage-thresholds-functions" => parse_f64_value(raw_value, next_token_text, has_next)?,
        "coverage-thresholds-branches" => parse_f64_value(raw_value, next_token_text, has_next)?,
        "coverage-thresholds-statements" => parse_f64_value(raw_value, next_token_text, has_next)?,
        _ => return Ok(None),
    };

    match flag {
        "coverage-thresholds-lines" => parsed.coverage_thresholds_lines = Some(value),
        "coverage-thresholds-functions" => parsed.coverage_thresholds_functions = Some(value),
        "coverage-thresholds-branches" => parsed.coverage_thresholds_branches = Some(value),
        "coverage-thresholds-statements" => parsed.coverage_thresholds_statements = Some(value),
        _ => {}
    }
    Ok(Some(used_next))
}

fn split_long_flag_token(token: &str) -> Option<(&str, Option<&str>)> {
    let body = token.strip_prefix("--")?;
    let Some((flag, value)) = body.split_once('=') else {
        return Some((body, None));
    };
    Some((flag, Some(value)))
}

fn normalize_flag_name(flag: &str) -> &str {
    match flag {
        "keepArtifacts" => "keep-artifacts",
        "coverage.abortOnFailure" => "coverage-abort-on-failure",
        "coverageUi" => "coverage-ui",
        "coverage.detail" => "coverage-detail",
        "coverage.showCode" => "coverage-show-code",
        "coverage.mode" => "coverage-mode",
        "coverage.maxFiles" => "coverage-max-files",
        "coverage.maxHotspots" => "coverage-max-hotspots",
        "coverage.thresholds.lines" => "coverage-thresholds-lines",
        "coverage.thresholds.functions" => "coverage-thresholds-functions",
        "coverage.thresholds.branches" => "coverage-thresholds-branches",
        "coverage.thresholds.statements" => "coverage-thresholds-statements",
        "coverage.pageFit" => "coverage-page-fit",
        "coverage.include" => "coverage-include",
        "coverage.exclude" => "coverage-exclude",
        "coverage.editor" => "coverage-editor",
        "coverage.root" => "coverage-root",
        "onlyFailures" => "only-failures",
        "showLogs" => "show-logs",
        "watchAll" => "watch-all",
        "noCache" => "no-cache",
        "bootstrapCommand" => "bootstrap-command",
        "changed.depth" => "changed-depth",
        "dependencyLanguage" => "dependency-language",
        _ => flag,
    }
}

fn parse_bool_text(text: &str) -> Option<bool> {
    match text {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn parse_bool_with_optional_value(
    raw_value: Option<&str>,
    next_token_text: &str,
    has_next: bool,
) -> Result<(bool, usize), HeadlampCliParseError> {
    if let Some(value_text) = raw_value {
        return parse_bool_text(value_text)
            .map(|b| (b, 0))
            .ok_or_else(|| HeadlampCliParseError {
                message: format!("invalid bool value: {value_text}"),
            });
    }
    if has_next {
        if let Some(b) = parse_bool_text(next_token_text) {
            return Ok((b, 1));
        }
    }
    Ok((true, 0))
}

fn parse_string_value(
    raw_value: Option<&str>,
    next_token_text: &str,
    has_next: bool,
) -> Result<(String, usize), HeadlampCliParseError> {
    if let Some(value_text) = raw_value {
        return Ok((value_text.to_string(), 0));
    }
    if has_next && !next_token_text.starts_with("--") {
        return Ok((next_token_text.to_string(), 1));
    }
    Err(HeadlampCliParseError {
        message: "missing value".to_string(),
    })
}

fn parse_optional_string_with_default(
    raw_value: Option<&str>,
    next_token_text: &str,
    has_next: bool,
    default_value: &str,
) -> (String, usize) {
    if let Some(value_text) = raw_value {
        return (value_text.to_string(), 0);
    }
    if has_next && !next_token_text.starts_with("--") {
        return (next_token_text.to_string(), 1);
    }
    (default_value.to_string(), 0)
}

fn parse_u32_value(
    raw_value: Option<&str>,
    next_token_text: &str,
    has_next: bool,
) -> Result<(u32, usize), HeadlampCliParseError> {
    let (value_text, used_next) = parse_string_value(raw_value, next_token_text, has_next)?;
    let value: u32 = value_text.parse().map_err(|_| HeadlampCliParseError {
        message: format!("invalid u32 value: {value_text}"),
    })?;
    Ok((value, used_next))
}

fn parse_f64_value(
    raw_value: Option<&str>,
    next_token_text: &str,
    has_next: bool,
) -> Result<(f64, usize), HeadlampCliParseError> {
    let (value_text, used_next) = parse_string_value(raw_value, next_token_text, has_next)?;
    let value: f64 = value_text.parse().map_err(|_| HeadlampCliParseError {
        message: format!("invalid f64 value: {value_text}"),
    })?;
    Ok((value, used_next))
}

fn extend_comma_delimited(out: &mut Vec<String>, value: &str) {
    value
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .for_each(|s| out.push(s));
}
