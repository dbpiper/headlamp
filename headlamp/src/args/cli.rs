use clap::Parser;

#[derive(Debug, Clone, Parser, Default)]
#[command(
    name = "headlamp",
    disable_help_flag = true,
    disable_version_flag = true
)]
pub(super) struct HeadlampCli {
    #[arg(
        long = "keep-artifacts",
        alias = "keepArtifacts",
        default_value_t = false,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    pub(super) keep_artifacts: bool,

    #[arg(
        long = "coverage",
        default_value_t = false,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    pub(super) coverage: bool,

    #[arg(
        long = "coverage-abort-on-failure",
        alias = "coverage.abortOnFailure",
        default_value_t = false,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    pub(super) coverage_abort_on_failure: bool,

    #[arg(long = "coverage-ui", alias = "coverageUi")]
    pub(super) coverage_ui: Option<String>,

    #[arg(long = "coverage-detail", alias = "coverage.detail")]
    pub(super) coverage_detail: Option<String>,

    #[arg(
        long = "coverage-show-code",
        alias = "coverage.showCode",
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    pub(super) coverage_show_code: Option<bool>,

    #[arg(long = "coverage-mode", alias = "coverage.mode")]
    pub(super) coverage_mode: Option<String>,

    #[arg(long = "coverage-max-files", alias = "coverage.maxFiles")]
    pub(super) coverage_max_files: Option<u32>,

    #[arg(long = "coverage-max-hotspots", alias = "coverage.maxHotspots")]
    pub(super) coverage_max_hotspots: Option<u32>,

    #[arg(
        long = "coverage-thresholds-lines",
        alias = "coverage.thresholds.lines"
    )]
    pub(super) coverage_thresholds_lines: Option<f64>,

    #[arg(
        long = "coverage-thresholds-functions",
        alias = "coverage.thresholds.functions"
    )]
    pub(super) coverage_thresholds_functions: Option<f64>,

    #[arg(
        long = "coverage-thresholds-branches",
        alias = "coverage.thresholds.branches"
    )]
    pub(super) coverage_thresholds_branches: Option<f64>,

    #[arg(
        long = "coverage-thresholds-statements",
        alias = "coverage.thresholds.statements"
    )]
    pub(super) coverage_thresholds_statements: Option<f64>,

    #[arg(
        long = "coverage-page-fit",
        alias = "coverage.pageFit",
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    pub(super) coverage_page_fit: Option<bool>,

    #[arg(
        long = "coverage-include",
        alias = "coverage.include",
        value_delimiter = ','
    )]
    pub(super) coverage_include: Vec<String>,

    #[arg(
        long = "coverage-exclude",
        alias = "coverage.exclude",
        value_delimiter = ','
    )]
    pub(super) coverage_exclude: Vec<String>,

    #[arg(long = "coverage-editor", alias = "coverage.editor")]
    pub(super) coverage_editor: Option<String>,

    #[arg(long = "coverage-root", alias = "coverage.root")]
    pub(super) coverage_root: Option<String>,

    #[arg(
        long = "only-failures",
        alias = "onlyFailures",
        default_value_t = false,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    pub(super) only_failures: bool,

    #[arg(
        long = "show-logs",
        alias = "showLogs",
        default_value_t = false,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    pub(super) show_logs: bool,

    #[arg(
        long = "sequential",
        default_value_t = false,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    pub(super) sequential: bool,

    #[arg(
        long = "watch",
        default_value_t = false,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    pub(super) watch: bool,

    #[arg(
        long = "watch-all",
        alias = "watchAll",
        default_value_t = false,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    pub(super) watch_all: bool,

    #[arg(
        long = "ci",
        default_value_t = false,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    pub(super) ci: bool,

    #[arg(
        long = "verbose",
        default_value_t = false,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    pub(super) verbose: bool,

    #[arg(
        long = "no-cache",
        alias = "noCache",
        default_value_t = false,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = clap::value_parser!(bool)
    )]
    pub(super) no_cache: bool,

    #[arg(long = "bootstrap-command", alias = "bootstrapCommand")]
    pub(super) bootstrap_command: Option<String>,

    #[arg(long = "changed", num_args = 0..=1, default_missing_value = "all")]
    pub(super) changed: Option<String>,

    #[arg(long = "changed-depth", alias = "changed.depth")]
    pub(super) changed_depth: Option<u32>,

    #[arg(
        long = "coverage-compact",
        alias = "coverage.compact",
        default_value_t = false
    )]
    pub(super) coverage_compact: bool,

    #[arg(long = "dependency-language", alias = "dependencyLanguage")]
    pub(super) dependency_language: Option<String>,
}
