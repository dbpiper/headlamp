use crate::config::{ChangedMode, CoverageMode, CoverageThresholds, CoverageUi};
use crate::selection::dependency_language::DependencyLanguageId;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedArgs {
    pub runner_args: Vec<String>,
    pub selection_paths: Vec<String>,
    pub selection_specified: bool,

    pub keep_artifacts: bool,

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
